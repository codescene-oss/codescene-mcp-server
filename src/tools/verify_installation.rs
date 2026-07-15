use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::json;

use crate::api_client;
use crate::auth::AuthCredential;
use crate::cli;
use crate::cli::CliRunner;
use crate::docker;
use crate::environment;
use crate::errors::CliError;
use crate::http::HttpClient;
use crate::tools::GitRepoParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: GitRepoParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    let project_root = docker::adapt_path_for_docker(Path::new(&params.git_repository_path));
    let credential = server
        .auth_manager
        .resolve_credential(&*server.cli_runner)
        .await
        .ok()
        .flatten();
    let ctx = CheckContext {
        cli_runner: &*server.cli_runner,
        http_client: &*server.http_client,
        is_standalone: server.is_standalone,
        credential: credential.as_ref(),
        target_label: api_url_label(credential.as_ref()),
        cli_timeout: TOKEN_CHECK_TIMEOUT,
    };
    let checks = run_all_checks(&project_root, &ctx).await;
    let text = format_results(&checks);
    server.track("verify-installation", json!({}));
    let text = server.maybe_version_warning(&text).await;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

struct CheckResult {
    name: &'static str,
    passed: bool,
    detail: String,
}

const TOKEN_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Context for running all installation checks.
struct CheckContext<'a> {
    cli_runner: &'a dyn CliRunner,
    http_client: &'a dyn HttpClient,
    is_standalone: bool,
    credential: Option<&'a AuthCredential>,
    /// Pre-computed user-friendly label for the API target.
    target_label: String,
    /// Timeout for CLI-based checks (token validation, connectivity).
    cli_timeout: std::time::Duration,
}

async fn run_all_checks(project_root: &str, ctx: &CheckContext<'_>) -> Vec<CheckResult> {
    let path = Path::new(project_root);
    let mut checks = vec![
        check_git_repository(path),
        check_token_via_cli(path, ctx).await,
        check_cli_connectivity(path, ctx).await,
    ];
    if !ctx.is_standalone {
        checks.push(check_api_connectivity(ctx).await);
    }
    checks.push(check_environment());
    checks
}

/// Keywords in CLI stderr that indicate a TLS / connectivity problem
/// rather than a "the CLI ran and auth succeeded but something else
/// failed" situation.
fn is_connectivity_error(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    // Cover OpenSSL, rustls, curl, and Java-style TLS diagnostics the
    // underlying CLI might emit.
    const KEYWORDS: &[&str] = &[
        "certificate",
        "ssl",
        "tls",
        "handshake",
        "connection refused",
        "could not resolve",
        "could not reach",
        "network",
        "timed out",
    ];
    KEYWORDS.iter().any(|kw| lower.contains(kw))
}

async fn check_token_via_cli(repo_path: &Path, ctx: &CheckContext<'_>) -> CheckResult {
    if matches!(ctx.credential, Some(AuthCredential::OAuth { .. })) {
        return CheckResult {
            name: "Access Token",
            passed: true,
            detail: "Authenticated via OAuth session.".to_string(),
        };
    }

    let token = std::env::var("CS_ACCESS_TOKEN")
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    if token.is_empty() {
        return CheckResult {
            name: "Access Token",
            passed: false,
            detail: "CS_ACCESS_TOKEN is not set or empty.".to_string(),
        };
    }
    let probe = find_probe_file(repo_path);
    let args: Vec<&str> = vec!["review", "--output-format=json", &probe];
    let cli_future = ctx.cli_runner.run(&args, Some(repo_path));
    match tokio::time::timeout(ctx.cli_timeout, cli_future).await {
        Err(_) => CheckResult {
            name: "Access Token",
            passed: false,
            detail: "Token check timed out after 30 s.".to_string(),
        },
        Ok(Ok(_)) => token_pass(),
        Ok(Err(CliError::LicenseCheckFailed { ref stderr })) if is_connectivity_error(stderr) => {
            CheckResult {
                name: "Access Token",
                passed: false,
                detail: format!(
                    "CLI could not connect to {} — possible TLS/network issue: {stderr}",
                    ctx.target_label
                ),
            }
        }
        Ok(Err(CliError::LicenseCheckFailed { .. })) => CheckResult {
            name: "Access Token",
            passed: false,
            detail: "Token is set but invalid or expired.".to_string(),
        },
        Ok(Err(CliError::NonZeroExit { stderr, .. })) if is_connectivity_error(&stderr) => {
            CheckResult {
                name: "Access Token",
                passed: false,
                detail: format!(
                    "CLI could not connect to {} — possible TLS/network issue: {stderr}",
                    ctx.target_label
                ),
            }
        }
        // Any other error (e.g. unsupported file) still means auth passed.
        Ok(Err(_)) => token_pass(),
    }
}

/// Check CLI connectivity by running a code health review.
///
/// Used for standalone users who don't have API access. The CLI still
/// connects to CodeScene to validate the license, so a broken CA cert
/// or network issue will surface here.
async fn check_cli_connectivity(repo_path: &Path, ctx: &CheckContext<'_>) -> CheckResult {
    let probe = find_probe_file(repo_path);
    let args: Vec<&str> = vec!["review", "--output-format=json", &probe];
    let cli_future = ctx.cli_runner.run(&args, Some(repo_path));
    match tokio::time::timeout(ctx.cli_timeout, cli_future).await {
        Err(_) => CheckResult {
            name: "CLI Connectivity",
            passed: false,
            detail: "CLI connectivity check timed out after 30 s.".to_string(),
        },
        Ok(Err(CliError::NonZeroExit { ref stderr, .. }))
        | Ok(Err(CliError::LicenseCheckFailed { ref stderr }))
            if is_connectivity_error(stderr) =>
        {
            CheckResult {
                name: "CLI Connectivity",
                passed: false,
                detail: format!(
                    "CLI could not connect to {} — possible TLS/network issue: {stderr}",
                    ctx.target_label
                ),
            }
        }
        // Success or any non-connectivity error means the CLI reached
        // CodeScene's servers — the TLS chain is valid.
        Ok(_) => CheckResult {
            name: "CLI Connectivity",
            passed: true,
            detail: format!("CLI connected to {} successfully.", ctx.target_label),
        },
    }
}

/// Check API connectivity by hitting the projects endpoint.
///
/// This exercises the full reqwest → TLS → API path, catching CA
/// certificate misconfiguration that the CLI check alone might miss
/// (since the CLI and the MCP server build their TLS stacks
/// independently).
async fn check_api_connectivity(ctx: &CheckContext<'_>) -> CheckResult {
    let Some(credential) = ctx.credential else {
        return CheckResult {
            name: "API Connectivity",
            passed: false,
            detail: "Skipped — no access token configured and not signed in via OAuth.".to_string(),
        };
    };
    match api_client::query_api_with_auth("v2/projects", ctx.http_client, Some(credential)).await {
        Ok(_) => CheckResult {
            name: "API Connectivity",
            passed: true,
            detail: format!("Connected to {} successfully.", ctx.target_label),
        },
        Err(e) => CheckResult {
            name: "API Connectivity",
            passed: false,
            detail: format!("Could not reach {}: {e}", ctx.target_label),
        },
    }
}

/// User-friendly label for the API target (on-prem URL or "CodeScene Cloud").
fn api_url_label(credential: Option<&AuthCredential>) -> String {
    crate::auth::resolve_web_root(credential).unwrap_or_else(|| "CodeScene Cloud".to_string())
}

/// Find a source file in the repo to use as a probe for the license
/// check.  Falls back to a non-existent path which still triggers the
/// license validation before the CLI reports "file not found".
fn find_probe_file(repo_path: &Path) -> String {
    for entry in repo_path.read_dir().into_iter().flatten() {
        if let Ok(e) = entry {
            let path = e.path();
            if path.is_file() {
                return path.to_string_lossy().to_string();
            }
        }
    }
    repo_path.join("__probe__.py").to_string_lossy().to_string()
}

fn token_pass() -> CheckResult {
    CheckResult {
        name: "Access Token",
        passed: true,
        detail: "Token is set and authenticated successfully.".to_string(),
    }
}

fn check_git_repository(path: &Path) -> CheckResult {
    match cli::find_git_root(path) {
        Some(root) => CheckResult {
            name: "Git Repository",
            passed: true,
            detail: format!("Found git root: {}", root.display()),
        },
        None => CheckResult {
            name: "Git Repository",
            passed: false,
            detail: format!("'{}' is not inside a git repository.", path.display()),
        },
    }
}

fn check_environment() -> CheckResult {
    CheckResult {
        name: "Runtime Environment",
        passed: true,
        detail: format!("Running as: {}", environment::detect()),
    }
}

fn format_results(checks: &[CheckResult]) -> String {
    let mut lines = Vec::with_capacity(checks.len() + 3);
    lines.push("## Installation Verification\n".to_string());
    for c in checks {
        let icon = if c.passed { "PASS" } else { "FAIL" };
        lines.push(format!("[{icon}] {}: {}", c.name, c.detail));
    }
    let total = checks.len();
    let passed = checks.iter().filter(|c| c.passed).count();
    lines.push(format!("\n{passed}/{total} checks passed."));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::http::tests::MockHttpClient;
    use crate::http::HttpResponse;
    use crate::tests::{clear_token, make_server_with_mocks, set_token, MockCliRunner};
    use crate::tools::GitRepoParam;

    use super::*;

    /// Test helper: build a CheckContext for check_token_via_cli / check_cli_connectivity tests.
    fn test_ctx_cli<'a>(
        cli_runner: &'a dyn CliRunner,
        credential: Option<&'a AuthCredential>,
    ) -> CheckContext<'a> {
        CheckContext {
            cli_runner,
            http_client: &crate::http::ReqwestClient,
            is_standalone: false,
            credential,
            target_label: "CodeScene Cloud".to_string(),
            cli_timeout: std::time::Duration::from_millis(100),
        }
    }

    /// Test helper: build a CheckContext for check_api_connectivity tests.
    fn test_ctx_http<'a>(
        http_client: &'a dyn HttpClient,
        cli_runner: &'a dyn CliRunner,
        credential: Option<&'a AuthCredential>,
    ) -> CheckContext<'a> {
        CheckContext {
            cli_runner,
            http_client,
            is_standalone: false,
            credential,
            target_label: "CodeScene Cloud".to_string(),
            cli_timeout: std::time::Duration::from_millis(100),
        }
    }

    fn repo_param(path: &str) -> GitRepoParam {
        GitRepoParam {
            git_repository_path: path.to_string(),
        }
    }

    const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

    const SSL_HANDSHAKE_STDERR: &str =
        "License check failed (https://codescene.example.com/api/v2/tool-license/cli):\n\
         error - javax.net.ssl.SSLHandshakeException: (certificate_unknown)\n\
         PKIX path building failed: unable to find valid certification path";

    fn mock_license_failure(stderr: &str) -> MockCliRunner {
        MockCliRunner::with_responses(vec![Err(CliError::LicenseCheckFailed {
            stderr: stderr.into(),
        })])
    }

    /// Mock CLI that returns OK for both the token check and the connectivity check.
    fn mock_cli_all_ok() -> MockCliRunner {
        MockCliRunner::with_responses(vec![Ok("{}".into()), Ok("{}".into())])
    }

    /// Assert a check's pass/fail state and that its detail mentions `needle`.
    fn assert_check(result: &CheckResult, passed: bool, needle: &str) {
        assert_eq!(result.passed, passed, "detail: {}", result.detail);
        assert!(
            result.detail.contains(needle),
            "expected detail to contain {needle:?}, got: {}",
            result.detail
        );
    }

    // -- check_token_via_cli -------------------------------------------------

    #[tokio::test]
    async fn token_missing_reports_fail() {
        let _g = clear_token();
        let cli = MockCliRunner::with_ok("");
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert!(!result.passed);
        assert!(result.detail.contains("not set"));
    }

    #[tokio::test]
    async fn token_valid_reports_pass() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_ok("{}");
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert!(result.passed);
        assert!(result.detail.contains("authenticated"));
    }

    #[tokio::test]
    async fn token_invalid_reports_fail() {
        let _g = set_token("bad");
        let cli = mock_license_failure("License check failed: [401]");
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert!(!result.passed);
        assert!(result.detail.contains("invalid or expired"));
    }

    #[tokio::test]
    async fn non_license_error_still_passes_auth() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(1, "no changes found");
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert!(result.passed);
    }

    #[tokio::test]
    async fn tls_error_from_cli_reports_connectivity_failure() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(
            1,
            "SSL certificate problem: unable to get local issuer certificate",
        );
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert_check(&result, false, "TLS/network");
    }

    #[tokio::test]
    async fn connection_refused_from_cli_reports_connectivity_failure() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(1, "connection refused");
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert_check(&result, false, "TLS/network");
    }

    #[tokio::test]
    async fn license_check_with_ssl_handshake_reports_connectivity_failure() {
        let _g = set_token("tok");
        let cli = mock_license_failure(SSL_HANDSHAKE_STDERR);
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert_check(&result, false, "TLS/network");
        assert!(
            result.detail.contains("SSLHandshakeException"),
            "should include original error: {}",
            result.detail
        );
    }

    #[tokio::test]
    async fn license_check_with_unreachable_server_reports_connectivity_failure() {
        let _g = set_token("tok");
        let cli = mock_license_failure(
            "License check failed, could not reach CodeScene servers (https://wrong.ee/api/v2/tool-license/cli)"
        );
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert_check(&result, false, "TLS/network");
        assert!(
            result.detail.contains("could not reach"),
            "should include original error: {}",
            result.detail
        );
    }

    #[tokio::test]
    async fn token_check_timeout_reports_fail() {
        use crate::errors::CliError;

        struct HangingCli;

        #[async_trait::async_trait]
        impl CliRunner for HangingCli {
            async fn run(
                &self,
                _args: &[&str],
                _working_dir: Option<&Path>,
            ) -> Result<String, CliError> {
                tokio::time::sleep(std::time::Duration::from_secs(999)).await;
                Ok(String::new())
            }
        }

        let _g = set_token("tok");
        let result = check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&HangingCli, None)).await;
        assert!(!result.passed);
        assert!(result.detail.contains("timed out"));
    }

    #[tokio::test]
    async fn oauth_credential_reports_token_pass_without_env() {
        let _g = clear_token();
        let cli = MockCliRunner::with_responses(vec![]);
        let credential = AuthCredential::OAuth {
            access_token: "oau-token".to_string(),
            onprem_url: None,
        };
        let result =
            check_token_via_cli(Path::new("/tmp"), &test_ctx_cli(&cli, Some(&credential))).await;
        assert_check(&result, true, "OAuth");
        assert!(cli.calls().lock().unwrap().is_empty());
    }

    // -- check_api_connectivity ----------------------------------------------

    fn test_credential() -> AuthCredential {
        AuthCredential::Configured {
            access_token: "tok".to_string(),
            onprem_url: None,
        }
    }

    #[tokio::test]
    async fn api_connectivity_succeeds() {
        let _g = set_token("tok");
        let http = MockHttpClient::always(HttpResponse::ok(r#"[{"id":1}]"#));
        let cli = MockCliRunner::with_ok("");
        let cred = test_credential();
        let result = check_api_connectivity(&test_ctx_http(&http, &cli, Some(&cred))).await;
        assert_check(&result, true, "successfully");
    }

    #[tokio::test]
    async fn api_connectivity_fails_on_transport_error() {
        let _g = set_token("tok");
        let http = MockHttpClient::new(vec![]);
        let cli = MockCliRunner::with_ok("");
        let cred = test_credential();
        let result = check_api_connectivity(&test_ctx_http(&http, &cli, Some(&cred))).await;
        assert_check(&result, false, "Could not reach");
    }

    #[tokio::test]
    async fn api_connectivity_skipped_without_token() {
        let _g = clear_token();
        let http = MockHttpClient::new(vec![]);
        let cli = MockCliRunner::with_ok("");
        let result = check_api_connectivity(&test_ctx_http(&http, &cli, None)).await;
        assert_check(&result, false, "Skipped");
    }

    #[tokio::test]
    async fn api_connectivity_fails_on_auth_error() {
        let _g = set_token("bad-token");
        let http = MockHttpClient::always(HttpResponse::error(401, "Unauthorized"));
        let cli = MockCliRunner::with_ok("");
        let cred = AuthCredential::Configured {
            access_token: "bad-token".to_string(),
            onprem_url: None,
        };
        let result = check_api_connectivity(&test_ctx_http(&http, &cli, Some(&cred))).await;
        assert!(!result.passed);
        assert!(result.detail.contains("401"), "detail: {}", result.detail);
    }

    // -- is_connectivity_error -----------------------------------------------

    // -- check_cli_connectivity ----------------------------------------------

    #[tokio::test]
    async fn cli_connectivity_succeeds_on_ok() {
        let cli = MockCliRunner::with_ok("{}");
        let result = check_cli_connectivity(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert!(result.passed, "detail: {}", result.detail);
        assert!(
            result.detail.contains("successfully"),
            "detail: {}",
            result.detail
        );
    }

    #[tokio::test]
    async fn cli_connectivity_succeeds_on_non_tls_error() {
        let cli = MockCliRunner::with_err(1, "unsupported file type");
        let result = check_cli_connectivity(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert!(
            result.passed,
            "non-TLS error should still pass: {}",
            result.detail
        );
    }

    #[tokio::test]
    async fn cli_connectivity_fails_on_tls_error() {
        let cli = MockCliRunner::with_err(
            1,
            "SSL certificate problem: unable to get local issuer certificate",
        );
        let result = check_cli_connectivity(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert_check(&result, false, "TLS/network");
    }

    #[tokio::test]
    async fn cli_connectivity_fails_on_license_check_ssl_error() {
        let cli = mock_license_failure(SSL_HANDSHAKE_STDERR);
        let result = check_cli_connectivity(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert_check(&result, false, "TLS/network");
    }

    #[tokio::test]
    async fn cli_connectivity_fails_when_server_unreachable() {
        let cli = mock_license_failure(
            "License check failed, could not reach CodeScene servers (https://wrong.ee/api/v2/tool-license/cli)"
        );
        let result = check_cli_connectivity(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert_check(&result, false, "TLS/network");
    }

    #[tokio::test]
    async fn cli_connectivity_passes_on_plain_license_failure() {
        let cli = mock_license_failure("License check failed: [401] Unauthorized");
        let result = check_cli_connectivity(Path::new("/tmp"), &test_ctx_cli(&cli, None)).await;
        assert!(
            result.passed,
            "plain license failure (no TLS) should pass connectivity: {}",
            result.detail
        );
    }

    #[tokio::test]
    async fn cli_connectivity_fails_on_timeout() {
        struct HangingCli;
        #[async_trait::async_trait]
        impl CliRunner for HangingCli {
            async fn run(
                &self,
                _args: &[&str],
                _working_dir: Option<&Path>,
            ) -> Result<String, CliError> {
                tokio::time::sleep(std::time::Duration::from_secs(999)).await;
                Ok(String::new())
            }
        }
        let result =
            check_cli_connectivity(Path::new("/tmp"), &test_ctx_cli(&HangingCli, None)).await;
        assert!(!result.passed, "timeout should fail: {}", result.detail);
        assert!(
            result.detail.contains("timed out"),
            "detail: {}",
            result.detail
        );
    }

    // -- is_connectivity_error (keyword detection) ---------------------------

    #[test]
    fn detects_tls_keywords() {
        assert!(is_connectivity_error(
            "SSL certificate problem: unable to get local issuer certificate"
        ));
        assert!(is_connectivity_error("TLS handshake failed"));
        assert!(is_connectivity_error("certificate verify failed"));
        assert!(is_connectivity_error("connection refused"));
        assert!(is_connectivity_error("Could not resolve host: example.com"));
        assert!(is_connectivity_error(
            "could not reach CodeScene servers (https://wrong.ee/api/v2/tool-license/cli)"
        ));
        assert!(is_connectivity_error("network is unreachable"));
        assert!(is_connectivity_error("Operation timed out"));
    }

    #[test]
    fn ignores_non_connectivity_errors() {
        assert!(!is_connectivity_error("no changes found"));
        assert!(!is_connectivity_error("unsupported file type"));
        assert!(!is_connectivity_error("exit code 1"));
    }

    // -- check_git_repository ------------------------------------------------

    #[test]
    fn git_repo_found_in_project() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = check_git_repository(path);
        assert!(result.passed);
        assert!(result.detail.contains("git root"));
    }

    #[test]
    fn git_repo_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = check_git_repository(dir.path());
        assert!(!result.passed);
        assert!(result.detail.contains("not inside a git repository"));
    }

    // -- check_environment ---------------------------------------------------

    #[test]
    fn environment_check_returns_known_value() {
        let result = check_environment();
        assert!(result.passed);
        assert!(
            result.detail.contains("binary") || result.detail.contains("docker"),
            "unexpected: {}",
            result.detail
        );
    }

    // -- format_results ------------------------------------------------------

    #[test]
    fn format_results_counts_correctly() {
        let checks = vec![
            CheckResult {
                name: "A",
                passed: true,
                detail: "ok".to_string(),
            },
            CheckResult {
                name: "B",
                passed: false,
                detail: "fail".to_string(),
            },
        ];
        let output = format_results(&checks);
        assert!(output.contains("[PASS] A: ok"));
        assert!(output.contains("[FAIL] B: fail"));
        assert!(output.contains("1/2 checks passed"));
    }

    // -- find_probe_file -----------------------------------------------------

    #[test]
    fn probe_file_falls_back_when_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let probe = find_probe_file(dir.path());
        assert!(probe.ends_with("__probe__.py"));
    }

    #[test]
    fn probe_file_finds_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.py"), "x = 1").unwrap();
        let probe = find_probe_file(dir.path());
        assert!(probe.ends_with("hello.py"));
    }

    // -- api_url_label -------------------------------------------------------

    #[test]
    fn api_url_label_returns_cloud_when_no_credential() {
        assert_eq!(api_url_label(None), "CodeScene Cloud");
    }

    #[test]
    fn api_url_label_returns_onprem_from_credential() {
        let cred = AuthCredential::Configured {
            access_token: "tok".to_string(),
            onprem_url: Some("https://my-instance.com".to_string()),
        };
        assert_eq!(api_url_label(Some(&cred)), "https://my-instance.com");
    }

    // -- handle (integration) ------------------------------------------------

    #[tokio::test]
    async fn handle_returns_success_result() {
        let _g = set_token("tok");
        let server = make_server_with_mocks(
            false,
            mock_cli_all_ok(),
            MockHttpClient::always(HttpResponse::ok(r#"[{"id":1}]"#)),
        );
        let result = handle(&server, repo_param("/tmp")).await.unwrap();
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let text = crate::tests::result_text(&result);
        assert!(
            text.contains("CLI Connectivity"),
            "API mode should also run CLI check: {text}"
        );
        assert!(
            text.contains("API Connectivity"),
            "API mode should run API check: {text}"
        );
    }

    #[tokio::test]
    async fn handle_runs_cli_connectivity_for_standalone() {
        let _g = set_token("tok");
        let server = make_server_with_mocks(true, mock_cli_all_ok(), MockHttpClient::new(vec![]));
        let result = handle(&server, repo_param("/tmp")).await.unwrap();
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let text = crate::tests::result_text(&result);
        assert!(
            text.contains("CLI Connectivity"),
            "standalone should run CLI check: {text}"
        );
        assert!(
            !text.contains("API Connectivity"),
            "standalone should NOT run API check: {text}"
        );
    }
}

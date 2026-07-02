use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::json;

use crate::api_client;
use crate::auth;
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
    let _ = server.credentials.ensure_fresh().await;
    let project_root = docker::adapt_path_for_docker(Path::new(&params.git_repository_path));
    let checks = run_all_checks(&project_root, server).await;
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

async fn run_all_checks(project_root: &str, server: &CodeSceneServer) -> Vec<CheckResult> {
    let path = Path::new(project_root);
    let cli_runner = &*server.cli_runner;
    let mut checks = vec![
        check_git_repository(path),
        check_token_via_cli(path, cli_runner, &server.credentials, TOKEN_CHECK_TIMEOUT).await,
        check_cli_connectivity(path, cli_runner, TOKEN_CHECK_TIMEOUT).await,
    ];
    if !server.is_standalone {
        checks.push(check_api_connectivity(
            &*server.http_client,
            Some(server.credentials.as_ref()),
        )
        .await);
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

async fn check_token_via_cli(
    repo_path: &Path,
    cli_runner: &dyn CliRunner,
    credentials: &auth::CredentialResolver,
    timeout: std::time::Duration,
) -> CheckResult {
    let token = auth::access_token_from_env().unwrap_or_default();
    if token.is_empty() {
        return match credentials.auth_status_json().await {
            Ok(json) => {
                let state = oauth_status_state(&json).unwrap_or_else(|| "signed_out".to_string());
                CheckResult {
                    name: "Access Token",
                    passed: false,
                    detail: format!(
                        "Not signed in ({state}). Run: cs auth login --client mcp"
                    ),
                }
            }
            Err(err) => CheckResult {
                name: "Access Token",
                passed: false,
                detail: format!(
                    "CS_ACCESS_TOKEN is not set and OAuth status check failed: {err}. \
                     Run: cs auth login --client mcp"
                ),
            },
        };
    }
    // Use `review` on a known source file instead of `delta` because
    // `delta` performs heavyweight git operations that can hang on
    // Windows.  The license check runs before any analysis, so any
    // analysable file works.
    let probe = find_probe_file(repo_path);
    let args: Vec<&str> = vec!["review", "--output-format=json", &probe];
    let cli_future = cli_runner.run(&args, Some(repo_path));
    match tokio::time::timeout(timeout, cli_future).await {
        Err(_) => CheckResult {
            name: "Access Token",
            passed: false,
            detail: "Token check timed out after 30 s.".to_string(),
        },
        Ok(Ok(_)) => token_pass(),
        Ok(Err(CliError::LicenseCheckFailed { ref stderr }))
            if is_connectivity_error(stderr) =>
        {
            CheckResult {
                name: "Access Token",
                passed: false,
                detail: format!(
                    "CLI could not connect to {} — possible TLS/network issue: {stderr}",
                    api_url_label()
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
                    api_url_label()
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
async fn check_cli_connectivity(
    repo_path: &Path,
    cli_runner: &dyn CliRunner,
    timeout: std::time::Duration,
) -> CheckResult {
    let probe = find_probe_file(repo_path);
    let args: Vec<&str> = vec!["review", "--output-format=json", &probe];
    let cli_future = cli_runner.run(&args, Some(repo_path));
    match tokio::time::timeout(timeout, cli_future).await {
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
                    api_url_label()
                ),
            }
        }
        // Success or any non-connectivity error means the CLI reached
        // CodeScene's servers — the TLS chain is valid.
        Ok(_) => CheckResult {
            name: "CLI Connectivity",
            passed: true,
            detail: format!("CLI connected to {} successfully.", api_url_label()),
        },
    }
}

/// Check API connectivity by hitting the projects endpoint.
///
/// This exercises the full reqwest → TLS → API path, catching CA
/// certificate misconfiguration that the CLI check alone might miss
/// (since the CLI and the MCP server build their TLS stacks
/// independently).
async fn check_api_connectivity(
    http_client: &dyn HttpClient,
    credentials: Option<&auth::CredentialResolver>,
) -> CheckResult {
    if auth::access_token_from_env().is_none() {
        return CheckResult {
            name: "API Connectivity",
            passed: false,
            detail: "Skipped — no access token configured.".to_string(),
        };
    }
    match api_client::query_api_with_client("v2/projects", http_client, credentials).await {
        Ok(_) => CheckResult {
            name: "API Connectivity",
            passed: true,
            detail: format!("Connected to {} successfully.", api_url_label()),
        },
        Err(e) => CheckResult {
            name: "API Connectivity",
            passed: false,
            detail: format!("Could not reach {}: {e}", api_url_label()),
        },
    }
}

/// User-friendly label for the API target (on-prem URL or "CodeScene Cloud").
fn api_url_label() -> String {
    std::env::var("CS_ONPREM_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "CodeScene Cloud".to_string())
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

fn oauth_status_state(json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.get("state").and_then(|s| s.as_str()).map(str::to_string))
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
            detail: format!(
                "'{}' is not inside a git repository.",
                path.display()
            ),
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
    use std::sync::Arc;

    use crate::http::tests::MockHttpClient;
    use crate::http::HttpResponse;
    use crate::tests::{
        clear_token, make_server_with_mocks, set_token, MockCliRunner,
    };
    use crate::tools::GitRepoParam;

    use super::*;

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

    fn mock_credentials(cli: MockCliRunner) -> auth::CredentialResolver {
        auth::CredentialResolver::new(Arc::new(cli), false)
    }

    async fn assert_token_check_connectivity_failure(cli: MockCliRunner, error_fragment: &str) {
        let _g = set_token("tok");
        let creds = mock_credentials(MockCliRunner::with_ok(""));
        let result = check_token_via_cli(Path::new("/tmp"), &cli, &creds, TEST_TIMEOUT).await;
        assert!(!result.passed, "detail: {}", result.detail);
        assert!(result.detail.contains("TLS/network"), "detail: {}", result.detail);
        assert!(
            result.detail.contains(error_fragment),
            "should include original error: {}",
            result.detail
        );
    }

    struct HandleVerificationCase {
        standalone: bool,
        cli: MockCliRunner,
        http: MockHttpClient,
        must_contain: &'static [&'static str],
        must_not_contain: &'static [&'static str],
    }

    async fn run_handle_verification(case: HandleVerificationCase) {
        let _g = set_token("tok");
        let server = make_server_with_mocks(case.standalone, case.cli, case.http);
        let result = handle(&server, repo_param("/tmp")).await.unwrap();
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        let text = crate::tests::result_text(&result);
        for fragment in case.must_contain {
            assert!(text.contains(fragment), "expected '{fragment}' in: {text}");
        }
        for fragment in case.must_not_contain {
            assert!(!text.contains(fragment), "unexpected '{fragment}' in: {text}");
        }
    }

    // -- check_token_via_cli -------------------------------------------------

    #[tokio::test]
    async fn token_missing_reports_fail() {
        let _g = clear_token();
        let creds = mock_credentials(MockCliRunner::with_ok(r#"{"state":"signed_out"}"#));
        let cli = MockCliRunner::with_ok("");
        let result = check_token_via_cli(Path::new("/tmp"), &cli, &creds, TEST_TIMEOUT).await;
        assert!(!result.passed);
        assert!(result.detail.contains("cs auth login"));
    }

    #[tokio::test]
    async fn token_valid_reports_pass() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_ok("{}");
        let creds = mock_credentials(MockCliRunner::with_ok(""));
        let result = check_token_via_cli(Path::new("/tmp"), &cli, &creds, TEST_TIMEOUT).await;
        assert!(result.passed);
        assert!(result.detail.contains("authenticated"));
    }

    #[tokio::test]
    async fn token_invalid_reports_fail() {
        let _g = set_token("bad");
        let cli = mock_license_failure("License check failed: [401]");
        let creds = mock_credentials(MockCliRunner::with_ok(""));
        let result = check_token_via_cli(Path::new("/tmp"), &cli, &creds, TEST_TIMEOUT).await;
        assert!(!result.passed);
        assert!(result.detail.contains("invalid or expired"));
    }

    #[tokio::test]
    async fn non_license_error_still_passes_auth() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(1, "no changes found");
        let creds = mock_credentials(MockCliRunner::with_ok(""));
        let result = check_token_via_cli(Path::new("/tmp"), &cli, &creds, TEST_TIMEOUT).await;
        assert!(result.passed);
    }

    #[tokio::test]
    async fn tls_error_from_cli_reports_connectivity_failure() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(1, "SSL certificate problem: unable to get local issuer certificate");
        let creds = mock_credentials(MockCliRunner::with_ok(""));
        let result = check_token_via_cli(Path::new("/tmp"), &cli, &creds, TEST_TIMEOUT).await;
        assert!(!result.passed);
        assert!(result.detail.contains("TLS/network"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn connection_refused_from_cli_reports_connectivity_failure() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(1, "connection refused");
        let creds = mock_credentials(MockCliRunner::with_ok(""));
        let result = check_token_via_cli(Path::new("/tmp"), &cli, &creds, TEST_TIMEOUT).await;
        assert!(!result.passed);
        assert!(result.detail.contains("TLS/network"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn license_check_with_ssl_handshake_reports_connectivity_failure() {
        assert_token_check_connectivity_failure(
            mock_license_failure(SSL_HANDSHAKE_STDERR),
            "SSLHandshakeException",
        )
        .await;
    }

    #[tokio::test]
    async fn license_check_with_unreachable_server_reports_connectivity_failure() {
        assert_token_check_connectivity_failure(
            mock_license_failure(
                "License check failed, could not reach CodeScene servers (https://wrong.ee/api/v2/tool-license/cli)",
            ),
            "could not reach",
        )
        .await;
    }

    #[tokio::test]
    async fn token_check_timeout_reports_fail() {
        use crate::errors::CliError;

        struct HangingCli;

        #[async_trait::async_trait]
        impl CliRunner for HangingCli {
            async fn run(&self, _args: &[&str], _working_dir: Option<&Path>) -> Result<String, CliError> {
                tokio::time::sleep(std::time::Duration::from_secs(999)).await;
                Ok(String::new())
            }
        }

        let _g = set_token("tok");
        let short = std::time::Duration::from_millis(50);
        let creds = mock_credentials(MockCliRunner::with_ok(""));
        let result = check_token_via_cli(Path::new("/tmp"), &HangingCli, &creds, short).await;
        assert!(!result.passed);
        assert!(result.detail.contains("timed out"));
    }

    // -- check_api_connectivity ----------------------------------------------

    #[tokio::test]
    async fn api_connectivity_succeeds() {
        let _g = set_token("tok");
        let http = MockHttpClient::always(HttpResponse::ok(r#"[{"id":1}]"#));
        let result = check_api_connectivity(&http, None).await;
        assert!(result.passed);
        assert!(result.detail.contains("successfully"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn api_connectivity_fails_on_transport_error() {
        let _g = set_token("tok");
        let http = MockHttpClient::new(vec![]);
        let result = check_api_connectivity(&http, None).await;
        assert!(!result.passed);
        assert!(result.detail.contains("Could not reach"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn api_connectivity_skipped_without_token() {
        let _g = clear_token();
        let http = MockHttpClient::new(vec![]);
        let result = check_api_connectivity(&http, None).await;
        assert!(!result.passed);
        assert!(result.detail.contains("Skipped"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn api_connectivity_fails_on_auth_error() {
        let _g = set_token("bad-token");
        let http = MockHttpClient::always(HttpResponse::error(401, "Unauthorized"));
        let result = check_api_connectivity(&http, None).await;
        assert!(!result.passed);
        assert!(result.detail.contains("401"), "detail: {}", result.detail);
    }

    // -- is_connectivity_error -----------------------------------------------

    // -- check_cli_connectivity ----------------------------------------------

    #[tokio::test]
    async fn cli_connectivity_succeeds_on_ok() {
        let cli = MockCliRunner::with_ok("{}");
        let result = check_cli_connectivity(Path::new("/tmp"), &cli, TEST_TIMEOUT).await;
        assert!(result.passed, "detail: {}", result.detail);
        assert!(result.detail.contains("successfully"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn cli_connectivity_succeeds_on_non_tls_error() {
        let cli = MockCliRunner::with_err(1, "unsupported file type");
        let result = check_cli_connectivity(Path::new("/tmp"), &cli, TEST_TIMEOUT).await;
        assert!(result.passed, "non-TLS error should still pass: {}", result.detail);
    }

    #[tokio::test]
    async fn cli_connectivity_fails_on_tls_error() {
        let cli = MockCliRunner::with_err(1, "SSL certificate problem: unable to get local issuer certificate");
        let result = check_cli_connectivity(Path::new("/tmp"), &cli, TEST_TIMEOUT).await;
        assert!(!result.passed, "TLS error should fail: {}", result.detail);
        assert!(result.detail.contains("TLS/network"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn cli_connectivity_fails_on_license_check_ssl_error() {
        let cli = mock_license_failure(SSL_HANDSHAKE_STDERR);
        let result = check_cli_connectivity(Path::new("/tmp"), &cli, TEST_TIMEOUT).await;
        assert!(!result.passed, "should detect TLS in license error: {}", result.detail);
        assert!(result.detail.contains("TLS/network"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn cli_connectivity_fails_when_server_unreachable() {
        let cli = mock_license_failure(
            "License check failed, could not reach CodeScene servers (https://wrong.ee/api/v2/tool-license/cli)"
        );
        let result = check_cli_connectivity(Path::new("/tmp"), &cli, TEST_TIMEOUT).await;
        assert!(!result.passed, "unreachable server should fail connectivity: {}", result.detail);
        assert!(result.detail.contains("TLS/network"), "detail: {}", result.detail);
    }

    #[tokio::test]
    async fn cli_connectivity_passes_on_plain_license_failure() {
        let cli = mock_license_failure("License check failed: [401] Unauthorized");
        let result = check_cli_connectivity(Path::new("/tmp"), &cli, TEST_TIMEOUT).await;
        assert!(result.passed, "plain license failure (no TLS) should pass connectivity: {}", result.detail);
    }

    #[tokio::test]
    async fn cli_connectivity_fails_on_timeout() {
        struct HangingCli;
        #[async_trait::async_trait]
        impl CliRunner for HangingCli {
            async fn run(&self, _args: &[&str], _working_dir: Option<&Path>) -> Result<String, CliError> {
                tokio::time::sleep(std::time::Duration::from_secs(999)).await;
                Ok(String::new())
            }
        }
        let short = std::time::Duration::from_millis(50);
        let result = check_cli_connectivity(Path::new("/tmp"), &HangingCli, short).await;
        assert!(!result.passed, "timeout should fail: {}", result.detail);
        assert!(result.detail.contains("timed out"), "detail: {}", result.detail);
    }

    // -- is_connectivity_error (keyword detection) ---------------------------

    #[test]
    fn detects_tls_keywords() {
        assert!(is_connectivity_error("SSL certificate problem: unable to get local issuer certificate"));
        assert!(is_connectivity_error("TLS handshake failed"));
        assert!(is_connectivity_error("certificate verify failed"));
        assert!(is_connectivity_error("connection refused"));
        assert!(is_connectivity_error("Could not resolve host: example.com"));
        assert!(is_connectivity_error("could not reach CodeScene servers (https://wrong.ee/api/v2/tool-license/cli)"));
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
    fn api_url_label_returns_cloud_when_no_onprem() {
        let _lock = crate::config::lock_test_env();
        std::env::remove_var("CS_ONPREM_URL");
        assert_eq!(api_url_label(), "CodeScene Cloud");
    }

    #[test]
    fn api_url_label_returns_onprem_url() {
        let _lock = crate::config::lock_test_env();
        std::env::set_var("CS_ONPREM_URL", "https://my-instance.com");
        assert_eq!(api_url_label(), "https://my-instance.com");
        std::env::remove_var("CS_ONPREM_URL");
    }

    // -- handle (integration) ------------------------------------------------

    #[tokio::test]
    async fn handle_returns_success_result() {
        run_handle_verification(HandleVerificationCase {
            standalone: false,
            cli: mock_cli_all_ok(),
            http: MockHttpClient::always(HttpResponse::ok(r#"[{"id":1}]"#)),
            must_contain: &["CLI Connectivity", "API Connectivity"],
            must_not_contain: &[],
        })
        .await;
    }

    #[tokio::test]
    async fn handle_runs_cli_connectivity_for_standalone() {
        run_handle_verification(HandleVerificationCase {
            standalone: true,
            cli: mock_cli_all_ok(),
            http: MockHttpClient::new(vec![]),
            must_contain: &["CLI Connectivity"],
            must_not_contain: &["API Connectivity"],
        })
        .await;
    }
}

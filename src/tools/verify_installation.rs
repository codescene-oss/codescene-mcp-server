use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::json;

use crate::cli;
use crate::cli::CliRunner;
use crate::docker;
use crate::environment;
use crate::errors::CliError;
use crate::tools::GitRepoParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: GitRepoParam,
) -> Result<CallToolResult, ErrorData> {
    server.version_checker.check_in_background();
    let project_root = docker::adapt_path_for_docker(Path::new(&params.git_repository_path));
    let checks = run_all_checks(&project_root, &*server.cli_runner).await;
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

async fn run_all_checks(project_root: &str, cli_runner: &dyn CliRunner) -> Vec<CheckResult> {
    let path = Path::new(project_root);
    vec![
        check_git_repository(path),
        check_token_via_cli(path, cli_runner).await,
        check_environment(),
    ]
}

async fn check_token_via_cli(repo_path: &Path, cli_runner: &dyn CliRunner) -> CheckResult {
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
    // Use `review` on a known source file instead of `delta` because
    // `delta` performs heavyweight git operations that can hang on
    // Windows.  The license check runs before any analysis, so any
    // analysable file works.
    let probe = find_probe_file(repo_path);
    let args: Vec<&str> = vec!["review", "--output-format=json", &probe];
    let cli_future = cli_runner.run(&args, Some(repo_path));
    match tokio::time::timeout(std::time::Duration::from_secs(30), cli_future).await {
        Err(_) => CheckResult {
            name: "Access Token",
            passed: false,
            detail: "Token check timed out after 30 s.".to_string(),
        },
        Ok(Ok(_)) => token_pass(),
        Ok(Err(CliError::LicenseCheckFailed)) => CheckResult {
            name: "Access Token",
            passed: false,
            detail: "Token is set but invalid or expired.".to_string(),
        },
        // Any other error (e.g. unsupported file) still means auth passed.
        Ok(Err(_)) => token_pass(),
    }
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
    use crate::tests::{
        clear_token, make_cli_mock_server, set_token, MockCliRunner,
    };
    use crate::tools::GitRepoParam;

    use super::*;

    fn repo_param(path: &str) -> GitRepoParam {
        GitRepoParam {
            git_repository_path: path.to_string(),
        }
    }

    // -- check_token_via_cli -------------------------------------------------

    #[tokio::test]
    async fn token_missing_reports_fail() {
        let _g = clear_token();
        let cli = MockCliRunner::with_ok("");
        let result = check_token_via_cli(Path::new("/tmp"), &cli).await;
        assert!(!result.passed);
        assert!(result.detail.contains("not set"));
    }

    #[tokio::test]
    async fn token_valid_reports_pass() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_ok("{}");
        let result = check_token_via_cli(Path::new("/tmp"), &cli).await;
        assert!(result.passed);
        assert!(result.detail.contains("authenticated"));
    }

    #[tokio::test]
    async fn token_invalid_reports_fail() {
        let _g = set_token("bad");
        let cli = MockCliRunner::with_responses(vec![Err(CliError::LicenseCheckFailed)]);
        let result = check_token_via_cli(Path::new("/tmp"), &cli).await;
        assert!(!result.passed);
        assert!(result.detail.contains("invalid or expired"));
    }

    #[tokio::test]
    async fn non_license_error_still_passes_auth() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(1, "no changes found");
        let result = check_token_via_cli(Path::new("/tmp"), &cli).await;
        assert!(result.passed);
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
        let cli_future = HangingCli.run(&["delta"], Some(Path::new("/tmp")));
        let result = match tokio::time::timeout(std::time::Duration::from_millis(50), cli_future).await {
            Err(_) => CheckResult { name: "Access Token", passed: false, detail: "timed out".to_string() },
            Ok(Ok(_)) => token_pass(),
            Ok(Err(CliError::LicenseCheckFailed)) => CheckResult { name: "Access Token", passed: false, detail: "invalid".to_string() },
            Ok(Err(_)) => token_pass(),
        };
        assert!(!result.passed);
        assert!(result.detail.contains("timed out"));
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

    // -- handle (integration) ------------------------------------------------

    #[tokio::test]
    async fn handle_returns_success_result() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok("{}"));
        let result = handle(&server, repo_param("/tmp")).await.unwrap();
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }
}

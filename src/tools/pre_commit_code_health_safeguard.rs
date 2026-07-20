use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::delta;
use crate::docker;
use crate::event_properties;
use crate::tools::common::{run_delta, tool_error};
use crate::tools::validation::CliCheck;
use crate::tools::GitRepoParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: GitRepoParam,
) -> Result<CallToolResult, ErrorData> {
    if let Some(r) = server.require_token().await {
        return Ok(r);
    }
    server.version_checker.check_in_background();
    let repo_path = docker::adapt_path_for_docker(Path::new(&params.git_repository_path));
    let rp = Path::new(&repo_path);
    if let Err(e) = server.validator.run_checks(&[CliCheck::InsideGitRepo(rp)]) {
        server.track_validation_err("pre-commit-code-health-safeguard", &e);
        return Ok(tool_error(&e.message));
    }
    let result = run_delta(rp, None, &*server.cli_runner).await;
    match result {
        Ok(output) => {
            let parsed = delta::analyze_delta_output(&output);
            let result_str = serde_json::to_string(&parsed).unwrap_or_default();
            let props = event_properties::pre_commit_properties(
                Path::new(&params.git_repository_path),
                &result_str,
            );
            server.track("pre-commit-code-health-safeguard", props);
            let text = server.maybe_version_warning(&result_str).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err("pre-commit-code-health-safeguard", &e);
            Ok(tool_error(&format!("Error: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{
        assert_error_contains, assert_token_error, clear_token, make_cli_mock_server,
        make_failing_validator_server, make_server, set_token, MockCliRunner,
    };
    use crate::tools::GitRepoParam;

    async fn run_safeguard(cli: MockCliRunner) -> rmcp::model::CallToolResult {
        let server = make_cli_mock_server(cli);
        let params = GitRepoParam {
            git_repository_path: "/tmp/repo".to_string(),
        };
        server
            .pre_commit_code_health_safeguard(Parameters(params))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let params = GitRepoParam {
            git_repository_path: "/tmp/repo".to_string(),
        };
        let result = make_server(false)
            .pre_commit_code_health_safeguard(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn success_returns_parsed_delta() {
        let _g = set_token("tok");
        let result = run_safeguard(MockCliRunner::with_ok("--- file.rs\n+++ file.rs\n")).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }

    #[tokio::test]
    async fn error_returns_tool_error() {
        let _g = set_token("tok");
        let result = run_safeguard(MockCliRunner::with_err(1, "delta failed")).await;
        assert_error_contains(&result, "delta failed");
    }

    #[tokio::test]
    async fn validation_failure_returns_error() {
        let _g = set_token("tok");
        let server = make_failing_validator_server("not_a_git_repo", "Not inside a git repository");
        let params = GitRepoParam {
            git_repository_path: "/tmp/repo".to_string(),
        };
        let result = server
            .pre_commit_code_health_safeguard(Parameters(params))
            .await
            .unwrap();
        assert_error_contains(&result, "Not inside a git repository");
    }
}

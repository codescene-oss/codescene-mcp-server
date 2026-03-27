use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::delta;
use crate::docker;
use crate::event_properties;
use crate::tools::common::{run_delta, tool_error};
use crate::tools::ChangeSetParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: ChangeSetParam,
) -> Result<CallToolResult, ErrorData> {
    if let Some(r) = server.require_token() {
        return Ok(r);
    }
    server.version_checker.check_in_background();
    let repo_path = docker::adapt_path_for_docker(Path::new(&params.git_repository_path));
    let result = run_delta(
        Path::new(&repo_path),
        Some(&params.base_ref),
        &*server.cli_runner,
    )
    .await;
    match result {
        Ok(output) => {
            let parsed = delta::analyze_delta_output(&output);
            let result_str = serde_json::to_string(&parsed).unwrap_or_default();
            let props = event_properties::change_set_properties(
                Path::new(&params.git_repository_path),
                Path::new(&params.base_ref),
                &result_str,
            );
            server.track("analyze-change-set", props);
            let text = server.maybe_version_warning(&result_str).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err("analyze-change-set", &e.to_string());
            Ok(tool_error(&format!("Error: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{
        assert_error_contains, assert_token_error, clear_token, make_cli_mock_server, make_server,
        set_token, MockCliRunner,
    };
    use crate::tools::ChangeSetParam;

    async fn run_change_set(cli: MockCliRunner) -> rmcp::model::CallToolResult {
        let server = make_cli_mock_server(cli);
        let params = ChangeSetParam {
            base_ref: "main".to_string(),
            git_repository_path: "/tmp/repo".to_string(),
        };
        server.analyze_change_set(Parameters(params)).await.unwrap()
    }

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let params = ChangeSetParam {
            base_ref: "main".to_string(),
            git_repository_path: "/tmp/repo".to_string(),
        };
        let result = make_server(false)
            .analyze_change_set(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn success_returns_parsed_delta() {
        let _g = set_token("tok");
        let result = run_change_set(MockCliRunner::with_ok("--- file.rs\n+++ file.rs\n")).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }

    #[tokio::test]
    async fn error_returns_tool_error() {
        let _g = set_token("tok");
        let result = run_change_set(MockCliRunner::with_err(1, "delta failed")).await;
        assert_error_contains(&result, "delta failed");
    }
}

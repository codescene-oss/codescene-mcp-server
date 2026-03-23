use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::docker;
use crate::event_properties;
use crate::tools::common::{extract_score, run_review, tool_error};
use crate::tools::FilePathParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: FilePathParam,
) -> Result<CallToolResult, ErrorData> {
    if let Some(r) = server.require_token() {
        return Ok(r);
    }
    server.version_checker.check_in_background();
    let file_path = docker::adapt_path_for_docker(Path::new(&params.file_path));
    let result = run_review(Path::new(&file_path), &*server.cli_runner).await;
    match result {
        Ok(output) => {
            let score = extract_score(&output);
            let props = event_properties::score_properties(Path::new(&params.file_path), score);
            server.track("code-health-score", props);
            let msg = score
                .map(|s| format!("Code Health score: {s}"))
                .unwrap_or_else(|| "Could not determine Code Health score.".to_string());
            let text = server.maybe_version_warning(&msg).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err("code-health-score", &e.to_string());
            Ok(tool_error(&format!("Error: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{
        assert_error_contains, assert_success_contains, assert_token_error, clear_token,
        make_cli_mock_server, make_server, set_token, MockCliRunner,
    };
    use crate::tools::FilePathParam;

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let params = FilePathParam {
            file_path: "/tmp/f.rs".to_string(),
        };
        let result = make_server(false)
            .code_health_score(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn success_returns_score() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":8.5,"review":[]}"#));
        let params = FilePathParam {
            file_path: "/tmp/test.rs".to_string(),
        };
        let result = server.code_health_score(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "Code Health score: 8.5");
    }

    #[tokio::test]
    async fn no_score_in_output() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"review":[]}"#));
        let params = FilePathParam {
            file_path: "/tmp/test.rs".to_string(),
        };
        let result = server.code_health_score(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "Could not determine");
    }

    #[tokio::test]
    async fn error_returns_tool_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(1, "score failed"));
        let params = FilePathParam {
            file_path: "/tmp/test.rs".to_string(),
        };
        let result = server.code_health_score(Parameters(params)).await.unwrap();
        assert_error_contains(&result, "score failed");
    }
}

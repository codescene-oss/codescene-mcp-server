use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::docker;
use crate::event_properties;
use crate::tools::common::{run_review, tool_error};
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
    match &result {
        Ok(output) => {
            let props = event_properties::review_properties(Path::new(&params.file_path), output);
            server.track("code-health-review", props);
            let text = server.maybe_version_warning(output).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err("code-health-review", &e.to_string());
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
            .code_health_review(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn success_returns_cli_output() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":9.5,"review":[]}"#));
        let params = FilePathParam {
            file_path: "/tmp/test.rs".to_string(),
        };
        let result = server.code_health_review(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "9.5");
    }

    #[tokio::test]
    async fn error_returns_tool_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(1, "review failed"));
        let params = FilePathParam {
            file_path: "/tmp/test.rs".to_string(),
        };
        let result = server.code_health_review(Parameters(params)).await.unwrap();
        assert_error_contains(&result, "review failed");
    }
}

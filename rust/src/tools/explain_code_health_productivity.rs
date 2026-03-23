use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::json;

use crate::resources;
use crate::CodeSceneServer;

pub(crate) async fn handle(server: &CodeSceneServer) -> Result<CallToolResult, ErrorData> {
    if let Some(r) = server.require_token() {
        return Ok(r);
    }
    server.version_checker.check_in_background();
    server.track("explain-code-health-productivity", json!({}));
    let text = server.maybe_version_warning(resources::BUSINESS_CASE).await;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{clear_token, make_server, result_text, set_token};
    use crate::tools::OptionalContext;

    #[tokio::test]
    async fn returns_content_when_token_set() {
        let _g = set_token("test-token");
        let result = make_server(false)
            .explain_code_health_productivity(Parameters(OptionalContext { context: None }))
            .await
            .unwrap();
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn returns_token_error_when_missing() {
        let _g = clear_token();
        let result = make_server(false)
            .explain_code_health_productivity(Parameters(OptionalContext { context: None }))
            .await
            .unwrap();
        assert!(result_text(&result).contains("No access token configured"));
    }
}

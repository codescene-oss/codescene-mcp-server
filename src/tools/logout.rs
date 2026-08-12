use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::json;

use crate::auth;
use crate::tools::LogoutParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    _params: LogoutParam,
) -> Result<CallToolResult, ErrorData> {
    tracing::info!("starting CodeScene logout");
    let pat_configured = auth::configured_credential().is_some();
    match server.auth_manager.logout(&*server.cli_runner).await {
        Ok(()) => {
            if pat_configured {
                tracing::info!("OAuth session cleared; CS_ACCESS_TOKEN remains configured");
                server.track(
                    "auth-logout",
                    json!({"result": "pat_still_configured"}),
                );
                Ok(CallToolResult::success(vec![Content::text(
                    "Signed out of CodeScene OAuth. \
                     CS_ACCESS_TOKEN is still configured, so API tools may continue to work \
                     until you remove it via set_config or your MCP client environment.",
                )]))
            } else {
                tracing::info!("CodeScene logout succeeded");
                server.track("auth-logout", json!({"result": "success"}));
                Ok(CallToolResult::success(vec![Content::text(
                    "Successfully signed out of CodeScene.",
                )]))
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "CodeScene logout CLI failed; local OAuth state cleared");
            server.track("auth-logout", json!({"result": "error"}));
            let mut message = format!(
                "Local OAuth session cleared, but CLI logout reported an error: {e}\n\
                 You may also run: cs auth logout"
            );
            if pat_configured {
                message.push_str(
                    "\n\nCS_ACCESS_TOKEN is still configured. Remove it to fully stop using that credential.",
                );
            }
            Ok(CallToolResult::success(vec![Content::text(message)]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::MockHttpClient;
    use crate::test_utils::MockCliRunner;
    use crate::tests::{clear_token, make_server_with_mocks, result_text, set_token};

    fn params() -> LogoutParam {
        LogoutParam {}
    }

    const SIGNED_OUT_JSON: &str = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;

    #[tokio::test]
    async fn logout_succeeds_and_clears_oauth() {
        let _g = clear_token();
        std::env::set_var("CS_OAUTH_TOKEN", "oau-test");
        std::env::set_var("CS_OAUTH_EXPIRES_AT", "9999999999");
        let cli = MockCliRunner::with_ok(SIGNED_OUT_JSON);
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("Successfully signed out"), "got: {text}");
        assert!(std::env::var("CS_OAUTH_TOKEN").is_err());
        assert_eq!(
            std::env::var("CS_OAUTH_EXPIRES_AT").ok().as_deref(),
            Some("0")
        );
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
    }

    #[tokio::test]
    async fn logout_notes_pat_when_still_configured() {
        let _g = set_token("pat-still-here");
        let cli = MockCliRunner::with_ok(SIGNED_OUT_JSON);
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("CS_ACCESS_TOKEN is still configured"), "got: {text}");
    }

    #[tokio::test]
    async fn logout_reports_cli_error_but_clears_local_state() {
        let _g = clear_token();
        std::env::set_var("CS_OAUTH_TOKEN", "oau-stale");
        std::env::set_var("CS_OAUTH_EXPIRES_AT", "9999999999");
        let cli = MockCliRunner::with_err(1, "connection refused");
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("CLI logout reported an error"), "got: {text}");
        assert!(text.contains("cs auth logout"), "got: {text}");
        assert!(std::env::var("CS_OAUTH_TOKEN").is_err());
        assert_eq!(
            std::env::var("CS_OAUTH_EXPIRES_AT").ok().as_deref(),
            Some("0")
        );
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
    }

    #[tokio::test]
    async fn logout_cli_error_notes_pat_when_still_configured() {
        let _g = set_token("pat-still-here");
        let cli = MockCliRunner::with_err(1, "connection refused");
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("CLI logout reported an error"), "got: {text}");
        assert!(
            text.contains("CS_ACCESS_TOKEN is still configured"),
            "got: {text}"
        );
        assert!(
            text.contains("Remove it to fully stop using that credential"),
            "got: {text}"
        );
    }
}

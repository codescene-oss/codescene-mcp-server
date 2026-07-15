use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::json;

use crate::auth;
use crate::tools::LoginParam;
use crate::CodeSceneServer;

/// Attempt to reuse an existing session (token already stored by the CLI).
/// Returns `Some(result)` if the user is already signed in, `None` otherwise.
async fn try_existing_session(server: &CodeSceneServer) -> Option<CallToolResult> {
    server
        .auth_manager
        .current_token(&*server.cli_runner)
        .await
        .ok()??;
    server.track("auth-login", json!({"result": "already_signed_in"}));
    Some(CallToolResult::success(vec![Content::text(
        "Already signed in to CodeScene.",
    )]))
}

/// Run the interactive login flow (opens browser) and apply the result.
async fn run_and_apply_login(server: &CodeSceneServer) -> CallToolResult {
    match server.auth_manager.login(&*server.cli_runner).await {
        Ok(resp) if resp.is_signed_in() => {
            server.track("auth-login", json!({"result": "success"}));
            CallToolResult::success(vec![Content::text("Successfully signed in to CodeScene.")])
        }
        Ok(resp) => {
            server.track("auth-login", json!({"result": "failed"}));
            CallToolResult::success(vec![Content::text(format!(
                "Login did not complete. Status: {}",
                resp.status
            ))])
        }
        Err(e) => {
            server.track("auth-login", json!({"result": "error"}));
            CallToolResult::success(vec![Content::text(format!(
                "Login failed: {e}\n\n\
                 If your browser did not open automatically, you can sign in manually \
                 using the CodeScene CLI:\n  cs auth login"
            ))])
        }
    }
}

pub(crate) async fn handle(
    server: &CodeSceneServer,
    _params: LoginParam,
) -> Result<CallToolResult, ErrorData> {
    if auth::configured_credential().is_some() {
        server.track("auth-login", json!({"result": "already_configured"}));
        return Ok(CallToolResult::success(vec![Content::text(
            "CS_ACCESS_TOKEN is already configured. OAuth login is not needed.\n\
             To use OAuth instead, remove CS_ACCESS_TOKEN from your MCP client configuration.",
        )]));
    }

    if let Some(result) = try_existing_session(server).await {
        return Ok(result);
    }

    Ok(run_and_apply_login(server).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::MockHttpClient;
    use crate::test_utils::MockCliRunner;
    use crate::tests::{clear_token, make_server_with_mocks, result_text, set_token};

    fn params() -> LoginParam {
        LoginParam {}
    }

    const SIGNED_IN_JSON: &str = r#"{"status":"signed_in","access-token":"oau_test","api-url":"https://api.codescene.io/api"}"#;
    const SIGNED_OUT_JSON: &str = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;

    #[tokio::test]
    async fn returns_early_when_token_already_set() {
        let _g = set_token("existing-token");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_ok(""),
            MockHttpClient::new(vec![]),
        );
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(
            text.contains("CS_ACCESS_TOKEN is already configured"),
            "got: {text}"
        );
    }

    #[tokio::test]
    async fn returns_already_signed_in_when_token_fresh() {
        let _g = clear_token();
        let cli = MockCliRunner::with_responses(vec![Ok(SIGNED_IN_JSON.into())]);
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("Already signed in"), "got: {text}");
    }

    #[tokio::test]
    async fn runs_login_flow_when_no_existing_session() {
        let _g = clear_token();

        // Case 1: token check returns signed_out → proceeds to login
        let cli = MockCliRunner::with_responses(vec![
            Ok(SIGNED_OUT_JSON.into()),
            Ok(SIGNED_IN_JSON.into()),
        ]);
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("Successfully signed in"), "got: {text}");
        assert!(std::env::var("CS_ACCESS_TOKEN").is_err());

        // Case 2: token check errors → falls through to login
        let cli = MockCliRunner::with_responses(vec![
            Err(crate::errors::CliError::NonZeroExit {
                code: 1,
                stderr: "no credentials file".into(),
            }),
            Ok(SIGNED_IN_JSON.into()),
        ]);
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("Successfully signed in"), "got: {text}");
    }

    #[tokio::test]
    async fn shows_error_message_when_login_fails() {
        let _g = clear_token();
        let cli = MockCliRunner::with_responses(vec![
            Ok(SIGNED_OUT_JSON.into()),
            Err(crate::errors::CliError::NonZeroExit {
                code: 1,
                stderr: "timeout".into(),
            }),
        ]);
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("Login failed"), "got: {text}");
        assert!(text.contains("cs auth login"), "got: {text}");
    }

    #[tokio::test]
    async fn login_response_not_signed_in_reports_status() {
        let _g = clear_token();
        let incomplete_json = r#"{"status":"expired","access-token":null,"api-url":null}"#;
        let cli = MockCliRunner::with_responses(vec![
            Ok(SIGNED_OUT_JSON.into()),
            Ok(incomplete_json.into()),
        ]);
        let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
        let result = handle(&server, params()).await.unwrap();
        let text = result_text(&result);
        assert!(text.contains("expired"), "got: {text}");
    }
}

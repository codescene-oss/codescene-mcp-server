use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::json;

use crate::auth::{self, SwitchAccountResult, SwitchAccountStatus};
use crate::tools::SwitchAccountParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: SwitchAccountParam,
) -> Result<CallToolResult, ErrorData> {
    let account_id = params.account_id;
    if let Some(early) = early_switch_account_response(server, account_id) {
        return Ok(early);
    }

    tracing::info!(
        account_id,
        current_oauth_account_id = ?server.auth_manager.try_cached_oauth_account_id(),
        "starting CodeScene account switch"
    );
    match server
        .auth_manager
        .switch_account(&*server.cli_runner, account_id)
        .await
    {
        Ok(result) => Ok(success_response(server, result)),
        Err(e) => Ok(error_response(server, account_id, e)),
    }
}

fn early_switch_account_response(
    server: &CodeSceneServer,
    account_id: i64,
) -> Option<CallToolResult> {
    if account_id <= 0 {
        server.track(
            "auth-switch-account",
            json!({"result": "invalid_account_id"}),
        );
        return Some(CallToolResult::success(vec![Content::text(
            "account_id must be a positive integer.",
        )]));
    }
    if auth::configured_credential().is_some() {
        tracing::info!("skipping switch_account because CS_ACCESS_TOKEN is configured");
        server.track(
            "auth-switch-account",
            json!({"result": "already_configured"}),
        );
        return Some(CallToolResult::success(vec![Content::text(
            "CS_ACCESS_TOKEN is already configured. OAuth account switching is not available.\n\
             To use OAuth instead, remove CS_ACCESS_TOKEN from your MCP client configuration \
             or unset it from your shell environment.",
        )]));
    }
    None
}

fn success_response(server: &CodeSceneServer, result: SwitchAccountResult) -> CallToolResult {
    server.track(
        "auth-switch-account",
        json!({"result": result.status_str(), "account_id": result.account_id}),
    );
    CallToolResult::success(vec![Content::text(success_message(&result))])
}

fn success_message(result: &SwitchAccountResult) -> String {
    match result.status {
        SwitchAccountStatus::AlreadyOnAccount => {
            format!("Already signed in to CodeScene account {}.", result.account_id)
        }
        SwitchAccountStatus::ReusedSession => format!(
            "Switched to CodeScene account {} using a stored OAuth session.",
            result.account_id
        ),
        SwitchAccountStatus::SignedIn => format!(
            "Switched to CodeScene account {} (interactive sign-in completed).",
            result.account_id
        ),
    }
}

fn error_response(server: &CodeSceneServer, account_id: i64, error: String) -> CallToolResult {
    tracing::warn!(error = %error, account_id, "CodeScene account switch failed");
    server.track(
        "auth-switch-account",
        json!({"result": "error", "account_id": account_id}),
    );
    CallToolResult::success(vec![Content::text(format!(
        "Failed to switch CodeScene account: {error}\n\n\
         Tip: use switch_account (not set_config alone) to change Cloud accounts. \
         The first switch into an account may open a browser so the CLI can store \
         credentials for that account slot."
    ))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::now_epoch_secs;
    use crate::http::tests::MockHttpClient;
    use crate::test_utils::MockCliRunner;
    use crate::tests::{clear_token, make_server_with_mocks, result_text, set_token};
    use crate::CodeSceneServer;

    const SIGNED_OUT_JSON: &str = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;

    #[derive(Clone, Copy)]
    enum ExpectedOutcome {
        ReusedSession { account_id: i64 },
        AlreadyOnAccount { account_id: i64 },
        InteractiveSignIn { account_id: i64 },
        Error,
    }

    struct SessionCase {
        session_token: &'static str,
        session_account: i64,
        target_account: i64,
        expected: ExpectedOutcome,
    }

    fn params(account_id: i64) -> SwitchAccountParam {
        SwitchAccountParam { account_id }
    }

    fn signed_in_json(token: &str, account_id: i64) -> String {
        format!(
            r#"{{"status":"signed_in","access-token":"{token}","api-url":"https://api.codescene.io/api","expires-at":{},"account-id":{account_id}}}"#,
            now_epoch_secs() + 3600
        )
    }

    fn server_with_cli(cli: MockCliRunner) -> CodeSceneServer {
        make_server_with_mocks(false, cli, MockHttpClient::new(vec![]))
    }

    fn seed_oauth_session(token: &str, account_id: i64) {
        std::env::set_var("CS_OAUTH_TOKEN", token);
        std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
        std::env::set_var("CS_OAUTH_ACCOUNT_ID", account_id.to_string());
    }

    fn clear_oauth_session() {
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
        std::env::remove_var("CS_ACCOUNT_ID");
    }

    async fn switch_text(server: &CodeSceneServer, account_id: i64) -> String {
        result_text(&handle(server, params(account_id)).await.unwrap()).to_string()
    }

    fn assert_expected_text(text: &str, expected: ExpectedOutcome) {
        match expected {
            ExpectedOutcome::ReusedSession { account_id } => {
                assert!(
                    text.contains(&format!("Switched to CodeScene account {account_id}"))
                        && text.contains("stored OAuth session"),
                    "got: {text}"
                );
            }
            ExpectedOutcome::AlreadyOnAccount { account_id } => {
                assert!(
                    text.contains("Already signed in")
                        && text.contains(&account_id.to_string()),
                    "got: {text}"
                );
            }
            ExpectedOutcome::InteractiveSignIn { account_id } => {
                assert!(
                    text.contains("interactive sign-in")
                        && text.contains(&account_id.to_string()),
                    "got: {text}"
                );
            }
            ExpectedOutcome::Error => {
                assert!(
                    text.contains("Failed to switch CodeScene account")
                        && text.contains("switch_account"),
                    "got: {text}"
                );
            }
        }
    }

    async fn assert_switch_with_session(case: SessionCase, cli: MockCliRunner) {
        let _g = clear_token();
        seed_oauth_session(case.session_token, case.session_account);
        let text = switch_text(&server_with_cli(cli), case.target_account).await;
        assert_expected_text(&text, case.expected);
        clear_oauth_session();
    }

    #[tokio::test]
    async fn rejects_non_positive_account_id() {
        let _g = clear_token();
        let text = switch_text(&server_with_cli(MockCliRunner::with_ok("")), 0).await;
        assert!(text.contains("positive integer"), "got: {text}");
    }

    #[tokio::test]
    async fn rejects_when_pat_configured() {
        let _g = set_token("pat");
        let text = switch_text(&server_with_cli(MockCliRunner::with_ok("")), 42).await;
        assert!(text.contains("CS_ACCESS_TOKEN"), "got: {text}");
    }

    #[tokio::test]
    async fn oauth_session_switch_outcomes() {
        let cases = [
            (
                SessionCase {
                    session_token: "old",
                    session_account: 1,
                    target_account: 99,
                    expected: ExpectedOutcome::ReusedSession { account_id: 99 },
                },
                MockCliRunner::with_ok(&signed_in_json("slot", 99)),
            ),
            (
                SessionCase {
                    session_token: "tok",
                    session_account: 42,
                    target_account: 42,
                    expected: ExpectedOutcome::AlreadyOnAccount { account_id: 42 },
                },
                MockCliRunner::with_responses(vec![]),
            ),
            (
                SessionCase {
                    session_token: "old",
                    session_account: 1,
                    target_account: 77,
                    expected: ExpectedOutcome::InteractiveSignIn { account_id: 77 },
                },
                MockCliRunner::with_responses(vec![
                    Ok(SIGNED_OUT_JSON.to_string()),
                    Ok(signed_in_json("new", 77)),
                ]),
            ),
            (
                SessionCase {
                    session_token: "old",
                    session_account: 1,
                    target_account: 77,
                    expected: ExpectedOutcome::Error,
                },
                MockCliRunner::with_responses(vec![
                    Ok(SIGNED_OUT_JSON.to_string()),
                    Ok(SIGNED_OUT_JSON.to_string()),
                ]),
            ),
        ];
        for (case, cli) in cases {
            assert_switch_with_session(case, cli).await;
        }
    }
}

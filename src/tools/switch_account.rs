use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;
use serde_json::{json, Value};

use crate::auth::{
    self, accounts_list_json, list_accounts_payload, resolve_named_account, AccountNameMatch,
    CloudAccount, SwitchAccountResult, SwitchAccountStatus,
};
use crate::tools::SwitchAccountParam;
use crate::CodeSceneServer;

enum SwitchRequest {
    List,
    ById(i64),
    ByName(String),
}

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: SwitchAccountParam,
) -> Result<CallToolResult, ErrorData> {
    if let Some(early) = pat_block_response(server) {
        return Ok(early);
    }
    match parse_switch_request(&params) {
        Err(message) => Ok(invalid_account_id_response(server, message)),
        Ok(SwitchRequest::List) => list_accounts_response(server).await,
        Ok(SwitchRequest::ById(account_id)) => switch_by_id(server, account_id).await,
        Ok(SwitchRequest::ByName(name)) => switch_by_name(server, &name).await,
    }
}

fn parse_switch_request(params: &SwitchAccountParam) -> Result<SwitchRequest, &'static str> {
    if let Some(account_id) = params.account_id {
        if account_id <= 0 {
            return Err("account_id must be a positive integer.");
        }
        return Ok(SwitchRequest::ById(account_id));
    }
    match normalized_name(&params.name) {
        Some(name) => Ok(SwitchRequest::ByName(name.to_string())),
        None => Ok(SwitchRequest::List),
    }
}

fn normalized_name(name: &Option<String>) -> Option<&str> {
    name.as_deref().map(str::trim).filter(|value| !value.is_empty())
}

fn pat_block_response(server: &CodeSceneServer) -> Option<CallToolResult> {
    if auth::configured_credential().is_none() {
        return None;
    }
    tracing::info!("skipping switch_account because CS_ACCESS_TOKEN is configured");
    server.track(
        "auth-switch-account",
        json!({"result": "already_configured"}),
    );
    Some(CallToolResult::success(vec![Content::text(
        "CS_ACCESS_TOKEN is already configured. OAuth account switching is not available.\n\
         To use OAuth instead, remove CS_ACCESS_TOKEN from your MCP client configuration \
         or unset it from your shell environment.",
    )]))
}

fn invalid_account_id_response(server: &CodeSceneServer, message: &str) -> CallToolResult {
    server.track(
        "auth-switch-account",
        json!({"result": "invalid_account_id"}),
    );
    CallToolResult::success(vec![Content::text(message.to_string())])
}

async fn list_accounts_response(server: &CodeSceneServer) -> Result<CallToolResult, ErrorData> {
    match list_accounts_payload(
        &*server.cli_runner,
        server.auth_manager.try_cached_oauth_account_id(),
    )
    .await
    {
        Ok(payload) => {
            server.track("auth-switch-account", json!({"result": "listed"}));
            Ok(json_text_result(&payload))
        }
        Err(error) => Ok(list_error_response(server, error)),
    }
}

async fn switch_by_name(
    server: &CodeSceneServer,
    name: &str,
) -> Result<CallToolResult, ErrorData> {
    let (accounts, matched) = match resolve_named_account(&*server.cli_runner, name).await {
        Ok(result) => result,
        Err(error) => return Ok(list_error_response(server, error)),
    };
    match matched {
        AccountNameMatch::Unique(account) => switch_by_id(server, account.id).await,
        AccountNameMatch::None => Ok(name_match_error(
            server,
            format!(
                "No account matched \"{name}\". Ask the user to pick from the list, then retry \
                 with account_id or an exact name/slug."
            ),
            &accounts,
            "name_not_found",
        )),
        AccountNameMatch::Ambiguous(matched) => Ok(name_match_error(
            server,
            format!(
                "Multiple accounts matched \"{name}\". Ask the user to pick one, then retry \
                 with account_id."
            ),
            &matched,
            "ambiguous_name",
        )),
    }
}

fn name_match_error(
    server: &CodeSceneServer,
    error: String,
    accounts: &[CloudAccount],
    result: &str,
) -> CallToolResult {
    server.track("auth-switch-account", json!({"result": result}));
    let mut payload = accounts_list_json(
        accounts,
        server.auth_manager.try_cached_oauth_account_id(),
    );
    payload["error"] = json!(error);
    json_text_result(&payload)
}

fn list_error_response(server: &CodeSceneServer, error: String) -> CallToolResult {
    tracing::warn!(error = %error, "CodeScene account list failed");
    server.track("auth-switch-account", json!({"result": "list_error"}));
    CallToolResult::success(vec![Content::text(format!(
        "Failed to list CodeScene accounts: {error}\n\n\
         Tip: sign in with the login tool first. Listing accounts requires a Cloud OAuth session."
    ))])
}

fn json_text_result(payload: &Value) -> CallToolResult {
    CallToolResult::success(vec![Content::text(
        serde_json::to_string(payload).unwrap_or_default(),
    )])
}

async fn switch_by_id(
    server: &CodeSceneServer,
    account_id: i64,
) -> Result<CallToolResult, ErrorData> {
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
    const SAMPLE_ACCOUNTS_JSON: &str = r#"{"accounts":[{"id":139802,"name":"Martin Säfsten","type":"individual","role":"owner","authenticated":true},{"id":11,"name":"CodeScene Showcase","type":"org","role":"member","slug":"codescene-showcase","authenticated":true}]}"#;

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
        SwitchAccountParam {
            account_id: Some(account_id),
            name: None,
        }
    }

    fn name_params(name: &str) -> SwitchAccountParam {
        SwitchAccountParam {
            account_id: None,
            name: Some(name.to_string()),
        }
    }

    fn list_params() -> SwitchAccountParam {
        SwitchAccountParam {
            account_id: None,
            name: None,
        }
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
    async fn rejects_list_when_pat_configured() {
        let _g = set_token("pat");
        let text = result_text(
            &handle(&server_with_cli(MockCliRunner::with_ok("")), list_params())
                .await
                .unwrap(),
        )
        .to_string();
        assert!(text.contains("CS_ACCESS_TOKEN"), "got: {text}");
    }

    #[tokio::test]
    async fn lists_accounts_when_called_without_args() {
        let _g = clear_token();
        seed_oauth_session("tok", 11);
        let text = result_text(
            &handle(
                &server_with_cli(MockCliRunner::with_ok(SAMPLE_ACCOUNTS_JSON)),
                list_params(),
            )
            .await
            .unwrap(),
        )
        .to_string();
        let payload: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(payload["accounts"][1]["id"], 11);
        assert_eq!(payload["accounts"][1]["current"], true);
        assert_eq!(payload["accounts"][0]["current"], false);
        clear_oauth_session();
    }

    #[tokio::test]
    async fn switches_by_unique_name_and_slug() {
        let _g = clear_token();
        seed_oauth_session("old", 1);
        let cli = MockCliRunner::with_responses(vec![
            Ok(SAMPLE_ACCOUNTS_JSON.to_string()),
            Ok(signed_in_json("slot", 11)),
        ]);
        let text = result_text(&handle(&server_with_cli(cli), name_params("codescene-showcase")).await.unwrap())
            .to_string();
        assert!(
            text.contains("Switched to CodeScene account 11") && text.contains("stored OAuth session"),
            "got: {text}"
        );
        clear_oauth_session();
    }

    #[tokio::test]
    async fn unknown_name_returns_list_and_does_not_switch() {
        let _g = clear_token();
        let cli = MockCliRunner::with_ok(SAMPLE_ACCOUNTS_JSON);
        let calls = cli.calls();
        let text = result_text(
            &handle(&server_with_cli(cli), name_params("missing"))
                .await
                .unwrap(),
        )
        .to_string();
        let payload: Value = serde_json::from_str(&text).unwrap();
        assert!(
            payload["error"].as_str().unwrap().contains("No account matched"),
            "got: {text}"
        );
        assert_eq!(payload["accounts"].as_array().unwrap().len(), 2);
        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn account_id_switch_does_not_call_list_accounts() {
        let _g = clear_token();
        seed_oauth_session("tok", 42);
        let cli = MockCliRunner::with_responses(vec![]);
        let calls = cli.calls();
        let text = switch_text(&server_with_cli(cli), 42).await;
        assert!(text.contains("Already signed in"), "got: {text}");
        assert!(
            calls.lock().unwrap().is_empty(),
            "switch by id should not call list-accounts"
        );
        clear_oauth_session();
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

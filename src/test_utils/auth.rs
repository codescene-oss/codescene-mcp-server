use super::*;
use crate::environment;
use crate::token_missing_msg;

#[tokio::test]
async fn require_token_returns_error_when_missing() {
    let _g = clear_token();
    assert!(make_server(false).require_token().await.is_some());
}

#[tokio::test]
async fn require_token_returns_none_when_set() {
    let _g = set_token("token");
    assert!(make_server(false).require_token().await.is_none());
}

#[tokio::test]
async fn require_token_treats_whitespace_as_missing() {
    let _g = set_token("   ");
    assert!(make_server(false).require_token().await.is_some());
}

fn signed_out_server() -> CodeSceneServer {
    std::env::set_var("CS_OAUTH_EXPIRES_AT", "0"); // signed-out sentinel avoids a CLI call
    make_server_with_mocks(
        false,
        MockCliRunner::with_responses(vec![]),
        MockHttpClient::new(vec![]),
    )
}

fn assert_token_missing_message(result: &CallToolResult) {
    assert!(
        result_text(result).contains("access token"),
        "got: {}",
        result_text(result)
    );
}

#[tokio::test]
async fn require_token_returns_missing_message_when_no_credential_resolved() {
    let _g = clear_token();
    let result = signed_out_server().require_token().await.unwrap();
    assert_token_missing_message(&result);
    std::env::remove_var("CS_OAUTH_EXPIRES_AT");
}

#[tokio::test]
async fn require_token_returns_missing_message_when_cli_errors() {
    let _g = clear_token();
    let cli = MockCliRunner::with_responses(vec![Err(crate::errors::CliError::NotFound(
        "cs".to_string(),
    ))]);
    let server = make_server_with_mocks(false, cli, MockHttpClient::new(vec![]));
    let result = server.require_token().await.unwrap();
    assert_token_missing_message(&result);
}

#[tokio::test]
async fn resolve_auth_credential_returns_error_result_when_no_credential_resolved() {
    let _g = clear_token();
    let err = signed_out_server()
        .resolve_auth_credential()
        .await
        .unwrap_err();
    assert_token_missing_message(&err);
    std::env::remove_var("CS_OAUTH_EXPIRES_AT");
}

#[tokio::test]
async fn resolve_auth_credential_returns_ok_when_configured() {
    let _g = set_token("pat-token");
    let server = make_server_with_mocks(
        false,
        MockCliRunner::with_responses(vec![]),
        MockHttpClient::new(vec![]),
    );
    let credential = server.resolve_auth_credential().await.unwrap();
    assert_eq!(credential.access_token(), "pat-token");
}

#[test]
fn credential_source_reports_configured_and_oauth() {
    let configured = crate::auth::AuthCredential::Configured {
        access_token: "tok".to_string(),
        onprem_url: None,
    };
    let oauth = crate::auth::AuthCredential::OAuth {
        access_token: "tok".to_string(),
        onprem_url: None,
    };
    assert_eq!(crate::credential_source(&configured), "configured");
    assert_eq!(crate::credential_source(&oauth), "oauth");
}

#[test]
fn token_missing_msg_uses_docker_copy_when_forced() {
    let _docker = environment::force_docker(true);
    let msg = token_missing_msg();
    assert!(msg.contains("not available in Docker"));
    assert!(msg.contains("CS_ACCESS_TOKEN"));
    assert!(!msg.contains("Call the `login` tool"));
}

#[test]
fn token_missing_msg_uses_oauth_copy_outside_docker() {
    let _docker = environment::force_docker(false);
    let msg = token_missing_msg();
    assert!(msg.contains("Call the `login` tool"));
}

use super::*;
use crate::config::{self};
use crate::{
    dispatch_cli_action_with, display_version, fetch_cli_version, help_text, parse_cli_args,
    run_auth_flow_with, run_list_accounts_flow_with, run_logout_flow_with,
    run_switch_account_flow_with, CliAction,
};

#[test]
fn display_version_strips_mcp_prefix() {
    assert_eq!(display_version("MCP-1.2.3"), "1.2.3");
}

#[test]
fn display_version_keeps_plain_version() {
    assert_eq!(display_version("1.2.3"), "1.2.3");
}

#[test]
fn parse_cli_args_defaults_to_run_server() {
    let args: Vec<String> = vec![];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::RunServer));
}

#[test]
fn parse_cli_args_supports_help_short() {
    let args = vec!["-h".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::PrintHelp));
}

#[test]
fn parse_cli_args_supports_help_long() {
    let args = vec!["--help".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::PrintHelp));
}

#[test]
fn parse_cli_args_supports_version_short() {
    let args = vec!["-v".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    match action {
        CliAction::PrintVersion(v) => assert_eq!(v, "1.2.3"),
        _ => panic!("expected version action"),
    }
}

#[test]
fn parse_cli_args_supports_version_long() {
    let args = vec!["--version".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    match action {
        CliAction::PrintVersion(v) => assert_eq!(v, "1.2.3"),
        _ => panic!("expected version action"),
    }
}

#[test]
fn parse_cli_args_rejects_unknown_argument() {
    let args = vec!["--nope".to_string()];
    let err = parse_cli_args(&args, "MCP-1.2.3").unwrap_err();
    assert!(err.contains("Unknown argument"));
}

#[test]
fn parse_cli_args_rejects_multiple_arguments() {
    let args = vec!["--help".to_string(), "--version".to_string()];
    let err = parse_cli_args(&args, "MCP-1.2.3").unwrap_err();
    assert!(err.contains("Unexpected arguments"));
    assert!(err.contains("--help"));
    assert!(err.contains("--version"));
}

fn assert_unexpected_cli_args(args: &[&str], expected: &str) {
    let owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let err = parse_cli_args(&owned, "MCP-1.2.3").unwrap_err();
    assert!(
        err.contains("Unexpected arguments") && err.contains(expected),
        "args={args:?} got: {err}"
    );
}

#[test]
fn parse_cli_args_renders_cli_version_and_auth_tokens() {
    assert_unexpected_cli_args(&["--cli-version", "extra"], "--cli-version");
    assert_unexpected_cli_args(&["auth", "mystery"], "auth mystery");
}

#[test]
fn parse_cli_args_renders_logout_switch_and_list_tokens() {
    assert_unexpected_cli_args(&["logout", "now"], "logout now");
    assert_unexpected_cli_args(&["switch", "1"], "switch 1");
    assert_unexpected_cli_args(&["list-accounts", "all"], "list-accounts all");
}

#[test]
fn parse_cli_args_supports_cli_version() {
    let args = vec!["--cli-version".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::PrintCliVersion));
}

#[test]
fn parse_cli_args_supports_auth() {
    let args = vec!["auth".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::Auth));
}

#[test]
fn parse_cli_args_supports_logout() {
    let args = vec!["logout".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::Logout));
}

#[test]
fn parse_cli_args_supports_auth_logout() {
    let args = vec!["auth".to_string(), "logout".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::Logout));
}

#[test]
fn parse_cli_args_supports_auth_switch() {
    let args = vec!["auth".to_string(), "switch".to_string(), "42".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::SwitchAccount(42)));
}

#[test]
fn parse_cli_args_supports_auth_list_accounts() {
    let args = vec!["auth".to_string(), "list-accounts".to_string()];
    let action = parse_cli_args(&args, "MCP-1.2.3").unwrap();
    assert!(matches!(action, CliAction::ListAccounts));
}

#[test]
fn parse_cli_args_rejects_auth_switch_without_id() {
    let args = vec!["auth".to_string(), "switch".to_string()];
    let err = parse_cli_args(&args, "MCP-1.2.3").unwrap_err();
    assert!(err.contains("account id"), "got: {err}");
}

#[test]
fn parse_cli_args_rejects_non_positive_auth_switch_id() {
    let args = vec!["auth".to_string(), "switch".to_string(), "0".to_string()];
    let err = parse_cli_args(&args, "MCP-1.2.3").unwrap_err();
    assert!(err.contains("positive"), "got: {err}");
}

#[tokio::test]
async fn fetch_cli_version_returns_cli_output() {
    let runner = MockCliRunner::with_ok("cs version 1.5.0\n");
    let result = fetch_cli_version(&runner).await.unwrap();
    assert_eq!(result, "cs version 1.5.0\n");
}

#[tokio::test]
async fn fetch_cli_version_propagates_cli_error() {
    let runner = MockCliRunner::with_err(1, "not found");
    let result = fetch_cli_version(&runner).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn ensure_oauth_client_configured_sets_default_when_unset() {
    let _lock = config::lock_test_env();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("CS_CONFIG_DIR", dir.path().as_os_str());
    std::env::remove_var("CS_OAUTH_CLIENT");

    crate::ensure_oauth_client_configured().await;

    assert_eq!(
        std::env::var("CS_OAUTH_CLIENT").ok().as_deref(),
        Some("mcp")
    );

    std::env::remove_var("CS_OAUTH_CLIENT");
    std::env::remove_var("CS_CONFIG_DIR");
}

#[tokio::test]
async fn ensure_oauth_client_configured_leaves_existing_value_untouched() {
    let _lock = config::lock_test_env();
    std::env::set_var("CS_OAUTH_CLIENT", "custom-client");

    crate::ensure_oauth_client_configured().await;

    assert_eq!(
        std::env::var("CS_OAUTH_CLIENT").ok().as_deref(),
        Some("custom-client")
    );
    std::env::remove_var("CS_OAUTH_CLIENT");
}

#[tokio::test]
async fn ensure_oauth_client_configured_tolerates_persistence_failure() {
    let _lock = config::lock_test_env();
    let impossible = if cfg!(windows) {
        r"NUL\impossible"
    } else {
        "/dev/null/impossible"
    };
    std::env::set_var("CS_CONFIG_DIR", impossible);
    std::env::remove_var("CS_OAUTH_CLIENT");

    crate::ensure_oauth_client_configured().await;

    assert_eq!(
        std::env::var("CS_OAUTH_CLIENT").ok().as_deref(),
        Some("mcp")
    );
    std::env::remove_var("CS_OAUTH_CLIENT");
    std::env::remove_var("CS_CONFIG_DIR");
}

/// Owns CLI-flow test state and cleans it before releasing the shared test lock.
struct LogoutFlowEnv {
    _lock: std::sync::MutexGuard<'static, ()>,
    _dir: tempfile::TempDir,
}

impl LogoutFlowEnv {
    fn new() -> Self {
        let lock = config::lock_test_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CS_CONFIG_DIR", dir.path().as_os_str());
        std::env::set_var("CS_OAUTH_CLIENT", "mcp");
        std::env::remove_var("CS_ACCESS_TOKEN");
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_REFRESH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
        std::env::remove_var("CS_ACCOUNT_ID");
        Self {
            _lock: lock,
            _dir: dir,
        }
    }

    fn assert_signed_out_sentinel(&self) {
        assert_eq!(
            std::env::var("CS_OAUTH_EXPIRES_AT").ok().as_deref(),
            Some("0")
        );
    }
}

impl Drop for LogoutFlowEnv {
    fn drop(&mut self) {
        std::env::remove_var("CS_OAUTH_CLIENT");
        std::env::remove_var("CS_CONFIG_DIR");
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_REFRESH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
        std::env::remove_var("CS_ACCOUNT_ID");
    }
}

#[tokio::test]
async fn run_logout_flow_with_succeeds() {
    let env = LogoutFlowEnv::new();
    let runner =
        MockCliRunner::with_ok(r#"{"status":"signed_out","access-token":null,"api-url":null}"#);
    run_logout_flow_with(&runner).await.unwrap();
    env.assert_signed_out_sentinel();
}

#[tokio::test]
async fn run_switch_account_flow_with_succeeds() {
    let env = LogoutFlowEnv::new();
    // Seed via config file only — do not set CS_OAUTH_* in the process env
    // before run_switch_account_flow_with, or snapshot_client_env_vars would
    // treat them as client-owned and block later logout tests from clearing them.
    let config_path =
        std::path::PathBuf::from(std::env::var("CS_CONFIG_DIR").unwrap()).join("config.json");
    std::fs::write(
        &config_path,
        serde_json::json!({
            "instance_id": "test-switch-flow",
            "oauth_token": "tok",
            "oauth_expires_at": "9999999999",
            "oauth_account_id": "42",
        })
        .to_string(),
    )
    .unwrap();
    let runner = MockCliRunner::with_ok(
        r#"{"status":"signed_in","access-token":"tok","api-url":null,"account-id":42}"#,
    );
    run_switch_account_flow_with(&runner, 42).await.unwrap();
    let config = crate::config::load().unwrap();
    assert_eq!(
        config.values.get("account_id").map(String::as_str),
        Some("42")
    );
    assert_eq!(
        config.values.get("oauth_account_id").map(String::as_str),
        Some("42")
    );
    drop(env);
}

#[tokio::test]
async fn run_switch_account_flow_with_reports_error() {
    let env = LogoutFlowEnv::new();
    let config_path =
        std::path::PathBuf::from(std::env::var("CS_CONFIG_DIR").unwrap()).join("config.json");
    std::fs::write(
        &config_path,
        serde_json::json!({
            "instance_id": "test-switch-flow-err",
            "oauth_token": "tok",
            "oauth_expires_at": "9999999999",
            "oauth_account_id": "1",
        })
        .to_string(),
    )
    .unwrap();
    let signed_out = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
    let runner =
        MockCliRunner::with_responses(vec![Ok(signed_out.to_string()), Ok(signed_out.to_string())]);
    let err = run_switch_account_flow_with(&runner, 99).await.unwrap_err();
    assert!(
        err.to_string().contains("Account switch failed"),
        "got: {err}"
    );
    drop(env);
}

#[tokio::test]
async fn run_list_accounts_flow_with_ok_and_error_paths() {
    let env = LogoutFlowEnv::new();
    run_list_accounts_flow_with(&MockCliRunner::with_ok(
        r#"{"accounts":[{"id":11,"name":"CodeScene Showcase","type":"org","role":"member","authenticated":true}]}"#,
    ))
    .await
    .expect("list-accounts success path");
    let err = run_list_accounts_flow_with(&MockCliRunner::with_err(
        1,
        "Failed to list accounts (HTTP 401).",
    ))
    .await
    .expect_err("list-accounts error path");
    assert!(
        err.to_string().contains("List accounts failed"),
        "got: {err}"
    );
    drop(env);
}

#[tokio::test]
async fn dispatch_cli_action_with_run_server_and_help() {
    let env = LogoutFlowEnv::new();
    assert!(
        !dispatch_cli_action_with(CliAction::RunServer, &MockCliRunner::with_ok(""))
            .await
            .unwrap()
    );
    assert!(
        dispatch_cli_action_with(CliAction::PrintHelp, &MockCliRunner::with_ok(""))
            .await
            .unwrap()
    );
    drop(env);
}

#[tokio::test]
async fn dispatch_cli_action_with_version_actions() {
    let env = LogoutFlowEnv::new();
    assert!(dispatch_cli_action_with(
        CliAction::PrintVersion("1.2.3".to_string()),
        &MockCliRunner::with_ok(""),
    )
    .await
    .unwrap());
    assert!(dispatch_cli_action_with(
        CliAction::PrintCliVersion,
        &MockCliRunner::with_ok("cs version 1.0.39\n"),
    )
    .await
    .unwrap());
    drop(env);
}

#[tokio::test]
async fn dispatch_cli_action_with_auth_and_logout() {
    let signed_out = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
    let signed_in = r#"{"status":"signed_in","access-token":"oau_test","api-url":"https://api.codescene.io/api"}"#;

    let env = LogoutFlowEnv::new();
    assert!(dispatch_cli_action_with(
        CliAction::Auth,
        &MockCliRunner::with_responses(
            vec![Ok(signed_out.to_string()), Ok(signed_in.to_string()),]
        ),
    )
    .await
    .unwrap());
    drop(env);

    let env = LogoutFlowEnv::new();
    assert!(
        dispatch_cli_action_with(CliAction::Logout, &MockCliRunner::with_ok(signed_out),)
            .await
            .unwrap()
    );
    drop(env);
}

#[tokio::test]
async fn dispatch_cli_action_with_list_and_switch() {
    let env = LogoutFlowEnv::new();
    assert!(
        dispatch_cli_action_with(
            CliAction::ListAccounts,
            &MockCliRunner::with_ok(
                r#"{"accounts":[{"id":11,"name":"CodeScene Showcase","type":"org","role":"member","authenticated":true}]}"#,
            ),
        )
        .await
        .unwrap()
    );
    drop(env);

    let env = LogoutFlowEnv::new();
    let config_path =
        std::path::PathBuf::from(std::env::var("CS_CONFIG_DIR").unwrap()).join("config.json");
    std::fs::write(
        &config_path,
        serde_json::json!({
            "instance_id": "test-dispatch-switch",
            "oauth_token": "tok",
            "oauth_expires_at": "9999999999",
            "oauth_account_id": "42",
        })
        .to_string(),
    )
    .unwrap();
    assert!(dispatch_cli_action_with(
        CliAction::SwitchAccount(42),
        &MockCliRunner::with_responses(vec![]),
    )
    .await
    .unwrap());
    drop(env);
}

#[tokio::test]
async fn run_auth_flow_with_already_signed_in_signed_in_incomplete_and_error() {
    let signed_out = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
    let signed_in = r#"{"status":"signed_in","access-token":"oau_test","api-url":"https://api.codescene.io/api"}"#;

    let env = LogoutFlowEnv::new();
    let config_path =
        std::path::PathBuf::from(std::env::var("CS_CONFIG_DIR").unwrap()).join("config.json");
    std::fs::write(
        &config_path,
        serde_json::json!({
            "instance_id": "test-auth-already",
            "oauth_token": "tok",
            "oauth_expires_at": "9999999999",
            "oauth_account_id": "1",
        })
        .to_string(),
    )
    .unwrap();
    run_auth_flow_with(&MockCliRunner::with_responses(vec![]))
        .await
        .unwrap();
    drop(env);

    let env = LogoutFlowEnv::new();
    run_auth_flow_with(&MockCliRunner::with_responses(vec![
        Ok(signed_out.to_string()),
        Ok(signed_in.to_string()),
    ]))
    .await
    .unwrap();
    drop(env);

    let env = LogoutFlowEnv::new();
    let err = run_auth_flow_with(&MockCliRunner::with_responses(vec![
        Ok(signed_out.to_string()),
        Ok(signed_out.to_string()),
    ]))
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("Login did not complete"),
        "got: {err}"
    );
    drop(env);

    let env = LogoutFlowEnv::new();
    let err = run_auth_flow_with(&MockCliRunner::with_responses(vec![
        Ok(signed_out.to_string()),
        Err(crate::errors::CliError::NonZeroExit {
            code: 1,
            stderr: "timeout".into(),
        }),
    ]))
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Login failed"), "got: {err}");
    drop(env);
}

#[tokio::test]
async fn run_logout_flow_with_reports_cli_error() {
    let env = LogoutFlowEnv::new();
    let runner = MockCliRunner::with_err(1, "connection refused");
    let err = run_logout_flow_with(&runner).await.unwrap_err();
    assert!(err.to_string().contains("Logout failed"), "got: {err}");
    env.assert_signed_out_sentinel();
}

#[test]
fn help_text_contains_usage_info() {
    let text = help_text();
    assert!(text.contains("Usage:"));
    assert!(text.contains("--help"));
    assert!(text.contains("--version"));
    assert!(text.contains("auth"));
    assert!(text.contains("auth logout"));
    assert!(text.contains("auth switch"));
    assert!(text.contains("auth list-accounts"));
}

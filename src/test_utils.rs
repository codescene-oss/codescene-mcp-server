use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rmcp::model::CallToolResult;

use crate::auth::AuthManager;
use crate::cli::{self, CliRunner};
use crate::config::ConfigData;
use crate::errors::CliError;
use crate::http::tests::MockHttpClient;
use crate::http::{self};
use crate::tools::validation::{CliCheck, ValidationError, Validator};
use crate::version_checker::VersionChecker;
use crate::{CodeSceneServer, ServerDeps};

/// Mock validator that always passes. Use [`MockValidator::failing`] to
/// simulate validation failures in tests.
pub(crate) struct MockValidator {
    error: Option<ValidationError>,
}

impl MockValidator {
    pub(crate) fn passing() -> Self {
        Self { error: None }
    }

    pub(crate) fn failing(kind: &'static str, message: &str) -> Self {
        Self {
            error: Some(ValidationError {
                message: message.to_string(),
                kind,
                detail: None,
            }),
        }
    }
}

impl Validator for MockValidator {
    fn run_checks(&self, _checks: &[CliCheck<'_>]) -> Result<(), ValidationError> {
        match &self.error {
            Some(e) => Err(ValidationError {
                message: e.message.clone(),
                kind: e.kind,
                detail: e.detail.clone(),
            }),
            None => Ok(()),
        }
    }
}

pub(crate) struct TestMocks {
    pub(crate) cli: Arc<dyn cli::CliRunner>,
    pub(crate) http: Arc<dyn http::HttpClient>,
    pub(crate) validator: Arc<dyn Validator>,
}

pub(crate) fn test_deps(id: &str, is_standalone: bool, mocks: TestMocks) -> ServerDeps {
    ServerDeps {
        config_data: ConfigData {
            instance_id: Some(id.to_string()),
            values: HashMap::new(),
        },
        instance_id: id.to_string(),
        is_standalone,
        version_checker: VersionChecker::new("dev"),
        auth_manager: AuthManager::new(),
        cli_runner: mocks.cli,
        http_client: mocks.http,
        validator: mocks.validator,
    }
}

pub(crate) fn make_server(is_standalone: bool) -> CodeSceneServer {
    CodeSceneServer::new(test_deps(
        "test-instance",
        is_standalone,
        TestMocks {
            cli: Arc::new(cli::ProductionCliRunner),
            http: Arc::new(http::ReqwestClient),
            validator: Arc::new(MockValidator::passing()),
        },
    ))
}

pub(crate) fn result_text(result: &CallToolResult) -> &str {
    result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("")
}

pub(crate) async fn make_server_with_version(
    current: &str,
    latest: &str,
    is_outdated: bool,
) -> CodeSceneServer {
    let vc = VersionChecker::new(current);
    vc.set_cached_info(crate::version_checker::VersionInfo {
        latest: latest.to_string(),
        current: current.to_string(),
        is_outdated,
    })
    .await;
    CodeSceneServer::new(ServerDeps {
        config_data: ConfigData {
            instance_id: Some("test".to_string()),
            values: HashMap::new(),
        },
        instance_id: "test".to_string(),
        is_standalone: false,
        version_checker: vc,
        auth_manager: AuthManager::new(),
        cli_runner: Arc::new(cli::ProductionCliRunner),
        http_client: Arc::new(http::ReqwestClient),
        validator: Arc::new(MockValidator::passing()),
    })
}

pub(crate) struct TokenGuard<'a> {
    _lock: std::sync::MutexGuard<'a, ()>,
}

impl Drop for TokenGuard<'_> {
    fn drop(&mut self) {
        std::env::remove_var("CS_ACCESS_TOKEN");
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_REFRESH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
        std::env::remove_var("CS_ACCOUNT_ID");
        std::env::remove_var("CS_OAUTH_CLIENT");
    }
}

pub(crate) fn set_token(value: &str) -> TokenGuard<'static> {
    let lock = crate::config::lock_test_env();
    std::env::set_var("CS_ACCESS_TOKEN", value);
    TokenGuard { _lock: lock }
}

pub(crate) fn clear_token() -> TokenGuard<'static> {
    let lock = crate::config::lock_test_env();
    std::env::remove_var("CS_ACCESS_TOKEN");
    std::env::remove_var("CS_OAUTH_TOKEN");
    std::env::remove_var("CS_OAUTH_EXPIRES_AT");
    std::env::remove_var("CS_OAUTH_REFRESH_EXPIRES_AT");
    std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
    std::env::remove_var("CS_ACCOUNT_ID");
    std::env::remove_var("CS_OAUTH_CLIENT");
    TokenGuard { _lock: lock }
}

pub(crate) struct MockCliRunner {
    responses: Mutex<Vec<Result<String, CliError>>>,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl MockCliRunner {
    pub(crate) fn with_responses(responses: Vec<Result<String, CliError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn with_ok(output: &str) -> Self {
        Self::with_responses(vec![Ok(output.to_string())])
    }

    pub(crate) fn with_err(code: i32, stderr: &str) -> Self {
        Self::with_responses(vec![Err(CliError::NonZeroExit {
            code,
            stderr: stderr.to_string(),
        })])
    }

    pub(crate) fn calls(&self) -> Arc<Mutex<Vec<Vec<String>>>> {
        self.calls.clone()
    }

    pub(crate) fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl CliRunner for MockCliRunner {
    async fn run(&self, args: &[&str], _working_dir: Option<&Path>) -> Result<String, CliError> {
        self.calls
            .lock()
            .unwrap()
            .push(args.iter().map(|arg| arg.to_string()).collect());
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            return Err(CliError::NonZeroExit {
                code: 1,
                stderr: format!(
                    "MockCliRunner: no queued responses left (args: {})",
                    args.join(" ")
                ),
            });
        }
        responses.remove(0)
    }
}

pub(crate) fn make_server_with_mocks(
    is_standalone: bool,
    cli: MockCliRunner,
    http: MockHttpClient,
) -> CodeSceneServer {
    CodeSceneServer::new(test_deps(
        "test-mock",
        is_standalone,
        TestMocks {
            cli: Arc::new(cli),
            http: Arc::new(http),
            validator: Arc::new(MockValidator::passing()),
        },
    ))
}

pub(crate) fn make_cli_mock_server(cli: MockCliRunner) -> CodeSceneServer {
    make_server_with_mocks(false, cli, MockHttpClient::new(vec![]))
}

pub(crate) fn make_failing_validator_server(kind: &'static str, message: &str) -> CodeSceneServer {
    CodeSceneServer::new(test_deps(
        "test-validation",
        false,
        TestMocks {
            cli: Arc::new(MockCliRunner::with_responses(vec![])),
            http: Arc::new(MockHttpClient::new(vec![])),
            validator: Arc::new(MockValidator::failing(kind, message)),
        },
    ))
}

pub(crate) fn assert_success_contains(result: &CallToolResult, needle: &str) {
    assert!(
        result.is_error.is_none() || result.is_error == Some(false),
        "expected success, got error: {:?}",
        result_text(result),
    );
    assert!(
        result_text(result).contains(needle),
        "expected text to contain {:?}, got {:?}",
        needle,
        result_text(result),
    );
}

pub(crate) fn assert_error_contains(result: &CallToolResult, needle: &str) {
    assert_eq!(result.is_error, Some(true), "expected error result");
    assert!(
        result_text(result).contains(needle),
        "expected error text to contain {:?}, got {:?}",
        needle,
        result_text(result),
    );
}

pub(crate) fn assert_token_error(result: &CallToolResult) {
    assert!(
        result_text(result).contains("No access token configured"),
        "expected token error, got: {}",
        result_text(result),
    );
}

pub(crate) fn assert_standalone_error(result: &CallToolResult) {
    assert_eq!(result.is_error, Some(true), "expected is_error=true");
    assert!(
        result_text(result).contains("standalone"),
        "expected standalone mention in: {}",
        result_text(result),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::io;
    use std::sync::{Arc, Mutex};

    use rmcp::model::{ClientJsonRpcMessage, ServerJsonRpcMessage};
    use rmcp::service::RoleServer;
    use rmcp::service::ServerInitializeError;
    use rmcp::transport::Transport;
    use rmcp::ServerHandler;
    use serde_json::json;

    use super::*;
    use crate::config::{self, ConfigData};
    use crate::environment;
    use crate::server_handler::build_instructions;
    use crate::version_checker::VersionChecker;
    use crate::{
        display_version, fetch_cli_version, help_text, parse_cli_args, remove_docker_unsupported_tools,
        run_list_accounts_flow_with, run_logout_flow_with, run_switch_account_flow_with,
        token_missing_msg, CliAction,
        API_ONLY_TOOLS,
    };

    #[derive(Clone)]
    struct ScriptedTransport {
        incoming: Arc<Mutex<VecDeque<ClientJsonRpcMessage>>>,
        sent: Arc<Mutex<Vec<ServerJsonRpcMessage>>>,
    }

    impl ScriptedTransport {
        fn from_messages(messages: Vec<ClientJsonRpcMessage>) -> Self {
            Self {
                incoming: Arc::new(Mutex::new(VecDeque::from(messages))),
                sent: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl Transport<RoleServer> for ScriptedTransport {
        type Error = io::Error;

        fn send(
            &mut self,
            item: ServerJsonRpcMessage,
        ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
            let sent = self.sent.clone();
            async move {
                sent.lock().unwrap().push(item);
                Ok(())
            }
        }

        fn receive(
            &mut self,
        ) -> impl std::future::Future<Output = Option<ClientJsonRpcMessage>> + Send {
            let next = self.incoming.lock().unwrap().pop_front();
            std::future::ready(next)
        }

        fn close(&mut self) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send {
            std::future::ready(Ok(()))
        }
    }

    fn initialize_request_message() -> ClientJsonRpcMessage {
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "unit-test-client",
                    "version": "1.0.0"
                }
            }
        }))
        .unwrap()
    }

    fn initialized_notification_message() -> ClientJsonRpcMessage {
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .unwrap()
    }

    #[test]
    fn api_only_tools_has_expected_entries() {
        assert!(API_ONLY_TOOLS.contains(&"select_project"));
        assert!(API_ONLY_TOOLS.contains(&"code_ownership_for_path"));
        assert_eq!(API_ONLY_TOOLS.len(), 6);
    }

    #[test]
    fn new_api_mode_keeps_all_tools() {
        let server = make_server(false);
        assert!(!server.is_standalone);
    }

    #[test]
    fn new_standalone_mode_sets_flag() {
        let server = make_server(true);
        assert!(server.is_standalone);
    }

    #[test]
    fn new_stores_instance_id() {
        let server = make_server(false);
        assert_eq!(server.instance_id, "test-instance");
    }

    #[test]
    fn get_info_returns_server_name() {
        let server = make_server(false);
        assert_eq!(server.get_info().server_info.name, "codescene-mcp-server");
    }

    #[test]
    fn get_info_standalone_instructions_omit_api_tools() {
        let info = make_server(true).get_info();
        assert!(!info
            .instructions
            .as_deref()
            .unwrap_or("")
            .contains("select_project"));
    }

    #[test]
    fn get_info_api_instructions_include_api_tools() {
        let info = make_server(false).get_info();
        assert!(info
            .instructions
            .as_deref()
            .unwrap_or("")
            .contains("select_project"));
    }

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

    #[tokio::test]
    async fn require_token_returns_missing_message_when_no_credential_resolved() {
        let _g = clear_token();
        std::env::set_var("CS_OAUTH_EXPIRES_AT", "0"); // signed-out sentinel avoids a CLI call
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![]),
        );
        let result = server.require_token().await.unwrap();
        assert!(
            result_text(&result).contains("access token"),
            "got: {}",
            result_text(&result)
        );
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
        assert!(
            result_text(&result).contains("access token"),
            "got: {}",
            result_text(&result)
        );
    }

    #[tokio::test]
    async fn resolve_auth_credential_returns_error_result_when_no_credential_resolved() {
        let _g = clear_token();
        std::env::set_var("CS_OAUTH_EXPIRES_AT", "0"); // signed-out sentinel avoids a CLI call
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![]),
        );
        let err = server.resolve_auth_credential().await.unwrap_err();
        assert!(
            result_text(&err).contains("access token"),
            "got: {}",
            result_text(&err)
        );
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

    #[tokio::test]
    async fn login_dispatch_method_delegates_to_login_handler() {
        use rmcp::handler::server::wrapper::Parameters;

        let _g = set_token("existing-token");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![]),
        );
        let result = server
            .login(Parameters(crate::tools::LoginParam {}))
            .await
            .unwrap();
        assert!(
            result_text(&result).contains("CS_ACCESS_TOKEN is already configured"),
            "got: {}",
            result_text(&result)
        );
    }

    #[tokio::test]
    async fn logout_dispatch_method_delegates_to_logout_handler() {
        use rmcp::handler::server::wrapper::Parameters;

        let _g = clear_token();
        std::env::set_var("CS_OAUTH_TOKEN", "oau-dispatch");
        std::env::set_var("CS_OAUTH_EXPIRES_AT", "9999999999");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_ok(
                r#"{"status":"signed_out","access-token":null,"api-url":null}"#,
            ),
            MockHttpClient::new(vec![]),
        );
        let result = server
            .logout(Parameters(crate::tools::LogoutParam {}))
            .await
            .unwrap();
        assert!(
            result_text(&result).contains("Successfully signed out"),
            "got: {}",
            result_text(&result)
        );
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
    }

    #[tokio::test]
    async fn switch_account_dispatch_method_delegates_to_handler() {
        use rmcp::handler::server::wrapper::Parameters;

        let _g = clear_token();
        let expires_at = crate::auth::now_epoch_secs() + 3600;
        std::env::set_var("CS_OAUTH_TOKEN", "tok");
        std::env::set_var("CS_OAUTH_EXPIRES_AT", expires_at.to_string());
        std::env::set_var("CS_OAUTH_ACCOUNT_ID", "42");
        // Queue a signed-in response so a concurrent env race that forces a CLI
        // token lookup still completes instead of panicking on an empty mock.
        let signed_in = format!(
            r#"{{"status":"signed_in","access-token":"tok","api-url":"https://api.codescene.io/api","expires-at":{expires_at},"account-id":42}}"#
        );
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_ok(&signed_in),
            MockHttpClient::new(vec![]),
        );
        let result = server
            .switch_account(Parameters(crate::tools::SwitchAccountParam {
                account_id: Some(42),
                name: None,
            }))
            .await
            .unwrap();
        let text = result_text(&result);
        assert!(
            text.contains("Already signed in") || text.contains("Switched to CodeScene account 42"),
            "got: {text}"
        );
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
        std::env::remove_var("CS_ACCOUNT_ID");
    }

    /// Env setup for `run_logout_flow_with` tests. Keeps the tempdir alive until drop.
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
        let runner = MockCliRunner::with_ok(
            r#"{"status":"signed_out","access-token":null,"api-url":null}"#,
        );
        run_logout_flow_with(&runner).await.unwrap();
        env.assert_signed_out_sentinel();
    }

    #[tokio::test]
    async fn run_switch_account_flow_with_succeeds() {
        let env = LogoutFlowEnv::new();
        // Seed via config file only — do not set CS_OAUTH_* in the process env
        // before run_switch_account_flow_with, or snapshot_client_env_vars would
        // treat them as client-owned and block later logout tests from clearing them.
        let config_path = std::path::PathBuf::from(std::env::var("CS_CONFIG_DIR").unwrap())
            .join("config.json");
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
        let runner = MockCliRunner::with_responses(vec![]);
        run_switch_account_flow_with(&runner, 42).await.unwrap();
        assert_eq!(std::env::var("CS_ACCOUNT_ID").ok().as_deref(), Some("42"));
        drop(env);
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
        std::env::remove_var("CS_ACCOUNT_ID");
    }

    #[tokio::test]
    async fn run_switch_account_flow_with_reports_error() {
        let env = LogoutFlowEnv::new();
        let config_path = std::path::PathBuf::from(std::env::var("CS_CONFIG_DIR").unwrap())
            .join("config.json");
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
        let runner = MockCliRunner::with_responses(vec![
            Ok(signed_out.to_string()),
            Ok(signed_out.to_string()),
        ]);
        let err = run_switch_account_flow_with(&runner, 99).await.unwrap_err();
        assert!(
            err.to_string().contains("Account switch failed"),
            "got: {err}"
        );
        drop(env);
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
        std::env::remove_var("CS_ACCOUNT_ID");
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
    async fn run_logout_flow_with_reports_cli_error() {
        let env = LogoutFlowEnv::new();
        let runner = MockCliRunner::with_err(1, "connection refused");
        let err = run_logout_flow_with(&runner).await.unwrap_err();
        assert!(
            err.to_string().contains("Logout failed"),
            "got: {err}"
        );
        env.assert_signed_out_sentinel();
    }

    #[tokio::test]
    async fn verify_installation_dispatch_method_delegates_to_handler() {
        use rmcp::handler::server::wrapper::Parameters;

        let _g = set_token("existing-token");
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![Ok("{}".to_string()), Ok("{}".to_string())]),
            MockHttpClient::always(crate::http::HttpResponse::ok(r#"[{"id":1}]"#)),
        );
        let result = server
            .verify_installation(Parameters(crate::tools::GitRepoParam {
                git_repository_path: "/tmp/project".to_string(),
            }))
            .await
            .unwrap();
        assert!(
            result_text(&result).contains("CLI Connectivity"),
            "got: {}",
            result_text(&result)
        );
    }

    #[tokio::test]
    async fn maybe_version_warning_returns_text_when_no_cache() {
        assert_eq!(
            make_server(false).maybe_version_warning("hello").await,
            "hello"
        );
    }

    #[tokio::test]
    async fn maybe_version_warning_prepends_warning_when_outdated() {
        let server = make_server_with_version("1.0.0", "2.0.0", true).await;
        let result = server.maybe_version_warning("body text").await;
        assert!(result.contains("VERSION UPDATE AVAILABLE"));
        assert!(result.contains("body text"));
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

    #[test]
    fn inlined_schema_for_produces_object_with_type() {
        let schema = crate::inlined_schema_for::<crate::tools::FilePathParam>();
        assert!(schema.contains_key("type") || schema.contains_key("properties"));
    }

    #[test]
    fn build_instructions_standalone_omits_api_tools() {
        let text = build_instructions(true, false, false);
        assert!(text.contains("code_health_review"));
        assert!(!text.contains("select_project"));
    }

    #[test]
    fn build_instructions_api_mode_includes_all_tools() {
        let text = build_instructions(false, false, false);
        assert!(text.contains("code_health_review"));
        assert!(text.contains("select_project"));
        assert!(text.contains("code_ownership_for_path"));
    }

    #[test]
    fn build_instructions_tools_filtered_adds_note() {
        let text = build_instructions(false, true, false);
        assert!(text.contains("enabled_tools"));
        assert!(text.contains("restricted"));
    }

    #[test]
    fn build_instructions_tools_not_filtered_no_note() {
        let text = build_instructions(false, false, false);
        assert!(!text.contains("restricted"));
    }

    // --- Tool filtering via enabled_tools ---

    fn tool_names(server: &CodeSceneServer) -> Vec<String> {
        server
            .tool_router
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    fn make_server_with_enabled_tools(is_standalone: bool, enabled_tools: &str) -> CodeSceneServer {
        let mut values = HashMap::new();
        values.insert("enabled_tools".to_string(), enabled_tools.to_string());
        CodeSceneServer::new(ServerDeps {
            config_data: ConfigData {
                instance_id: Some("test-filter".to_string()),
                values,
            },
            instance_id: "test-filter".to_string(),
            is_standalone,
            version_checker: VersionChecker::new("dev"),
            auth_manager: AuthManager::new(),
            cli_runner: Arc::new(cli::ProductionCliRunner),
            http_client: Arc::new(http::ReqwestClient),
            validator: Arc::new(MockValidator::passing()),
        })
    }

    fn assert_has_config_tools(names: &[String]) {
        assert!(
            names.contains(&"get_config".to_string()),
            "missing get_config"
        );
        assert!(
            names.contains(&"set_config".to_string()),
            "missing set_config"
        );
        assert!(names.contains(&"login".to_string()), "missing login");
        assert!(names.contains(&"logout".to_string()), "missing logout");
        assert!(
            names.contains(&"switch_account".to_string()),
            "missing switch_account"
        );
    }

    fn assert_tool_count_and_config(names: &[String], expected: usize) {
        assert_eq!(
            names.len(),
            expected,
            "expected {expected} tools, got: {names:?}"
        );
        assert_has_config_tools(names);
    }

    #[test]
    fn enabled_tools_unset_keeps_all_tools() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        let server = make_server(false);
        let names = tool_names(&server);
        assert_tool_count_and_config(&names, 27);
        assert!(names.contains(&"code_health_review".to_string()));
    }

    #[test]
    fn enabled_tools_filters_to_allowlist() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        let server = make_server_with_enabled_tools(false, "code_health_review,code_health_score");
        let names = tool_names(&server);
        // Should have the 2 enabled tools + 5 always-on = 7
        assert_tool_count_and_config(&names, 7);
        assert!(names.contains(&"code_health_review".to_string()));
        assert!(names.contains(&"code_health_score".to_string()));
    }

    #[test]
    fn enabled_tools_cannot_remove_config_tools() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        // Only enable one tool — config tools must still be present
        let server = make_server_with_enabled_tools(false, "code_health_review");
        let names = tool_names(&server);
        assert_has_config_tools(&names);
    }

    #[test]
    fn enabled_tools_combines_with_standalone_filtering() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        // In standalone mode, API_ONLY_TOOLS are removed first,
        // then enabled_tools further restricts the list
        let server = make_server_with_enabled_tools(true, "code_health_review,select_project");
        let names = tool_names(&server);
        // select_project is api-only, so removed in standalone even if in enabled_tools
        assert!(!names.contains(&"select_project".to_string()));
        assert!(names.contains(&"code_health_review".to_string()));
        assert_has_config_tools(&names);
    }

    #[test]
    fn enabled_tools_single_tool() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        let server = make_server_with_enabled_tools(false, "analyze_change_set");
        let names = tool_names(&server);
        assert_tool_count_and_config(&names, 6);
        assert!(names.contains(&"analyze_change_set".to_string()));
    }

    #[test]
    fn handle_serve_error_connection_closed_initialize_request_is_ok() {
        let result = crate::handle_serve_error(ServerInitializeError::ConnectionClosed(
            "initialize request".to_string(),
        ));
        assert!(result.is_ok());
    }

    #[test]
    fn handle_serve_error_connection_closed_initialize_notification_is_ok() {
        let result = crate::handle_serve_error(ServerInitializeError::ConnectionClosed(
            "initialize notification".to_string(),
        ));
        assert!(result.is_ok());
    }

    #[test]
    fn handle_serve_error_non_connection_closed_returns_err() {
        let err = ServerInitializeError::Cancelled;
        let result = crate::handle_serve_error(err);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn serve_or_handle_disconnect_returns_none_when_client_closes_early() {
        let server = make_server(false);
        let transport = ScriptedTransport::from_messages(vec![]);

        let result = crate::serve_or_handle_disconnect(server, transport)
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn serve_or_handle_disconnect_returns_service_on_successful_handshake() {
        let server = make_server(false);
        let transport = ScriptedTransport::from_messages(vec![
            initialize_request_message(),
            initialized_notification_message(),
        ]);

        let mut service = crate::serve_or_handle_disconnect(server, transport)
            .await
            .unwrap()
            .expect("expected initialized service");

        let close_result = service.close().await;
        assert!(close_result.is_ok());
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

    #[test]
    fn remove_docker_unsupported_tools_drops_login_and_switch_account_routes() {
        let mut router = CodeSceneServer::tool_router();
        assert!(
            router.list_all().iter().any(|t| t.name == "login"),
            "login should be present before removal"
        );
        assert!(
            router.list_all().iter().any(|t| t.name == "switch_account"),
            "switch_account should be present before removal"
        );
        remove_docker_unsupported_tools(&mut router);
        assert!(
            !router.list_all().iter().any(|t| t.name == "login"),
            "login should be removed for Docker"
        );
        assert!(
            !router.list_all().iter().any(|t| t.name == "switch_account"),
            "switch_account should be removed for Docker"
        );
    }

    #[test]
    fn docker_mode_removes_login_and_switch_account_tools_at_construction() {
        let _lock = config::lock_test_env();
        let _docker = environment::force_docker(true);
        let server = make_server(false);
        let names = tool_names(&server);
        assert!(
            !names.contains(&"login".to_string()),
            "login must be absent in Docker mode"
        );
        assert!(
            !names.contains(&"switch_account".to_string()),
            "switch_account must be absent in Docker mode"
        );
        assert!(
            names.contains(&"logout".to_string()),
            "logout must remain available in Docker"
        );
        assert!(names.contains(&"get_config".to_string()));
        assert!(names.contains(&"set_config".to_string()));
    }

    fn prompts_list_request_message() -> ClientJsonRpcMessage {
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "prompts/list",
            "params": {}
        }))
        .unwrap()
    }

    fn prompts_get_request_message(name: &str) -> ClientJsonRpcMessage {
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "prompts/get",
            "params": { "name": name }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn list_prompts_and_get_prompt_handlers_work() {
        use rmcp::ServiceExt;

        let transport = ScriptedTransport::from_messages(vec![
            initialize_request_message(),
            initialized_notification_message(),
            prompts_list_request_message(),
            prompts_get_request_message("review_code_health"),
        ]);
        let sent = transport.sent.clone();
        let server = make_server(false);

        let service = server
            .serve(transport)
            .await
            .expect("MCP handshake should succeed");

        // Drain remaining client requests (prompts/list + prompts/get).
        let _ = tokio::time::timeout(std::time::Duration::from_millis(500), service.waiting()).await;

        let messages = sent.lock().unwrap().clone();
        let bodies: Vec<serde_json::Value> = messages
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();

        let list_ok = bodies.iter().any(|b| {
            b.get("id") == Some(&json!(2))
                && b.pointer("/result/prompts")
                    .and_then(|p| p.as_array())
                    .is_some_and(|prompts| {
                        prompts.iter().any(|p| p.get("name") == Some(&json!("review_code_health")))
                    })
        });
        assert!(list_ok, "prompts/list handler should return review_code_health; got {bodies:?}");

        let get_ok = bodies.iter().any(|b| {
            b.get("id") == Some(&json!(3))
                && b.pointer("/result/messages")
                    .and_then(|m| m.as_array())
                    .is_some_and(|messages| !messages.is_empty())
        });
        assert!(get_ok, "prompts/get handler should return messages; got {bodies:?}");
    }
}

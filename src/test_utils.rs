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
mod auth;
#[cfg(test)]
mod cli_flows;
#[cfg(test)]
mod protocol;
#[cfg(test)]
mod tool_registry;

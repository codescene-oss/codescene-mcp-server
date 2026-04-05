use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rmcp::model::CallToolResult;

use crate::cli::{self, CliRunner};
use crate::config::ConfigData;
use crate::errors::CliError;
use crate::http::{self};
use crate::http::tests::MockHttpClient;
use crate::version_checker::VersionChecker;
use crate::{CodeSceneServer, ServerDeps};

pub(crate) fn test_deps(
    id: &str,
    is_standalone: bool,
    cli: Arc<dyn cli::CliRunner>,
    http: Arc<dyn http::HttpClient>,
) -> ServerDeps {
    ServerDeps {
        config_data: ConfigData {
            instance_id: Some(id.to_string()),
            values: HashMap::new(),
        },
        instance_id: id.to_string(),
        is_standalone,
        version_checker: VersionChecker::new("dev"),
        cli_runner: cli,
        http_client: http,
    }
}

pub(crate) fn make_server(is_standalone: bool) -> CodeSceneServer {
    CodeSceneServer::new(test_deps(
        "test-instance",
        is_standalone,
        Arc::new(cli::ProductionCliRunner),
        Arc::new(http::ReqwestClient),
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
        cli_runner: Arc::new(cli::ProductionCliRunner),
        http_client: Arc::new(http::ReqwestClient),
    })
}

pub(crate) struct TokenGuard<'a> {
    _lock: std::sync::MutexGuard<'a, ()>,
}

impl Drop for TokenGuard<'_> {
    fn drop(&mut self) {
        std::env::remove_var("CS_ACCESS_TOKEN");
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
    TokenGuard { _lock: lock }
}

pub(crate) struct MockCliRunner {
    responses: Mutex<Vec<Result<String, CliError>>>,
}

impl MockCliRunner {
    pub(crate) fn with_responses(responses: Vec<Result<String, CliError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
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
}

#[async_trait]
impl CliRunner for MockCliRunner {
    async fn run(
        &self,
        _args: &[&str],
        _working_dir: Option<&Path>,
    ) -> Result<String, CliError> {
        self.responses.lock().unwrap().remove(0)
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
        Arc::new(cli),
        Arc::new(http),
    ))
}

pub(crate) fn make_cli_mock_server(cli: MockCliRunner) -> CodeSceneServer {
    make_server_with_mocks(false, cli, MockHttpClient::new(vec![]))
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
    use std::collections::HashMap;

    use rmcp::ServerHandler;

    use super::*;
    use crate::config::{self, ConfigData};
    use crate::server_handler::{build_instructions, extract_md_title, resolve_resource_content};
    use crate::version_checker::VersionChecker;
    use crate::{
        display_version, fetch_cli_version, help_text, parse_cli_args,
        resources, CliAction, API_ONLY_TOOLS,
    };

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
        assert!(make_server(false).require_token().is_some());
    }

    #[tokio::test]
    async fn require_token_returns_none_when_set() {
        let _g = set_token("token");
        assert!(make_server(false).require_token().is_none());
    }

    #[tokio::test]
    async fn require_token_treats_whitespace_as_missing() {
        let _g = set_token("   ");
        assert!(make_server(false).require_token().is_some());
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
    }

    #[test]
    fn inlined_schema_for_produces_object_with_type() {
        let schema = crate::inlined_schema_for::<crate::tools::FilePathParam>();
        assert!(schema.contains_key("type") || schema.contains_key("properties"));
    }

    #[test]
    fn resolve_resource_content_returns_how_it_works() {
        let content = resolve_resource_content(resources::HOW_IT_WORKS_URI).unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn resolve_resource_content_returns_business_case() {
        let content = resolve_resource_content(resources::BUSINESS_CASE_URI).unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn resolve_resource_content_returns_error_for_unknown() {
        let result = resolve_resource_content("unknown://resource");
        assert!(result.is_err());
    }

    #[test]
    fn build_instructions_standalone_omits_api_tools() {
        let text = build_instructions(true, false);
        assert!(text.contains("code_health_review"));
        assert!(!text.contains("select_project"));
    }

    #[test]
    fn build_instructions_api_mode_includes_all_tools() {
        let text = build_instructions(false, false);
        assert!(text.contains("code_health_review"));
        assert!(text.contains("select_project"));
        assert!(text.contains("code_ownership_for_path"));
    }

    #[test]
    fn build_instructions_tools_filtered_adds_note() {
        let text = build_instructions(false, true);
        assert!(text.contains("enabled_tools"));
        assert!(text.contains("restricted"));
    }

    #[test]
    fn build_instructions_tools_not_filtered_no_note() {
        let text = build_instructions(false, false);
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

    fn make_server_with_enabled_tools(
        is_standalone: bool,
        enabled_tools: &str,
    ) -> CodeSceneServer {
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
            cli_runner: Arc::new(cli::ProductionCliRunner),
            http_client: Arc::new(http::ReqwestClient),
        })
    }

    fn assert_has_config_tools(names: &[String]) {
        assert!(names.contains(&"get_config".to_string()), "missing get_config");
        assert!(names.contains(&"set_config".to_string()), "missing set_config");
    }

    fn assert_tool_count_and_config(names: &[String], expected: usize) {
        assert_eq!(names.len(), expected, "expected {expected} tools, got: {names:?}");
        assert_has_config_tools(names);
    }

    #[test]
    fn enabled_tools_unset_keeps_all_tools() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        let server = make_server(false);
        let names = tool_names(&server);
        assert_tool_count_and_config(&names, 16);
        assert!(names.contains(&"code_health_review".to_string()));
    }

    #[test]
    fn enabled_tools_filters_to_allowlist() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        let server =
            make_server_with_enabled_tools(false, "code_health_review,code_health_score");
        let names = tool_names(&server);
        // Should have the 2 enabled tools + 2 always-on = 4
        assert_tool_count_and_config(&names, 4);
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
        let server =
            make_server_with_enabled_tools(true, "code_health_review,select_project");
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
        assert_tool_count_and_config(&names, 3);
        assert!(names.contains(&"analyze_change_set".to_string()));
    }

    #[test]
    fn extract_md_title_returns_first_heading() {
        assert_eq!(extract_md_title("# Hello World\nsome text"), "Hello World");
    }

    #[test]
    fn extract_md_title_falls_back_to_resource() {
        assert_eq!(extract_md_title("no heading here"), "Untitled");
    }
}

mod ace_client;
mod api_client;
mod business_case;
mod cli;
mod config;
mod configure;
mod delta;
mod docker;
mod environment;
mod errors;
mod event_properties;
mod hashing;
mod http;
mod license;
mod platform;
mod prompts;
mod resources;
mod tools;
mod tracking;
mod version_checker;

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, GetPromptRequestParams, GetPromptResult, Implementation,
    ListPromptsResult, ListResourcesResult, PaginatedRequestParams, Prompt, PromptArgument,
    PromptMessage, PromptMessageRole, RawResource, ReadResourceRequestParams, ReadResourceResult,
    ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::schemars::{self, JsonSchema};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler, ServiceExt};
use tracing_subscriber::EnvFilter;

use crate::cli::CliRunner;
use crate::config::ConfigData;
use crate::http::HttpClient;
use crate::tools::{
    ChangeSetParam, FilePathParam, GetConfigParam, GitRepoParam, OptionalContext, OwnershipParam,
    ProjectFileParam, ProjectParam, RefactorParam, SetConfigParam,
};
use crate::version_checker::VersionChecker;

const TOKEN_MISSING_MSG: &str = "\
No access token configured.\n\n\
To use this tool, set your access token using one of these methods:\n\
1. Use the `set_config` tool: set_config(key=\"access_token\", value=\"your-token\")\n\
2. Set the CS_ACCESS_TOKEN environment variable in your MCP client configuration\n\n\
To get an Access Token, see:\n\
https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/getting-a-personal-access-token.md";

const _VERSION_NOTICE_SUFFIX: &str = "\n\
Note: If the result contains version update information (indicated by\n\
\"VERSION UPDATE AVAILABLE\"), please inform the user about this update\n\
notice and recommend they update their CodeScene MCP Server.";

const API_ONLY_TOOLS: &[&str] = &[
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
];

#[derive(Debug)]
enum CliAction {
    RunServer,
    PrintHelp,
    PrintVersion(String),
}

fn display_version(raw_version: &str) -> &str {
    raw_version.strip_prefix("MCP-").unwrap_or(raw_version)
}

fn help_text() -> &'static str {
    "CodeScene MCP Server\n\nUsage: cs-mcp [OPTIONS]\n\nOptions:\n  -h, --help       Show this help message and exit\n  -v, --version    Show version and exit"
}

fn parse_cli_args(args: &[String], raw_version: &str) -> Result<CliAction, String> {
    if args.is_empty() {
        return Ok(CliAction::RunServer);
    }

    if args.len() == 1 {
        return match args[0].as_str() {
            "-h" | "--help" => Ok(CliAction::PrintHelp),
            "-v" | "--version" => Ok(CliAction::PrintVersion(display_version(raw_version).to_string())),
            other => Err(format!("Unknown argument: {other}")),
        };
    }

    Err(format!("Unexpected arguments: {}", args.join(" ")))
}

fn inlined_schema_for<T: JsonSchema + 'static>() -> Arc<serde_json::Map<String, serde_json::Value>>
{
    let mut settings = schemars::generate::SchemaSettings::draft2020_12();
    settings.inline_subschemas = true;
    settings.transforms = vec![Box::new(schemars::transform::AddNullable::default())];
    let generator = settings.into_generator();
    let schema = generator.into_root_schema_for::<T>();
    let value = serde_json::to_value(schema).expect("schema serialization failed");
    match value {
        serde_json::Value::Object(obj) => Arc::new(obj),
        _ => panic!("schema is not an object"),
    }
}

struct ServerDeps {
    config_data: ConfigData,
    instance_id: String,
    is_standalone: bool,
    version_checker: VersionChecker,
    cli_runner: Arc<dyn CliRunner>,
    http_client: Arc<dyn HttpClient>,
}

#[derive(Clone)]
struct CodeSceneServer {
    tool_router: ToolRouter<Self>,
    pub(crate) version_checker: VersionChecker,
    pub(crate) config_data: Arc<ConfigData>,
    instance_id: String,
    is_standalone: bool,
    pub(crate) cli_runner: Arc<dyn CliRunner>,
    pub(crate) http_client: Arc<dyn HttpClient>,
}

impl CodeSceneServer {
    pub(crate) fn require_token(&self) -> Option<CallToolResult> {
        if std::env::var("CS_ACCESS_TOKEN")
            .map(|v| v.is_empty())
            .unwrap_or(true)
        {
            return Some(CallToolResult::success(vec![Content::text(
                TOKEN_MISSING_MSG,
            )]));
        }
        None
    }

    pub(crate) async fn maybe_version_warning(&self, text: &str) -> String {
        if let Some(info) = self.version_checker.try_read().await {
            if info.is_outdated {
                let warning = version_checker::format_version_warning(&info);
                return format!("{warning}{text}");
            }
        }
        text.to_string()
    }

    pub(crate) fn track(&self, event: &str, props: serde_json::Value) {
        tracking::track_event(event, props, &self.instance_id);
    }

    pub(crate) fn track_err(&self, tool: &str, err: &str) {
        tracking::track_error(err, tool, &self.instance_id);
    }
}

#[tool_router]
impl CodeSceneServer {
    fn new(deps: ServerDeps) -> Self {
        let mut router = Self::tool_router();
        if deps.is_standalone {
            for name in API_ONLY_TOOLS {
                router.remove_route(name);
            }
        }

        Self {
            tool_router: router,
            version_checker: deps.version_checker,
            config_data: Arc::new(deps.config_data),
            instance_id: deps.instance_id,
            is_standalone: deps.is_standalone,
            cli_runner: deps.cli_runner,
            http_client: deps.http_client,
        }
    }

    #[tool(
        description = "Explains CodeScene's Code Health metric for assessing code quality and maintainability for both human devs and AI.",
        input_schema = inlined_schema_for::<OptionalContext>()
    )]
    async fn explain_code_health(
        &self,
        #[allow(unused_variables)] Parameters(_params): Parameters<OptionalContext>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::explain_code_health::handle(self).await
    }

    #[tool(
        description = "Describes how to build a business case for Code Health improvements.",
        input_schema = inlined_schema_for::<OptionalContext>()
    )]
    async fn explain_code_health_productivity(
        &self,
        #[allow(unused_variables)] Parameters(_params): Parameters<OptionalContext>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::explain_code_health_productivity::handle(self).await
    }

    #[tool(
        description = "Review the Code Health of a single source file and return a detailed CLI review output that includes the score and code smell findings.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_review(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_review::handle(self, params).await
    }

    #[tool(
        description = "Calculate the Code Health score for a single source file.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_score(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_score::handle(self, params).await
    }

    #[tool(
        description = "Review all modified and staged files in a repository and report Code Health degradations before commit.",
        input_schema = inlined_schema_for::<GitRepoParam>()
    )]
    async fn pre_commit_code_health_safeguard(
        &self,
        Parameters(params): Parameters<GitRepoParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::pre_commit_code_health_safeguard::handle(self, params).await
    }

    #[tool(
        description = "Run a branch-level Code Health review for all files that differ between current HEAD and base_ref.",
        input_schema = inlined_schema_for::<ChangeSetParam>()
    )]
    async fn analyze_change_set(
        &self,
        Parameters(params): Parameters<ChangeSetParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::analyze_change_set::handle(self, params).await
    }

    #[tool(
        description = "Generate a data-driven business case for refactoring a source file.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_refactoring_business_case(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_refactoring_business_case::handle(self, params).await
    }

    #[tool(
        description = "Refactor a single function to fix specific code health problems.",
        input_schema = inlined_schema_for::<RefactorParam>()
    )]
    async fn code_health_auto_refactor(
        &self,
        Parameters(params): Parameters<RefactorParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_auto_refactor::handle(self, params).await
    }

    #[tool(description = "Lists all projects for an organization for selection by the user.")]
    async fn select_project(&self) -> Result<CallToolResult, ErrorData> {
        tools::select_project::handle(self).await
    }

    #[tool(
        description = "Lists the technical debt goals for a project.",
        input_schema = inlined_schema_for::<ProjectParam>()
    )]
    async fn list_technical_debt_goals_for_project(
        &self,
        Parameters(params): Parameters<ProjectParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_goals_for_project::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt goals for a specific file in a project.",
        input_schema = inlined_schema_for::<ProjectFileParam>()
    )]
    async fn list_technical_debt_goals_for_project_file(
        &self,
        Parameters(params): Parameters<ProjectFileParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_goals_for_project_file::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt hotspots for a project.",
        input_schema = inlined_schema_for::<ProjectParam>()
    )]
    async fn list_technical_debt_hotspots_for_project(
        &self,
        Parameters(params): Parameters<ProjectParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_hotspots_for_project::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt hotspots for a specific file in a project.",
        input_schema = inlined_schema_for::<ProjectFileParam>()
    )]
    async fn list_technical_debt_hotspots_for_project_file(
        &self,
        Parameters(params): Parameters<ProjectFileParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_hotspots_for_project_file::handle(self, params).await
    }

    #[tool(
        description = "Find the owner or owners of a specific path in a project.",
        input_schema = inlined_schema_for::<OwnershipParam>()
    )]
    async fn code_ownership_for_path(
        &self,
        Parameters(params): Parameters<OwnershipParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_ownership_for_path::handle(self, params).await
    }

    #[tool(
        description = "Read current CodeScene MCP Server configuration values.",
        input_schema = inlined_schema_for::<GetConfigParam>()
    )]
    async fn get_config(
        &self,
        Parameters(params): Parameters<GetConfigParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::get_config::handle(self, params).await
    }

    #[tool(
        description = "Write a CodeScene MCP Server configuration value.",
        input_schema = inlined_schema_for::<SetConfigParam>()
    )]
    async fn set_config(
        &self,
        Parameters(params): Parameters<SetConfigParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::set_config::handle(self, params).await
    }
}

#[tool_handler]
impl ServerHandler for CodeSceneServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "codescene-mcp-server",
            env!("CS_MCP_VERSION"),
        ))
        .with_instructions(build_instructions(self.is_standalone))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        use rmcp::model::AnnotateAble;
        let resources = vec![
            RawResource::new(
                resources::HOW_IT_WORKS_URI,
                extract_md_title(resources::HOW_IT_WORKS),
            )
            .with_description(
                "Explains CodeScene's Code Health metric for assessing code quality and maintainability for both human devs and AI.",
            )
            .with_mime_type("text/markdown")
            .no_annotation(),
            RawResource::new(
                resources::BUSINESS_CASE_URI,
                extract_md_title(resources::BUSINESS_CASE),
            )
            .with_description("Describes how to build a business case for Code Health improvements.")
            .with_mime_type("text/markdown")
            .no_annotation(),
        ];
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let uri = request.uri.as_str();
        let content = resolve_resource_content(uri)?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            content, uri,
        )]))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let prompts_list = vec![
            Prompt::new(
                "review_code_health",
                Some(
                    "Review Code Health and assess code quality for the current open file. The file path needs to be sent to the code_health_review MCP tool when using this prompt.",
                ),
                Some(vec![PromptArgument::new("context")
                    .with_description("Optional context string.")
                    .with_required(false)]),
            ),
            Prompt::new(
                "plan_code_health_refactoring",
                Some("Plan a prioritized, low-risk refactoring to remediate detected Code Health issues."),
                Some(vec![PromptArgument::new("context")
                    .with_description("Optional context string.")
                    .with_required(false)]),
            ),
        ];
        Ok(ListPromptsResult::with_all_items(prompts_list))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        let text = prompts::resolve_prompt_text(&request.name).ok_or_else(|| {
            ErrorData::new(
                rmcp::model::ErrorCode::INVALID_REQUEST,
                format!("Unknown prompt: {}", request.name),
                None,
            )
        })?;
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            text,
        )]))
    }
}

fn resolve_resource_content(uri: &str) -> Result<&'static str, ErrorData> {
    if uri == resources::HOW_IT_WORKS_URI {
        Ok(resources::HOW_IT_WORKS)
    } else if uri == resources::BUSINESS_CASE_URI {
        Ok(resources::BUSINESS_CASE)
    } else {
        Err(ErrorData::new(
            rmcp::model::ErrorCode::INVALID_REQUEST,
            format!("Unknown resource: {uri}"),
            None,
        ))
    }
}

fn build_instructions(is_standalone: bool) -> String {
    let mut text = String::from(
        "CodeScene MCP Server - Code Health analysis tools for AI-assisted development.\n\n\
         TOOLS (always available):\n\
         - explain_code_health: Learn about the Code Health metric.\n\
         - explain_code_health_productivity: Business case for Code Health.\n\
         - code_health_review: Detailed review of a single file.\n\
         - code_health_score: Quick numeric score for a file.\n\
         - pre_commit_code_health_safeguard: Check staged changes before commit.\n\
         - analyze_change_set: Branch-level review before PR.\n\
         - code_health_refactoring_business_case: ROI for refactoring.\n\
         - code_health_auto_refactor: ACE-powered function refactoring.\n\
         - get_config / set_config: Manage server configuration.\n",
    );

    if !is_standalone {
        text.push_str(
            "\nTOOLS (API-connected):\n\
             - select_project: Choose a CodeScene project.\n\
             - list_technical_debt_goals_for_project: View debt goals.\n\
             - list_technical_debt_goals_for_project_file: File-level goals.\n\
             - list_technical_debt_hotspots_for_project: View hotspots.\n\
             - list_technical_debt_hotspots_for_project_file: File-level hotspots.\n\
             - code_ownership_for_path: Find code owners.\n",
        );
    }

    text
}

fn extract_md_title(content: &str) -> &str {
    for line in content.lines() {
        if let Some(title) = line.strip_prefix("# ") {
            return title.trim();
        }
    }
    "Untitled"
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::errors::CliError;
    use crate::http::tests::MockHttpClient;

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
        vc.set_cached_info(version_checker::VersionInfo {
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
        let lock = config::lock_test_env();
        std::env::set_var("CS_ACCESS_TOKEN", value);
        TokenGuard { _lock: lock }
    }

    pub(crate) fn clear_token() -> TokenGuard<'static> {
        let lock = config::lock_test_env();
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
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let raw_version = env!("CS_MCP_VERSION");
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_cli_args(&args, raw_version) {
        Ok(CliAction::RunServer) => {}
        Ok(CliAction::PrintHelp) => {
            println!("{}", help_text());
            return Ok(());
        }
        Ok(CliAction::PrintVersion(version)) => {
            println!("{version}");
            return Ok(());
        }
        Err(message) => {
            eprintln!("{message}");
            eprintln!("Use --help to see available options.");
            anyhow::bail!("invalid command line arguments");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting CodeScene MCP server");

    config::snapshot_client_env_vars();
    let config_data = config::load().unwrap_or_default();
    config::apply_to_env(&config_data);
    let instance_id = config::instance_id(&config_data);

    let token = std::env::var("CS_ACCESS_TOKEN").unwrap_or_default();
    let is_standalone = !token.is_empty() && license::is_standalone_license(&token);

    let version = raw_version;
    let version_checker = VersionChecker::new(version);

    let server = CodeSceneServer::new(ServerDeps {
        config_data,
        instance_id,
        is_standalone,
        version_checker,
        cli_runner: Arc::new(cli::ProductionCliRunner),
        http_client: Arc::new(http::ReqwestClient),
    });

    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {:?}", e))?;

    service.waiting().await?;
    Ok(())
}

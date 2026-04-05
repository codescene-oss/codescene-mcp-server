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
mod startup;
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

/// Tools that cannot be disabled via `enabled_tools` config.
const ALWAYS_ENABLED_TOOLS: &[&str] = &["get_config", "set_config"];

#[derive(Debug)]
enum CliAction {
    RunServer,
    PrintHelp,
    PrintVersion(String),
    PrintCliVersion,
}

fn display_version(raw_version: &str) -> &str {
    raw_version.strip_prefix("MCP-").unwrap_or(raw_version)
}

fn help_text() -> &'static str {
    "CodeScene MCP Server\n\nUsage: cs-mcp [OPTIONS]\n\nOptions:\n  -h, --help       Show this help message and exit\n  -v, --version    Show version and exit\n  --cli-version    Show embedded CLI version and exit"
}

fn parse_cli_args(args: &[String], raw_version: &str) -> Result<CliAction, String> {
    if args.is_empty() {
        return Ok(CliAction::RunServer);
    }

    if args.len() == 1 {
        return match args[0].as_str() {
            "-h" | "--help" => Ok(CliAction::PrintHelp),
            "-v" | "--version" => Ok(CliAction::PrintVersion(
                display_version(raw_version).to_string(),
            )),
            "--cli-version" => Ok(CliAction::PrintCliVersion),
            other => Err(format!("Unknown argument: {other}")),
        };
    }

    Err(format!("Unexpected arguments: {}", args.join(" ")))
}

async fn fetch_cli_version(cli_runner: &dyn cli::CliRunner) -> anyhow::Result<String> {
    Ok(cli_runner.run(&["version"], None).await?)
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
            .map(|v| v.trim().is_empty())
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

        // Filter tools based on enabled_tools allowlist
        if let Some(enabled) = config::enabled_tools(&deps.config_data) {
            let all_names: Vec<String> = router
                .list_all()
                .iter()
                .map(|t| t.name.to_string())
                .collect();
            for name in all_names {
                if !ALWAYS_ENABLED_TOOLS.contains(&name.as_str()) && !enabled.contains(&name) {
                    router.remove_route(&name);
                }
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
        description = "Explains CodeScene's Code Health metric for assessing code quality and maintainability for both human devs and AI.\n\nWhen to use:\n    Use this tool when a user asks what Code Health means, how scores are\n    interpreted, or why Code Health matters in day-to-day development.\n\nLimitations:\n    - Returns static documentation text from this MCP server package.\n    - Does not analyze a specific repository or file.\n\nReturns:\n    Markdown content explaining the Code Health model and core concepts.\n\nExample:\n    Call this tool, then summarize the returned Markdown into a short\n    explanation tailored to the user's current question.",
        input_schema = inlined_schema_for::<OptionalContext>()
    )]
    async fn explain_code_health(
        &self,
        #[allow(unused_variables)] Parameters(_params): Parameters<OptionalContext>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::explain_code_health::handle(self).await
    }

    #[tool(
        description = "Describes how to build a business case for Code Health improvements.\nCovers empirical data on how healthy code lets you ship faster with\nfewer defects.\n\nWhen to use:\n    Use this tool when a user asks for ROI, productivity impact, or\n    management-facing framing for refactoring investments.\n\nLimitations:\n    - Returns static documentation text from this MCP server package.\n    - Does not compute project-specific forecasts.\n\nReturns:\n    Markdown content describing productivity and defect-risk implications\n    of improving Code Health.\n\nExample:\n    Call this tool and extract 2-3 evidence-based points to support a\n    proposal for incremental refactoring.",
        input_schema = inlined_schema_for::<OptionalContext>()
    )]
    async fn explain_code_health_productivity(
        &self,
        #[allow(unused_variables)] Parameters(_params): Parameters<OptionalContext>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::explain_code_health_productivity::handle(self).await
    }

    #[tool(
        description = "Review the Code Health of a single source file and return a detailed\nCLI review output that includes the score and code smell findings.\n\nWhen to use:\n    Use this tool when you need actionable maintainability diagnostics\n    for one file (not just the score).\n\nLimitations:\n    - Analyzes one file at a time.\n    - Requires a supported source file.\n    - Returns CLI review text, not a normalized JSON schema.\n\nReturns:\n    A review string from the CodeScene CLI. The output typically\n    includes a Code Health score and code smell details to explain\n    why the score is high or low.\n\n    The Code Health scores are interpreted as:\n      * Optimal Code: a Code Health 10.0 is optimized for both human and AI comprehension\n      * Green Code: high quality with a score of 9.0-9.9\n      * Yellow Code: problematic technical debt with a score of 4.0-8.9\n      * Red Code: severe technical debt, maintainability issues, and expensive onboarding with a score of 1.0-3.9\n\nExample:\n    Call with file_path=\"/repo/src/app.py\" and summarize the returned\n    smells into prioritized refactoring actions.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_review(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_review::handle(self, params).await
    }

    #[tool(
        description = "Calculate the Code Health score for a single source file.\nThe tool returns one numeric score from 10.0 (optimal) to 1.0 (worst).\n\nWhen to use:\n    Use this tool for quick triage, ranking files by maintainability,\n    or checking whether a refactoring improved file-level quality.\n\nLimitations:\n    - Analyzes one file at a time.\n    - Returns only the score summary, not the full smell breakdown.\n    - Requires a supported source file.\n\nReturns:\n    A string in the format \"Code Health score: <score>\".\n\n    The Code Health scores are interpreted as:\n      * Optimal Code: Code Health 10.0 optimized for human and AI comprehension\n      * Green Code: high quality with a score of 9.0-9.9\n      * Yellow Code: problematic technical debt with a score of 4.0-8.9\n      * Red Code: severe technical debt with a score of 1.0-3.9\n\nExample:\n    Call with file_path=\"/repo/src/module.py\" and compare the score\n    before and after a refactoring.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_score(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_score::handle(self, params).await
    }

    #[tool(
        description = "Review all modified and staged files in a repository and report\nCode Health degradations before commit.\n\nWhen to use:\n    Use this tool as a pre-commit safeguard on local changes to catch\n    regressions and code smells before creating a commit.\n\nLimitations:\n    - Requires a valid git repository path.\n    - Evaluates current local modifications/staged changes only.\n    - Output is JSON text from the CLI command.\n\nReturns:\n    A JSON object containing:\n      - quality_gates: the central outcome, summarizing whether the commit passes or fails Code Health thresholds for each file.\n      - files: an array of objects for each file with:\n          - name: the name of the file whose Code Health is impacted (positively or negatively).\n          - findings: an array describing improvements/degradation for each code smell.\n      - Each quality gate indicates if the file meets the required Code Health standards, helping teams enforce healthy code before commit.\n\nExample:\n    Run on git_repository_path=\"/repo\" and block commit preparation if\n    any quality gate fails.",
        input_schema = inlined_schema_for::<GitRepoParam>()
    )]
    async fn pre_commit_code_health_safeguard(
        &self,
        Parameters(params): Parameters<GitRepoParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::pre_commit_code_health_safeguard::handle(self, params).await
    }

    #[tool(
        description = "Run a branch-level Code Health review for all files that differ between\ncurrent HEAD and base_ref.\n\nWhen to use:\n    Use this as a local PR pre-flight check before opening a pull\n    request, so regressions are caught across the full change set.\n\nLimitations:\n    - Requires a valid git repository path.\n    - base_ref must exist and be resolvable by git in that repository.\n    - Reviews only files that differ from base_ref.\n    - Output is JSON text from the CLI command.\n\nThe result can be used to decide whether to refactor before creating\nor updating a pull request.\n\nReturns:\n    A JSON object containing:\n      - quality_gates: the central outcome, summarizing whether the change\n        set passes or fails Code Health thresholds (\"passed\" or \"failed\").\n      - results: an array of objects for each affected file with:\n          - name: the name of the file whose Code Health is impacted.\n          - verdict: \"improved\", \"degraded\", or \"stable\".\n          - findings: an array describing improvements/degradation for each code smell.\n\nExample:\n    Compare against base_ref=\"main\" for git_repository_path=\"/repo\" and\n    fail the local PR check if any file verdict is \"degraded\".",
        input_schema = inlined_schema_for::<ChangeSetParam>()
    )]
    async fn analyze_change_set(
        &self,
        Parameters(params): Parameters<ChangeSetParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::analyze_change_set::handle(self, params).await
    }

    #[tool(
        description = "Generate a data-driven business case for refactoring a source file.\n\nWhen to use:\n    Use this tool to justify refactoring investment with quantified\n    predictions tied to the file's current Code Health.\n\nLimitations:\n    - Estimates are model-based projections, not guarantees.\n    - Evaluates one file at a time.\n    - Requires an analyzable source file.\n\nReturns:\n    A JSON object with:\n        - scenario: Recommended target Code Health level.\n        - optimistic_outcome: Upper bound estimate for improvements\n          in development speed and defect reduction.\n        - pessimistic_outcome: Lower bound estimate for improvements.\n        - confidence_interval: The optimistic to pessimistic range,\n          representing a 90% confidence interval for the expected impact.\n\nExample:\n    Call with file_path=\"/repo/src/service.py\" and use the optimistic\n    and pessimistic outcomes to frame a refactoring proposal.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_refactoring_business_case(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_refactoring_business_case::handle(self, params).await
    }

    #[tool(
        description = "Refactor a single function to fix specific code health problems.\nThis auto-refactor uses CodeScene ACE, and is intended as an initial\nrefactoring to increase the modularity of the code so that you as an\nAI agent can continue and iterate with more specific refactorings.\n\nWhen to use:\n    Use this tool after a Code Health review has identified one of the\n    supported smells in a specific function.\n\nThe code_health_auto_refactor tool is supported for these languages:\n    - JavaScript/TypeScript\n    - Java\n    - C#\n    - C++\nand for these code smells:\n    - Complex Conditional\n    - Bumpy Road Ahead\n    - Complex Method\n    - Deep, Nested Complexity\n    - Large Method\n\nIMPORTANT:\n    - Only use this tool for functions shorter than 300 lines of code.\n    - Insert any new functions close to the refactored function.\n    - Requires ACE access to be configured (use set_config with key \"ace_access_token\").\n\nReturns:\n    A JSON object describing the refactoring, with these properties:\n      - code: The refactored function plus new extracted functions.\n      - declarations: Optional (used for languages like C++). Declarations of additional functions introduced when refactoring.\n        When present, find the right include file and insert the declarations there. Note that some C++ refactorings result\n        in standalone functions; standalone functions should just be inserted in the implementation unit, not declared in\n        include files.\n      - confidence: The confidence level of the resulting refactoring. For low confidence, review the\n        refactoring and fix any introduced problems.\n      - reasons: A list of strings describing the reasons for the assigned confidence level.\n        Use this list of strings to direct fixes of the refactored code.\n\nExample:\n    Call with file_path=\"/repo/src/service.ts\" and\n    function_name=\"OrderService.calculateTotal\", then apply returned\n    code and declarations and re-run Code Health checks.",
        input_schema = inlined_schema_for::<RefactorParam>()
    )]
    async fn code_health_auto_refactor(
        &self,
        Parameters(params): Parameters<RefactorParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_auto_refactor::handle(self, params).await
    }

    #[tool(
        description = "Lists all projects for an organization for selection by the user.\nThe user can select the desired project by either its name or ID.\n\nWhen to use:\n    Use this tool before project-scoped API tools so the user can pick\n    the project context explicitly.\n\nLimitations:\n    - If default_project_id is configured, the server returns that\n      project and selection is effectively locked.\n\nReturns:\n    A JSON object with the project name and ID, formatted in a Markdown table\n    with the columns \"Project Name\" and \"Project ID\". If the output contains a\n    `description` field, it indicates that a default project is configured\n    (`default_project_id`), and the user cannot select a different project.\n    Explain this to the user.\n\n    Additionally, a `link` field is provided to guide the user to the\n    Codescene projects page where the user can find more detailed information about each project.\n    Make sure to include this link in the output, and explain its purpose clearly.\n\nExample:\n    Call without arguments. If default_project_id is configured,\n    explain that the returned project is fixed unless that config is changed via set_config."
    )]
    async fn select_project(&self) -> Result<CallToolResult, ErrorData> {
        tools::select_project::handle(self).await
    }

    #[tool(
        description = "Lists the technical debt goals for a project.\n\nWhen to use:\n    Use this tool to see all files in a project that currently have\n    explicit technical debt goals in CodeScene.\n\nLimitations:\n    - Requires a valid project_id.\n    - Returns goal data from the latest available analysis.\n    - Includes only files with non-empty goals.\n\nReturns:\n    A JSON object with two fields:\n    - `data`: an array of objects each containing the path of a file and its goals.\n    - `link`: a URL to the CodeScene Code Biomarkers page for the project.\n\n    Show the goals for each file in a structured format that is easy to read and explain\n    the goal description for each file. It also includes a description, please include that in your output.\n    Always include the `link` in the output and explain that more details about the\n    technical debt goals can be found on that page.\n\nExample:\n    Call with project_id=42 and summarize each file's goals with the\n    biomarkers link for deeper inspection.",
        input_schema = inlined_schema_for::<ProjectParam>()
    )]
    async fn list_technical_debt_goals_for_project(
        &self,
        Parameters(params): Parameters<ProjectParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_goals_for_project::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt goals for a specific file in a project.\n\nWhen to use:\n    Use this tool when you need goal details for one file before\n    planning targeted refactoring work.\n\nLimitations:\n    - Requires a valid project_id.\n    - Returns data from the latest available analysis only.\n    - A file may return zero goals, which is a valid outcome.\n\nReturns:\n    A JSON object with two fields:\n    - `data`: an array containing the goals for the specified file.\n    - `link`: a URL to the CodeScene Code Biomarkers page for the specific file.\n\n    Show the goals in a structured format that is easy to read and explain\n    the goal description. It also includes a description, please include that in your output.\n    Always include the `link` in the output and explain that more details about the\n    technical debt goals can be found on that page.\n\nExample:\n    Call with file_path=\"/repo/src/module.py\" and project_id=42, then\n    use the goals and link to propose file-level improvements.",
        input_schema = inlined_schema_for::<ProjectFileParam>()
    )]
    async fn list_technical_debt_goals_for_project_file(
        &self,
        Parameters(params): Parameters<ProjectFileParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_goals_for_project_file::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt hotspots for a project.\n\nWhen to use:\n    Use this tool to identify high-impact technical debt hotspots across\n    a project and prioritize refactoring targets.\n\nLimitations:\n    - Requires a valid project_id.\n    - Returns hotspots from the latest available project analysis.\n    - Uses the API's hotspot filtering and pagination behavior.\n\nReturns:\n    A JSON object with two fields:\n    - `data`: an array of objects each containing the path of a file, code health score, revisions count and lines of code count.\n    - `link`: a URL to the CodeScene technical debt hotspots page for the project.\n\n    Describe the hotspots for each file in a structured format that is easy to read and explain.\n    It also includes a description, please include that in your output.\n    Always include the `link` in the output and explain that the user can find more\n    detailed information about each hotspot on that page.\n\nExample:\n    Call with project_id=42 and rank returned hotspots by code health\n    and revision frequency before proposing refactoring work.",
        input_schema = inlined_schema_for::<ProjectParam>()
    )]
    async fn list_technical_debt_hotspots_for_project(
        &self,
        Parameters(params): Parameters<ProjectParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_hotspots_for_project::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt hotspots for a specific file in a project.\n\nWhen to use:\n    Use this tool to inspect hotspot metrics for one file before\n    deciding if it should be a refactoring candidate.\n\nLimitations:\n    - Requires a valid project_id.\n    - Returns at most one hotspot object for the filtered file path.\n    - If no hotspot exists for the file, returns an empty hotspot object.\n\nReturns:\n    A JSON object with two fields:\n    - `data`: an array containing the code health score, revisions count and lines of code count for the specified file.\n    - `link`: a URL to the CodeScene technical debt hotspots page for the project.\n\n    Describe the hotspot in a structured format that is easy to read and explain.\n    It also includes a description, please include that in your output.\n    Always include the `link` in the output and explain that the user can find more\n    detailed information about each hotspot on that page.\n\nExample:\n    Call with file_path=\"/repo/src/module.py\" and project_id=42. If\n    hotspot is empty, report that the file is not currently a hotspot.",
        input_schema = inlined_schema_for::<ProjectFileParam>()
    )]
    async fn list_technical_debt_hotspots_for_project_file(
        &self,
        Parameters(params): Parameters<ProjectFileParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_hotspots_for_project_file::handle(self, params).await
    }

    #[tool(
        description = "Find the owner or owners of a specific path in a project.\n\nWhen to use:\n    Use this tool to identify likely reviewers or domain experts for\n    code reviews and technical questions about a file or directory.\n\nLimitations:\n    - Requires a valid project_id.\n    - Uses the latest project analysis data available in CodeScene.\n    - If no matching ownership data is found, an empty JSON array is returned.\n\nReturns:\n    A list of owners and their paths that they own. The name of the owner who can be responsible\n    for code reviews or inquiries about the file and a link to the CodeScene System Map page filtered\n    by the owner. Explain that this link can be used to see more details\n    about the owner's contributions and interactions within the project.\n    You MUST always show a link after every owner. Show resulting data in a Markdown\n    table with columns: Owner, Key Areas, Link.\n\nExample:\n    Call with project_id=42 and path=\"/repo/src/service.py\", then\n    present each owner row with its corresponding system-map link.",
        input_schema = inlined_schema_for::<OwnershipParam>()
    )]
    async fn code_ownership_for_path(
        &self,
        Parameters(params): Parameters<OwnershipParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_ownership_for_path::handle(self, params).await
    }

    #[tool(
        description = "Read current CodeScene MCP Server configuration values.\n\nWhen to use:\n    Use this tool to discover available configuration keys, inspect\n    effective values, and understand where each value comes from.\n\nLimitations:\n    - Returns JSON text only; callers must format it for display.\n    - Sensitive values (tokens) are masked.\n    - Effective values can be overridden by client-provided env vars.\n\nWhen called without a key, lists every available configuration\noption together with its current effective value, the source of\nthat value (environment variable vs. config file), and a short\ndescription.\n\nWhen called with a specific key, returns details for that option\nonly. Sensitive values (tokens) are masked in the output.\n\nReturns:\n    A JSON string. When querying a single key, the object has:\n    key, env_var, value, source, description, aliases, and\n    docs_url. When listing all, the object has: config_dir and\n    options (array of the same shape). Use the aliases array\n    to match user intent to the correct key. Present the data\n    clearly and always include docs_url links.\n\nExample:\n    Call with key=\"access_token\" to inspect one setting, or\n    call without key to list all configurable options.",
        input_schema = inlined_schema_for::<GetConfigParam>()
    )]
    async fn get_config(
        &self,
        Parameters(params): Parameters<GetConfigParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::get_config::handle(self, params).await
    }

    #[tool(
        description = "Write a CodeScene MCP Server configuration value.\n\nWhen to use:\n    Use this tool to persist or remove server configuration values\n    without editing config files manually.\n\nLimitations:\n    - Unknown keys are rejected.\n    - Client-level environment variables may still override runtime\n      behavior even after saving a config value.\n    - Some changes may require an MCP client restart.\n\nPersists the value to the config file and applies it to the\nrunning session immediately. To remove a value, pass an empty\nstring as the value.\n\nIf the same setting is also defined as an environment variable in\nyour MCP client configuration (e.g. VS Code settings or Claude\nDesktop config), the environment variable takes precedence at\nruntime.\n\nCall get_config first (without a key) to discover available keys,\ntheir aliases, and docs_url links.\n\nReturns:\n    A JSON string with status (\"saved\" or \"removed\"), key,\n    config_dir, and optional warning, restart_required, and\n    docs_url fields. Present the data clearly and always\n    include docs_url links.\n\nExample:\n    Call with key=\"access_token\" and value=\"...\" to save,\n    or pass an empty value to remove that key from config.",
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
        .with_instructions(build_instructions(
            self.is_standalone,
            config::enabled_tools(&self.config_data).is_some(),
        ))
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

fn build_instructions(is_standalone: bool, tools_filtered: bool) -> String {
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

    if tools_filtered {
        text.push_str(
            "\nNote: Tool availability is restricted by the 'enabled_tools' configuration. \
             Use get_config with key 'enabled_tools' to see the current setting.\n",
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
        let schema = inlined_schema_for::<crate::tools::FilePathParam>();
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

    #[test]
    fn enabled_tools_unset_keeps_all_tools() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        let server = make_server(false);
        let names = tool_names(&server);
        // All 16 tools should be present
        assert_eq!(names.len(), 16, "expected 16 tools, got: {:?}", names);
        assert!(names.contains(&"get_config".to_string()));
        assert!(names.contains(&"set_config".to_string()));
        assert!(names.contains(&"code_health_review".to_string()));
        assert!(names.contains(&"select_project".to_string()));
    }

    #[test]
    fn enabled_tools_filters_to_allowlist() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        let server =
            make_server_with_enabled_tools(false, "code_health_review,code_health_score");
        let names = tool_names(&server);
        // Should have the 2 enabled tools + 2 always-on = 4
        assert_eq!(names.len(), 4, "expected 4 tools, got: {:?}", names);
        assert!(names.contains(&"code_health_review".to_string()));
        assert!(names.contains(&"code_health_score".to_string()));
        assert!(names.contains(&"get_config".to_string()));
        assert!(names.contains(&"set_config".to_string()));
    }

    #[test]
    fn enabled_tools_cannot_remove_config_tools() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        // Only enable one tool — config tools must still be present
        let server = make_server_with_enabled_tools(false, "code_health_review");
        let names = tool_names(&server);
        assert!(names.contains(&"get_config".to_string()));
        assert!(names.contains(&"set_config".to_string()));
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
        assert!(names.contains(&"get_config".to_string()));
        assert!(names.contains(&"set_config".to_string()));
    }

    #[test]
    fn enabled_tools_single_tool() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENABLED_TOOLS");
        let server = make_server_with_enabled_tools(false, "analyze_change_set");
        let names = tool_names(&server);
        assert_eq!(names.len(), 3, "expected 3 tools, got: {:?}", names);
        assert!(names.contains(&"analyze_change_set".to_string()));
        assert!(names.contains(&"get_config".to_string()));
        assert!(names.contains(&"set_config".to_string()));
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
        Ok(CliAction::PrintCliVersion) => {
            let output = fetch_cli_version(&cli::ProductionCliRunner).await?;
            print!("{output}");
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

    startup::print_startup_logo();
    tracing::info!("CodeScene MCP server started");
    tracing::info!("Waiting for MCP client initialization...");

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
        .inspect_err(|e| {
            if e.to_string().contains("initialize request") {
                tracing::info!("No MCP initialize request received. If you ran `cs-mcp` directly in a terminal, run it through an MCP client instead.");
            }
            tracing::error!("serving error: {:?}", e);
        })?;

    tracing::info!("CodeScene MCP server ready");

    service.waiting().await?;
    Ok(())
}

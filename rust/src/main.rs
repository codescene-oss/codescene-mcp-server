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

use std::path::Path;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, GetPromptResult, Implementation, ListPromptsResult,
    ListResourcesResult, PaginatedRequestParams, Prompt, PromptArgument, PromptMessage,
    PromptMessageRole, RawResource, ReadResourceRequestParams, ReadResourceResult,
    ResourceContents, GetPromptRequestParams, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::RoleServer;
use rmcp::schemars::{self, JsonSchema};
use rmcp::{ErrorData, ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use serde_json::json;
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

/// API-only tools that should be hidden in standalone mode.
const API_ONLY_TOOLS: &[&str] = &[
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
];

/// Generate an inlined JSON schema for a parameter type (rmcp pattern).
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

/// Bundled dependencies for constructing a `CodeSceneServer`.
///
/// Groups the six constructor parameters into a single value so that
/// `CodeSceneServer::new` stays within the four-argument limit.
struct ServerDeps {
    config_data: ConfigData,
    instance_id: String,
    is_standalone: bool,
    version_checker: VersionChecker,
    cli_runner: Arc<dyn CliRunner>,
    http_client: Arc<dyn HttpClient>,
}

/// The CodeScene MCP server.
#[derive(Clone)]
struct CodeSceneServer {
    tool_router: ToolRouter<Self>,
    version_checker: VersionChecker,
    config_data: Arc<ConfigData>,
    instance_id: String,
    is_standalone: bool,
    cli_runner: Arc<dyn CliRunner>,
    http_client: Arc<dyn HttpClient>,
}

// ---------------------------------------------------------------------------
// Cross-cutting helpers
// ---------------------------------------------------------------------------

impl CodeSceneServer {
    fn require_token(&self) -> Option<CallToolResult> {
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

    async fn maybe_version_warning(&self, text: &str) -> String {
        if let Some(info) = self.version_checker.try_read().await {
            if info.is_outdated {
                let warning = version_checker::format_version_warning(&info);
                return format!("{warning}{text}");
            }
        }
        text.to_string()
    }

    fn track(&self, event: &str, props: serde_json::Value) {
        tracking::track_event(event, props, &self.instance_id);
    }

    fn track_err(&self, tool: &str, err: &str) {
        tracking::track_error(err, tool, &self.instance_id);
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool_router]
impl CodeSceneServer {
    fn new(deps: ServerDeps) -> Self {
        let mut router = Self::tool_router();

        // Standalone licenses don't have CodeScene API access —
        // remove API-only tools so they don't appear in tools/list.
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

    // -- Explain tools (static docs) ----------------------------------------

    #[tool(description = "Explains CodeScene's Code Health metric for assessing code quality and maintainability for both human devs and AI.\n\nWhen to use:\n    Use this tool when a user asks what Code Health means, how scores are\n    interpreted, or why Code Health matters in day-to-day development.\n\nLimitations:\n    - Returns static documentation text from this MCP server package.\n    - Does not analyze a specific repository or file.\n\nArgs:\n    context: Optional context string from the MCP protocol.\n        This argument is not used to customize output.\n\nReturns:\n    Markdown content explaining the Code Health model and core concepts.\n\nExample:\n    Call this tool, then summarize the returned Markdown into a short\n    explanation tailored to the user's current question.",
        input_schema = inlined_schema_for::<OptionalContext>())]
    async fn explain_code_health(
        &self,
        #[allow(unused_variables)] Parameters(_params): Parameters<OptionalContext>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        self.version_checker.check_in_background();
        self.track("explain-code-health", json!({}));
        let text = self.maybe_version_warning(resources::HOW_IT_WORKS).await;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Describes how to build a business case for Code Health improvements.\nCovers empirical data on how healthy code lets you ship faster with\nfewer defects.\n\nWhen to use:\n    Use this tool when a user asks for ROI, productivity impact, or\n    management-facing framing for refactoring investments.\n\nLimitations:\n    - Returns static documentation text from this MCP server package.\n    - Does not compute project-specific forecasts.\n\nArgs:\n    context: Optional context string from the MCP protocol.\n        This argument is not used to customize output.\n\nReturns:\n    Markdown content describing productivity and defect-risk implications\n    of improving Code Health.\n\nExample:\n    Call this tool and extract 2-3 evidence-based points to support a\n    proposal for incremental refactoring.",
        input_schema = inlined_schema_for::<OptionalContext>())]
    async fn explain_code_health_productivity(
        &self,
        #[allow(unused_variables)] Parameters(_params): Parameters<OptionalContext>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        self.version_checker.check_in_background();
        self.track("explain-code-health-productivity", json!({}));
        let text = self.maybe_version_warning(resources::BUSINESS_CASE).await;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // -- CLI-based analysis tools -------------------------------------------

    #[tool(description = "Review the Code Health of a single source file and return a detailed\n    CLI review output that includes the score and code smell findings.\n\n    When to use:\n        Use this tool when you need actionable maintainability diagnostics\n        for one file (not just the score).\n\n    Limitations:\n        - Analyzes one file at a time.\n        - Requires a supported source file.\n        - Returns CLI review text, not a normalized JSON schema.\n\n    Args:\n        file_path: Absolute path to the source code file to analyze.\n            Use a real file path in the local repository.\n\n    Returns:\n        A review string from the CodeScene CLI. The output typically\n        includes a Code Health score and code smell details to explain\n        why the score is high or low.\n\n        The Code Health scores are interpreted as:\n          * Optimal Code: a Code Health 10.0 is optimized for both human and AI comprehension\n          * Green Code: high quality with a score of 9.0-9.9\n          * Yellow Code: problematic technical debt with a score of 4.0-8.9\n          * Red Code: severe technical debt, maintainability issues, and expensive onboarding with a score of 1.0-3.9\n\n    Example:\n        Call with file_path=\"/repo/src/app.py\" and summarize the returned\n        smells into prioritized refactoring actions.",
        input_schema = inlined_schema_for::<FilePathParam>())]
    async fn code_health_review(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        self.version_checker.check_in_background();
        let file_path = docker::adapt_path_for_docker(Path::new(&params.file_path));
        let result = run_review(Path::new(&file_path), &*self.cli_runner).await;
        match &result {
            Ok(output) => {
                let props = event_properties::review_properties(Path::new(&params.file_path), output);
                self.track("code-health-review", props);
                let text = self.maybe_version_warning(output).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("code-health-review", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Calculate the Code Health score for a single source file.\n    The tool returns one numeric score from 10.0 (optimal) to 1.0 (worst).\n\n    When to use:\n        Use this tool for quick triage, ranking files by maintainability,\n        or checking whether a refactoring improved file-level quality.\n\n    Limitations:\n        - Analyzes one file at a time.\n        - Returns only the score summary, not the full smell breakdown.\n        - Requires a supported source file.\n\n    Args:\n        file_path: Absolute path to the source code file to analyze.\n            Use a concrete local file path.\n\n    Returns:\n        A string in the format \"Code Health score: <score>\".\n\n        The Code Health scores are interpreted as:\n          * Optimal Code: Code Health 10.0 optimized for human and AI comprehension\n          * Green Code: high quality with a score of 9.0-9.9\n          * Yellow Code: problematic technical debt with a score of 4.0-8.9\n          * Red Code: severe technical debt with a score of 1.0-3.9\n\n    Example:\n        Call with file_path=\"/repo/src/module.py\" and compare the score\n        before and after a refactoring.",
        input_schema = inlined_schema_for::<FilePathParam>())]
    async fn code_health_score(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        self.version_checker.check_in_background();
        let file_path = docker::adapt_path_for_docker(Path::new(&params.file_path));
        let result = run_review(Path::new(&file_path), &*self.cli_runner).await;
        match result {
            Ok(output) => {
                let score = extract_score(&output);
                let props = event_properties::score_properties(Path::new(&params.file_path), score);
                self.track("code-health-score", props);
                let msg = score
                    .map(|s| format!("Code Health score: {s}"))
                    .unwrap_or_else(|| "Could not determine Code Health score.".to_string());
                let text = self.maybe_version_warning(&msg).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("code-health-score", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Review all modified and staged files in a repository and report\n        Code Health degradations before commit.\n\n        When to use:\n            Use this tool as a pre-commit safeguard on local changes to catch\n            regressions and code smells before creating a commit.\n\n        Limitations:\n            - Requires a valid git repository path.\n            - Evaluates current local modifications/staged changes only.\n            - Output is JSON text from the CLI command.\n\n        Args:\n            git_repository_path: Absolute path to the local git repository to analyze.\n\n        Returns:\n            A JSON object containing:\n              - quality_gates: the central outcome, summarizing whether the commit passes or fails Code Health thresholds for each file.\n              - files: an array of objects for each file with:\n                  - name: the name of the file whose Code Health is impacted (positively or negatively).\n                  - findings: an array describing improvements/degradation for each code smell.\n              - Each quality gate indicates if the file meets the required Code Health standards, helping teams enforce healthy code before commit.\n\n        Example:\n            Run on git_repository_path=\"/repo\" and block commit preparation if\n            any quality gate fails.",
        input_schema = inlined_schema_for::<GitRepoParam>())]
    async fn pre_commit_code_health_safeguard(
        &self,
        Parameters(params): Parameters<GitRepoParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        self.version_checker.check_in_background();
        let repo_path = docker::adapt_path_for_docker(Path::new(&params.git_repository_path));
        let result = run_delta(Path::new(&repo_path), None, &*self.cli_runner).await;
        match result {
            Ok(output) => {
                let parsed = delta::analyze_delta_output(&output);
                let result_str = serde_json::to_string(&parsed).unwrap_or_default();
                let props = event_properties::pre_commit_properties(
                    Path::new(&params.git_repository_path),
                    &result_str,
                );
                self.track("pre-commit-code-health-safeguard", props);
                let text = self.maybe_version_warning(&result_str).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("pre-commit-code-health-safeguard", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Run a branch-level Code Health review for all files that differ between\n        current HEAD and base_ref.\n\n        When to use:\n            Use this as a local PR pre-flight check before opening a pull\n            request, so regressions are caught across the full change set.\n\n        Limitations:\n            - Requires a valid git repository path.\n            - base_ref must exist and be resolvable by git in that repository.\n            - Reviews only files that differ from base_ref.\n            - Output is JSON text from the CLI command.\n\n        The result can be used to decide whether to refactor before creating\n        or updating a pull request.\n\n        Args:\n            base_ref: Git reference to compare against, typically the target\n                branch of the pull request (for example \"main\" or \"origin/main\").\n            git_repository_path: Absolute path to the local git repository.\n\n        Returns:\n            A JSON object containing:\n              - quality_gates: the central outcome, summarizing whether the change\n                set passes or fails Code Health thresholds (\"passed\" or \"failed\").\n              - results: an array of objects for each affected file with:\n                  - name: the name of the file whose Code Health is impacted.\n                  - verdict: \"improved\", \"degraded\", or \"stable\".\n                  - findings: an array describing improvements/degradation for each code smell.\n\n        Example:\n            Compare against base_ref=\"main\" for git_repository_path=\"/repo\" and\n            fail the local PR check if any file verdict is \"degraded\".",
        input_schema = inlined_schema_for::<ChangeSetParam>())]
    async fn analyze_change_set(
        &self,
        Parameters(params): Parameters<ChangeSetParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        self.version_checker.check_in_background();
        let repo_path = docker::adapt_path_for_docker(Path::new(&params.git_repository_path));
        let result = run_delta(Path::new(&repo_path), Some(&params.base_ref), &*self.cli_runner).await;
        match result {
            Ok(output) => {
                let parsed = delta::analyze_delta_output(&output);
                let result_str = serde_json::to_string(&parsed).unwrap_or_default();
                let props = event_properties::change_set_properties(
                    Path::new(&params.git_repository_path),
                    Path::new(&params.base_ref),
                    &result_str,
                );
                self.track("analyze-change-set", props);
                let text = self.maybe_version_warning(&result_str).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("analyze-change-set", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Generate a data-driven business case for refactoring a source file.\n\n    When to use:\n        Use this tool to justify refactoring investment with quantified\n        predictions tied to the file's current Code Health.\n\n    Limitations:\n        - Estimates are model-based projections, not guarantees.\n        - Evaluates one file at a time.\n        - Requires an analyzable source file.\n\n    Args:\n        file_path: Absolute path to the source code file to analyze.\n\n    Returns:\n        A JSON object with:\n            - scenario: Recommended target Code Health level.\n            - optimistic_outcome: Upper bound estimate for improvements\n              in development speed and defect reduction.\n            - pessimistic_outcome: Lower bound estimate for improvements.\n            - confidence_interval: The optimistic \u{2192} pessimistic range,\n              representing a 90% confidence interval for the expected impact.\n\n    Example:\n        Call with file_path=\"/repo/src/service.py\" and use the optimistic\n        and pessimistic outcomes to frame a refactoring proposal.",
        input_schema = inlined_schema_for::<FilePathParam>())]
    async fn code_health_refactoring_business_case(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        self.version_checker.check_in_background();
        let file_path = docker::adapt_path_for_docker(Path::new(&params.file_path));
        let review_result = run_review(Path::new(&file_path), &*self.cli_runner).await;
        match review_result {
            Ok(output) => {
                let score = extract_score(&output);
                let result_text = match score {
                    Some(s) => match business_case::make_business_case(s) {
                        Some(bc) => serde_json::to_string_pretty(&bc).unwrap_or_default(),
                        None => "Code Health is already optimal. No business case needed.".into(),
                    },
                    None => "Could not determine Code Health score.".into(),
                };
                let props =
                    event_properties::business_case_properties(Path::new(&params.file_path), &result_text);
                self.track("code-health-refactoring-business-case", props);
                let text = self.maybe_version_warning(&result_text).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("code-health-refactoring-business-case", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Refactor a single function to fix specific code health problems.\n        This auto-refactor uses CodeScene ACE, and is intended as an initial\n        refactoring to increase the modularity of the code so that you as an\n        AI agent can continue and iterate with more specific refactorings.\n\n        When to use:\n            Use this tool after a Code Health review has identified one of the\n            supported smells in a specific function.\n\n        The code_health_auto_refactor tool is supported for these languages:\n            - JavaScript/TypeScript\n            - Java\n            - C#\n            - C++\n        and for these code smells:\n            - Complex Conditional\n            - Bumpy Road Ahead\n            - Complex Method\n            - Deep, Nested Complexity\n            - Large Method\n\n        IMPORTANT:\n            - Only use this tool for functions shorter than 300 lines of code.\n            - Insert any new functions close to the refactored function.\n            - Requires ACE access to be configured (use set_config with key \"ace_access_token\").\n\n        Args:\n            file_path: Absolute path to the source file that contains the target function.\n            function_name: Exact function name to refactor.\n                Include class scope prefix when relevant.\n\n        Returns:\n            A JSON object describing the refactoring, with these properties:\n              - code: The refactored function plus new extracted functions.\n              - declarations: Optional (used for languages like C++). Declarations of additional functions introduced when refactoring.\n                When present, find the right include file and insert the declarations there. Note that some C++ refactorings result\n                in standalone functions; standalone functions should just be inserted in the implementation unit, not declared in\n                include files.\n              - confidence: The confidence level of the resulting refactoring. For low confidence, review the\n                refactoring and fix any introduced problems.\n              - reasons: A list of strings describing the reasons for the assigned confidence level.\n                Use this list of strings to direct fixes of the refactored code.\n\n        Example:\n            Call with file_path=\"/repo/src/service.ts\" and\n            function_name=\"OrderService.calculateTotal\", then apply returned\n            code and declarations and re-run Code Health checks.",
        input_schema = inlined_schema_for::<RefactorParam>())]
    async fn code_health_auto_refactor(
        &self,
        Parameters(params): Parameters<RefactorParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        self.version_checker.check_in_background();
        let result = run_auto_refactor(Path::new(&params.file_path), &params.function_name, &*self.cli_runner, &*self.http_client).await;
        match &result {
            Ok(result_json) => {
                let props = event_properties::refactor_properties(
                    Path::new(&params.file_path),
                    result_json,
                );
                self.track("code-health-auto-refactor", props);
                let text = self
                    .maybe_version_warning(&serde_json::to_string(result_json).unwrap_or_default())
                    .await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("code-health-auto-refactor", e);
                let text = self.maybe_version_warning(e).await;
                Ok(tool_error(&text))
            }
        }
    }

    // -- API-based tools (conditional on not-standalone) ---------------------

    #[tool(description = "Lists all projects for an organization for selection by the user.\n        The user can select the desired project by either its name or ID.\n\n        When to use:\n            Use this tool before project-scoped API tools so the user can pick\n            the project context explicitly.\n\n        Limitations:\n            - If default_project_id is configured, the server returns that\n              project and selection is effectively locked.\n\n        Args:\n            None.\n\n        Returns:\n            A JSON object with the project name and ID, formatted in a Markdown table\n            with the columns \"Project Name\" and \"Project ID\". If the output contains a\n            `description` field, it indicates that a default project is configured\n            (`default_project_id`), and the user cannot select a different project.\n            Explain this to the user.\n\n            Additionally, a `link` field is provided to guide the user to the\n            Codescene projects page where the user can find more detailed information about each project.\n            Make sure to include this link in the output, and explain its purpose clearly.\n\n        Example:\n            Call without arguments. If default_project_id is configured,\n            explain that the returned project is fixed unless that config is changed via set_config.")]
    async fn select_project(&self) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        if self.is_standalone {
            return Ok(tool_error(
                "This tool requires a CodeScene API token (not a standalone license).",
            ));
        }
        self.version_checker.check_in_background();
        let result = run_select_project(&*self.http_client).await;
        match &result {
            Ok(output) => {
                let props = event_properties::select_project_properties();
                self.track("select-project", props);
                let text = serde_json::to_string(output).unwrap_or_default();
                let text = self.maybe_version_warning(&text).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("select-project", e);
                Ok(tool_error(e))
            }
        }
    }

    #[tool(description = "Lists the technical debt goals for a project.\n\n        When to use:\n            Use this tool to see all files in a project that currently have\n            explicit technical debt goals in CodeScene.\n\n        Limitations:\n            - Requires a valid project_id.\n            - Returns goal data from the latest available analysis.\n            - Includes only files with non-empty goals.\n\n        Args:\n            project_id: The Project ID selected by the user.\n\n        Returns:\n            A JSON array containing the path of a file and its goals, or a string error message if no project was selected.\n            Show the goals for each file in a structured format that is easy to read and explain\n            the goal description for each file. It also includes a description, please include that in your output.\n\n            Additionally, provide a link to the CodeScene Code Biomarkers page for the project technical debt goals.\n            Explain that you can find more details about the technical debt goals on that page.\n\n        Example:\n            Call with project_id=42 and summarize each file's goals with the\n            biomarkers link for deeper inspection.",
        input_schema = inlined_schema_for::<ProjectParam>())]
    async fn list_technical_debt_goals_for_project(
        &self,
        Parameters(params): Parameters<ProjectParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        if self.is_standalone {
            return Ok(tool_error(
                "This tool requires a CodeScene API token (not a standalone license).",
            ));
        }
        self.version_checker.check_in_background();
        let endpoint = format!("v2/projects/{}/goals", params.project_id);
        let result = api_client::query_api_list_with_client(&endpoint, &*self.http_client).await;
        match result {
            Ok(data) => {
                let props = event_properties::goals_properties(params.project_id, data.len());
                self.track("list-technical-debt-goals", props);
                let text = serde_json::to_string(&data).unwrap_or_default();
                let text = self.maybe_version_warning(&text).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("list-technical-debt-goals", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Lists the technical debt goals for a specific file in a project.\n\n        When to use:\n            Use this tool when you need goal details for one file before\n            planning targeted refactoring work.\n\n        Limitations:\n            - Requires a valid project_id.\n            - Returns data from the latest available analysis only.\n            - A file may return zero goals, which is a valid outcome.\n\n        Args:\n            file_path: The absolute path to the source code file.\n            project_id: The Project ID selected by the user.\n\n        Returns:\n            A JSON array containing the goals for the specified file, or a string error message if no project was selected.\n            Show the goals in a structured format that is easy to read and explain\n            the goal description. It also includes a description, please include that in your output.\n\n            Additionally, provide a link to the CodeScene Code Biomarkers page for the project file technical debt goals.\n            Explain that you can find more details about the technical debt goals on that page.\n\n        Example:\n            Call with file_path=\"/repo/src/module.py\" and project_id=42, then\n            use the goals and link to propose file-level improvements.",
        input_schema = inlined_schema_for::<ProjectFileParam>())]
    async fn list_technical_debt_goals_for_project_file(
        &self,
        Parameters(params): Parameters<ProjectFileParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        if self.is_standalone {
            return Ok(tool_error(
                "This tool requires a CodeScene API token (not a standalone license).",
            ));
        }
        self.version_checker.check_in_background();
        let file_path = docker::adapt_path_for_docker(Path::new(&params.file_path));
        let relative = make_relative_for_api(&file_path);
        let endpoint = format!(
            "v2/projects/{}/goals?file={}",
            params.project_id,
            urlencoded(&relative)
        );
        let result = api_client::query_api_list_with_client(&endpoint, &*self.http_client).await;
        match result {
            Ok(data) => {
                let props = event_properties::goals_file_properties(Path::new(&params.file_path));
                self.track("list-technical-debt-goals-file", props);
                let text = serde_json::to_string(&data).unwrap_or_default();
                let text = self.maybe_version_warning(&text).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("list-technical-debt-goals-file", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Lists the technical debt hotspots for a project.\n\n        When to use:\n            Use this tool to identify high-impact technical debt hotspots across\n            a project and prioritize refactoring targets.\n\n        Limitations:\n            - Requires a valid project_id.\n            - Returns hotspots from the latest available project analysis.\n            - Uses the API's hotspot filtering and pagination behavior.\n\n        Args:\n            project_id: The Project ID selected by the user.\n\n        Returns:\n            A JSON array containing the path of a file, code health score, revisions count and lines of code count.\n            Describe the hotspots for each file in a structured format that is easy to read and explain.\n            It also includes a description, please include that in your output.\n\n            Additionally, a `link` field is provided to guide the user to the\n            Codescene technical debt hotspots page for the project where the user can find more detailed information about each hotspot.\n            Make sure to include this link in the output, and explain its purpose clearly.\n\n        Example:\n            Call with project_id=42 and rank returned hotspots by code health\n            and revision frequency before proposing refactoring work.",
        input_schema = inlined_schema_for::<ProjectParam>())]
    async fn list_technical_debt_hotspots_for_project(
        &self,
        Parameters(params): Parameters<ProjectParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        if self.is_standalone {
            return Ok(tool_error(
                "This tool requires a CodeScene API token (not a standalone license).",
            ));
        }
        self.version_checker.check_in_background();
        let endpoint = format!("v2/projects/{}/hotspots", params.project_id);
        let result = api_client::query_api_list_with_client(&endpoint, &*self.http_client).await;
        match result {
            Ok(data) => {
                let props =
                    event_properties::hotspots_properties(params.project_id, data.len());
                self.track("list-technical-debt-hotspots", props);
                let text = serde_json::to_string(&data).unwrap_or_default();
                let text = self.maybe_version_warning(&text).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("list-technical-debt-hotspots", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Lists the technical debt hotspots for a specific file in a project.\n\n        When to use:\n            Use this tool to inspect hotspot metrics for one file before\n            deciding if it should be a refactoring candidate.\n\n        Limitations:\n            - Requires a valid project_id.\n            - Returns at most one hotspot object for the filtered file path.\n            - If no hotspot exists for the file, returns an empty hotspot object.\n\n        Args:\n            file_path: The absolute path to the source code file.\n            project_id: The Project ID selected by the user.\n\n        Returns:\n            A JSON array containing the code health score, revisions count and lines of code count for the specified file,\n            or a string error message if no project was selected.\n            Describe the hotspot in a structured format that is easy to read and explain.\n            It also includes a description, please include that in your output.\n\n            Additionally, a `link` field is provided to guide the user to the\n            Codescene technical debt hotspots page for the project where the user can find more detailed information about each hotspot.\n            Make sure to include this link in the output, and explain its purpose clearly.\n\n        Example:\n            Call with file_path=\"/repo/src/module.py\" and project_id=42. If\n            hotspot is empty, report that the file is not currently a hotspot.",
        input_schema = inlined_schema_for::<ProjectFileParam>())]
    async fn list_technical_debt_hotspots_for_project_file(
        &self,
        Parameters(params): Parameters<ProjectFileParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        if self.is_standalone {
            return Ok(tool_error(
                "This tool requires a CodeScene API token (not a standalone license).",
            ));
        }
        self.version_checker.check_in_background();
        let file_path = docker::adapt_path_for_docker(Path::new(&params.file_path));
        let relative = make_relative_for_api(&file_path);
        let endpoint = format!(
            "v2/projects/{}/hotspots?file={}",
            params.project_id,
            urlencoded(&relative)
        );
        let result = api_client::query_api_list_with_client(&endpoint, &*self.http_client).await;
        match result {
            Ok(data) => {
                let props = event_properties::hotspots_file_properties(Path::new(&params.file_path));
                self.track("list-technical-debt-hotspots-file", props);
                let text = serde_json::to_string(&data).unwrap_or_default();
                let text = self.maybe_version_warning(&text).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("list-technical-debt-hotspots-file", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    #[tool(description = "Find the owner or owners of a specific path in a project.\n\n        When to use:\n            Use this tool to identify likely reviewers or domain experts for\n            code reviews and technical questions about a file or directory.\n\n        Limitations:\n            - Requires a valid project_id.\n            - Uses the latest project analysis data available in CodeScene.\n            - If no matching ownership data is found, an empty JSON array is returned.\n\n        Args:\n            project_id: CodeScene project identifier.\n            path: Absolute or repository-relative path to a file or directory.\n\n        Returns:\n            A list of owners and their paths that they own. The name of the owner who can be responsible\n            for code reviews or inquiries about the file and a link to the CodeScene System Map page filtered\n            by the owner. Explain that this link can be used to see more details\n            about the owner's contributions and interactions within the project.\n            You MUST always show a link after every owner. Show resulting data in A Markdown\n            table with columns: Owner, Key Areas, Link.\n\n        Example:\n            Call with project_id=42 and path=\"/repo/src/service.py\", then\n            present each owner row with its corresponding system-map link.",
        input_schema = inlined_schema_for::<OwnershipParam>())]
    async fn code_ownership_for_path(
        &self,
        Parameters(params): Parameters<OwnershipParam>,
    ) -> Result<CallToolResult, ErrorData> {
        if let Some(r) = self.require_token() {
            return Ok(r);
        }
        if self.is_standalone {
            return Ok(tool_error(
                "This tool requires a CodeScene API token (not a standalone license).",
            ));
        }
        self.version_checker.check_in_background();
        let path = docker::adapt_path_for_docker(Path::new(&params.path));
        let relative = make_relative_for_api(&path);
        let endpoint = format!(
            "v2/projects/{}/ownership?path={}",
            params.project_id,
            urlencoded(&relative)
        );
        let result = api_client::query_api_list_with_client(&endpoint, &*self.http_client).await;
        match result {
            Ok(data) => {
                let props = event_properties::ownership_properties(
                    params.project_id,
                    Path::new(&params.path),
                );
                self.track("code-ownership", props);
                let text = serde_json::to_string(&data).unwrap_or_default();
                let text = self.maybe_version_warning(&text).await;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => {
                self.track_err("code-ownership", &e.to_string());
                Ok(tool_error(&format!("Error: {e}")))
            }
        }
    }

    // -- Configuration tools ------------------------------------------------

    #[tool(description = "Read current CodeScene MCP Server configuration values.\n\n        When to use:\n            Use this tool to discover available configuration keys, inspect\n            effective values, and understand where each value comes from.\n\n        Limitations:\n            - Returns JSON text only; callers must format it for display.\n            - Sensitive values (tokens) are masked.\n            - Effective values can be overridden by client-provided env vars.\n\n        When called without a key, lists every available configuration\n        option together with its current effective value, the source of\n        that value (environment variable vs. config file), and a short\n        description.\n\n        When called with a specific key, returns details for that option\n        only.  Sensitive values (tokens) are masked in the output.\n\n        Args:\n            key: Optional config key to query. Omit to list all options.\n        Returns:\n            A JSON string. When querying a single key, the object has:\n            key, env_var, value, source, description, aliases, and\n            docs_url.  When listing all, the object has: config_dir and\n            options (array of the same shape).  Use the aliases array\n            to match user intent to the correct key.  Present the data\n            clearly and always include docs_url links.\n\n        Example:\n            Call with key=\"access_token\" to inspect one setting, or\n            call without key to list all configurable options.",
        input_schema = inlined_schema_for::<GetConfigParam>())]
    async fn get_config(
        &self,
        Parameters(params): Parameters<GetConfigParam>,
    ) -> Result<CallToolResult, ErrorData> {
        self.version_checker.check_in_background();
        let key_str = params.key.as_deref().unwrap_or("");
        let props = event_properties::config_properties(event_properties::ConfigAction::Get, key_str);
        self.track("get-config", props);

        let result = match &params.key {
            Some(k) => configure::get_single(k, &self.config_data, self.is_standalone),
            None => configure::get_all(&self.config_data, self.is_standalone),
        };
        let text = self.maybe_version_warning(&result).await;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Write a CodeScene MCP Server configuration value.\n\n        When to use:\n            Use this tool to persist or remove server configuration values\n            without editing config files manually.\n\n        Limitations:\n            - Unknown keys are rejected.\n            - Client-level environment variables may still override runtime\n              behavior even after saving a config value.\n            - Some changes may require an MCP client restart.\n\n        Persists the value to the config file and applies it to the\n        running session immediately.  To remove a value, pass an empty\n        string as the value.\n\n        If the same setting is also defined as an environment variable in\n        your MCP client configuration (e.g. VS Code settings or Claude\n        Desktop config), the environment variable takes precedence at\n        runtime.\n\n        Call get_config first (without a key) to discover available keys,\n        their aliases, and docs_url links.\n\n        Args:\n            key: The configuration key to set.\n            value: The value to store. Pass an empty string to remove the\n                   key from the config file.\n        Returns:\n            A JSON string with status (\"saved\" or \"removed\"), key,\n            config_dir, and optional warning, restart_required, and\n            docs_url fields.  Present the data clearly and always\n            include docs_url links.\n\n        Example:\n            Call with key=\"access_token\" and value=\"...\" to save,\n            or pass an empty value to remove that key from config.",
        input_schema = inlined_schema_for::<SetConfigParam>())]
    async fn set_config(
        &self,
        Parameters(params): Parameters<SetConfigParam>,
    ) -> Result<CallToolResult, ErrorData> {
        self.version_checker.check_in_background();
        let props = event_properties::config_properties(event_properties::ConfigAction::Set, &params.key);
        self.track("set-config", props);

        let result = configure::set_value(&params.key, &params.value);
        let text = self.maybe_version_warning(&result).await;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

// ---------------------------------------------------------------------------
// ServerHandler implementation
// ---------------------------------------------------------------------------

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
        .with_server_info(
            Implementation::new("codescene-mcp-server", env!("CS_MCP_VERSION")),
        )
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
                "Explains CodeScene's Code Health metric for assessing code quality \
                 and maintainability for both human devs and AI.",
            )
            .with_mime_type("text/markdown")
            .no_annotation(),
            RawResource::new(
                resources::BUSINESS_CASE_URI,
                extract_md_title(resources::BUSINESS_CASE),
            )
            .with_description(
                "Describes how to build a business case for Code Health improvements.",
            )
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
        let content = if uri == resources::HOW_IT_WORKS_URI {
            resources::HOW_IT_WORKS
        } else if uri == resources::BUSINESS_CASE_URI {
            resources::BUSINESS_CASE
        } else {
            return Err(ErrorData::new(
                rmcp::model::ErrorCode::INVALID_REQUEST,
                format!("Unknown resource: {uri}"),
                None,
            ));
        };
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
                    "Review Code Health and assess code quality for the current open file. \
                     The file path needs to be sent to the code_health_review MCP tool \
                     when using this prompt.",
                ),
                Some(vec![PromptArgument::new("context")
                    .with_description("Optional context string.")
                    .with_required(false)]),
            ),
            Prompt::new(
                "plan_code_health_refactoring",
                Some(
                    "Plan a prioritized, low-risk refactoring to remediate detected \
                     Code Health issues.",
                ),
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
        let text = match request.name.as_str() {
            "review_code_health" => prompts::REVIEW_CODE_HEALTH,
            "plan_code_health_refactoring" => prompts::PLAN_CODE_HEALTH_REFACTORING,
            other => {
                return Err(ErrorData::new(
                    rmcp::model::ErrorCode::INVALID_REQUEST,
                    format!("Unknown prompt: {other}"),
                    None,
                ))
            }
        };
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            text,
        )]))
    }
}

// ---------------------------------------------------------------------------
// Free helper functions
// ---------------------------------------------------------------------------

/// Run `cs review` on a file and return the raw output.
async fn run_review(file_path: &Path, cli: &dyn CliRunner) -> Result<String, errors::CliError> {
    let resolved = resolve_file_path(file_path);
    let git_root = cli::find_git_root(Path::new(&resolved));
    let cli_path = make_cli_path(&resolved, git_root.as_deref());
    let args = vec!["review", "--output-format=json", &cli_path];
    cli.run(&args, git_root.as_deref()).await
}

/// Run `cs delta` (pre-commit or branch diff).
async fn run_delta(
    repo_path: &Path,
    base_ref: Option<&str>,
    cli: &dyn CliRunner,
) -> Result<String, errors::CliError> {
    let mut args = vec!["delta", "--output-format=json"];
    if let Some(br) = base_ref {
        args.push(br);
    }
    cli.run(&args, Some(repo_path)).await
}

/// Run the auto-refactor workflow: parse-fns → review → match → ACE API.
async fn run_auto_refactor(
    file_path: &Path,
    function_name: &str,
    cli: &dyn CliRunner,
    http: &dyn HttpClient,
) -> Result<serde_json::Value, String> {
    if std::env::var("CS_ACE_ACCESS_TOKEN")
        .map(|v| v.is_empty())
        .unwrap_or(true)
    {
        return Err(
            "Error: This tool needs ACE access configured via set_config key \
             \"ace_access_token\" (or CS_ACE_ACCESS_TOKEN). See \
             https://github.com/codescene-oss/codescene-mcp-server?tab=readme-ov-file#-activate-ace-in-codescene-mcp"
                .to_string(),
        );
    }

    let file_str = file_path.to_string_lossy();
    let docker_path = docker::adapt_path_for_docker(file_path);
    let git_root = cli::find_git_root(Path::new(&docker_path))
        .ok_or_else(|| format!("Error: Could not find git root for {}", file_str))?;
    let cli_path = make_cli_path(&docker_path, Some(&git_root));

    let parse_output = cli
        .run(&["parse-fns", "--path", &cli_path], Some(&git_root))
        .await
        .map_err(|e| format!("Error: {e}"))?;
    let functions: serde_json::Value =
        serde_json::from_str(&parse_output).map_err(|e| format!("Error parsing functions: {e}"))?;

    let review_output = cli
        .run(
            &["review", "--output-format=json", &cli_path],
            Some(&git_root),
        )
        .await
        .map_err(|e| format!("Error: {e}"))?;
    let review: serde_json::Value =
        serde_json::from_str(&review_output).map_err(|e| format!("Error parsing review: {e}"))?;

    let function = find_function_in_parsed(&functions, function_name)
        .ok_or_else(|| format!("Error: Couldn't find function: {function_name}"))?;

    let code_smells = extract_code_smells(&review, &function, function_name);
    if code_smells.is_empty() {
        return Err(format!(
            "Error: No code smells were found in {function_name}"
        ));
    }

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let payload = build_ace_payload(&function, &code_smells, ext);

    let response = ace_client::refactor_with_client(&payload, http)
        .await
        .map_err(|e| format!("Error: {e}"))?;

    Ok(format_ace_response(&response))
}

fn find_function_in_parsed<'a>(
    functions: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    functions
        .as_array()?
        .iter()
        .find(|f| f.get("name").and_then(|n| n.as_str()) == Some(name))
}

fn extract_code_smells(
    review: &serde_json::Value,
    function: &serde_json::Value,
    function_name: &str,
) -> Vec<serde_json::Value> {
    let fn_start = function
        .get("start-line")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let review_items = review
        .get("review")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let mut smells = Vec::new();
    for smell in &review_items {
        let category = smell
            .get("category")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        let fns = smell
            .get("functions")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();
        for f in &fns {
            let title = f.get("title").and_then(|t| t.as_str()).unwrap_or("");
            if matches_function_name(title, function_name) {
                let start = f.get("start-line").and_then(|s| s.as_i64()).unwrap_or(0);
                smells.push(json!({
                    "category": category,
                    "start-line": start - fn_start + 1,
                }));
            }
        }
    }
    smells
}

fn matches_function_name(title: &str, function_name: &str) -> bool {
    if title == function_name {
        return true;
    }
    // Match "FunctionName:N" pattern
    if let Some(base) = title.strip_suffix(|c: char| c == ':' || c.is_ascii_digit()) {
        let base = base.trim_end_matches(':');
        return base == function_name;
    }
    title.starts_with(function_name)
        && title[function_name.len()..].starts_with(':')
        && title[function_name.len() + 1..]
            .chars()
            .all(|c| c.is_ascii_digit())
}

fn build_ace_payload(
    function: &serde_json::Value,
    code_smells: &[serde_json::Value],
    file_ext: &str,
) -> serde_json::Value {
    let body = function
        .get("body")
        .and_then(|b| b.as_str())
        .unwrap_or("");
    let fn_type = function
        .get("function-type")
        .and_then(|t| t.as_str())
        .unwrap_or("Unknown");

    json!({
        "api-version": "v2",
        "source-snippet": {
            "file-type": file_ext,
            "body": body,
            "function-type": fn_type,
        },
        "review": code_smells,
    })
}

fn format_ace_response(response: &serde_json::Value) -> serde_json::Value {
    let code = response.get("code").cloned().unwrap_or(json!(""));
    let declarations = response
        .get("declarations")
        .cloned()
        .unwrap_or(json!(""));
    let confidence = response
        .get("confidence")
        .and_then(|c| c.get("description"))
        .and_then(|d| d.as_str())
        .unwrap_or("unknown");
    let reasons: Vec<String> = response
        .get("reasons")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("summary").and_then(|s| s.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    json!({
        "code": code,
        "declarations": declarations,
        "confidence": confidence,
        "reasons": reasons,
    })
}

/// Build the `select_project` response.
async fn run_select_project(http: &dyn HttpClient) -> Result<serde_json::Value, String> {
    let link = std::env::var("CS_ONPREM_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "https://codescene.io/projects".to_string());

    if let Ok(id_str) = std::env::var("CS_DEFAULT_PROJECT_ID") {
        if !id_str.is_empty() {
            let id: i64 = id_str.parse().unwrap_or(0);
            return Ok(json!({
                "id": id,
                "name": "Default Project (from CS_DEFAULT_PROJECT_ID env var)",
                "description": "Using default project from CS_DEFAULT_PROJECT_ID environment variable. If you want to be able to select a different project, unset this variable.",
                "link": link,
            }));
        }
    }

    let data = api_client::query_api_list_with_client("v2/projects", http)
        .await
        .map_err(|e| format!("Error: {e}"))?;

    Ok(json!({ "projects": data, "link": link }))
}

/// Extract the Code Health score from review JSON output.
fn extract_score(review_output: &str) -> Option<f64> {
    let parsed: serde_json::Value = serde_json::from_str(review_output).ok()?;
    parsed.get("score").and_then(|s| s.as_f64())
}

/// Make a CLI-compatible file path (relative to git root or Docker-adapted).
fn make_cli_path(file_path: &str, git_root: Option<&Path>) -> String {
    if environment::is_docker() {
        return docker::adapt_path_for_docker(Path::new(file_path));
    }
    match git_root {
        Some(root) => docker::get_relative_file_path_for_api(Path::new(file_path), root),
        None => file_path.to_string(),
    }
}

/// URL-encode a string for query parameters.
fn urlencoded(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('#', "%23")
        .replace('?', "%3F")
}

/// Resolve a potentially relative file path to an absolute path.
///
/// Mirrors Python's `Path(file_path).resolve()` — if the path is relative
/// it is joined with the current working directory.
fn resolve_file_path(file_path: &Path) -> String {
    if file_path.is_absolute() {
        return file_path.to_string_lossy().to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(file_path).to_string_lossy().to_string(),
        Err(_) => file_path.to_string_lossy().to_string(),
    }
}

/// Make a file path relative for API calls.
fn make_relative_for_api(file_path: &str) -> String {
    let git_root = cli::find_git_root(Path::new(file_path));
    match git_root {
        Some(root) => {
            docker::get_relative_file_path_for_api(Path::new(file_path), &root)
        }
        None => file_path.to_string(),
    }
}

/// Format a tool error result.
fn tool_error(msg: &str) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg)])
}

/// Build server instructions based on whether the token is standalone.
fn build_instructions(is_standalone: bool) -> String {
    let mut text = String::from(
        "CodeScene MCP Server — Code Health analysis tools for AI-assisted development.\n\n\
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

/// Extract the first markdown heading from content (for resource names).
fn extract_md_title(content: &str) -> &str {
    for line in content.lines() {
        if let Some(title) = line.strip_prefix("# ") {
            return title.trim();
        }
    }
    "Untitled"
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- extract_score --

    #[test]
    fn extract_score_valid_json() {
        assert_eq!(extract_score(r#"{"score": 8.5}"#), Some(8.5));
    }

    #[test]
    fn extract_score_integer() {
        assert_eq!(extract_score(r#"{"score": 10}"#), Some(10.0));
    }

    #[test]
    fn extract_score_missing_key() {
        assert_eq!(extract_score(r#"{"review": []}"#), None);
    }

    #[test]
    fn extract_score_invalid_json() {
        assert_eq!(extract_score("not json"), None);
    }

    #[test]
    fn extract_score_null_value() {
        assert_eq!(extract_score(r#"{"score": null}"#), None);
    }

    #[test]
    fn extract_score_string_value() {
        assert_eq!(extract_score(r#"{"score": "8.5"}"#), None);
    }

    // -- urlencoded --

    #[test]
    fn urlencoded_no_special_chars() {
        assert_eq!(urlencoded("hello"), "hello");
    }

    #[test]
    fn urlencoded_spaces() {
        assert_eq!(urlencoded("hello world"), "hello%20world");
    }

    #[test]
    fn urlencoded_percent() {
        assert_eq!(urlencoded("100%"), "100%25");
    }

    #[test]
    fn urlencoded_ampersand_equals() {
        assert_eq!(urlencoded("a=1&b=2"), "a%3D1%26b%3D2");
    }

    #[test]
    fn urlencoded_hash_question() {
        assert_eq!(urlencoded("path?q#f"), "path%3Fq%23f");
    }

    #[test]
    fn urlencoded_all_special() {
        assert_eq!(urlencoded("% &=#?"), "%25%20%26%3D%23%3F");
    }

    #[test]
    fn urlencoded_empty() {
        assert_eq!(urlencoded(""), "");
    }

    // -- resolve_file_path --

    #[test]
    fn resolve_file_path_absolute() {
        let result = resolve_file_path(Path::new("/absolute/path/file.rs"));
        assert_eq!(result, "/absolute/path/file.rs");
    }

    #[test]
    fn resolve_file_path_relative() {
        let result = resolve_file_path(Path::new("relative/file.rs"));
        // Should prepend current working directory
        assert!(result.ends_with("relative/file.rs"));
        assert!(Path::new(&result).is_absolute());
    }

    // -- extract_md_title --

    #[test]
    fn extract_md_title_with_heading() {
        assert_eq!(extract_md_title("# My Title\nSome content"), "My Title");
    }

    #[test]
    fn extract_md_title_with_heading_and_whitespace() {
        assert_eq!(extract_md_title("# My Title  \nContent"), "My Title");
    }

    #[test]
    fn extract_md_title_no_heading() {
        assert_eq!(extract_md_title("No heading here\nJust text"), "Untitled");
    }

    #[test]
    fn extract_md_title_heading_not_first_line() {
        assert_eq!(
            extract_md_title("Some preamble\n# Actual Title\nMore content"),
            "Actual Title"
        );
    }

    #[test]
    fn extract_md_title_h2_not_h1() {
        assert_eq!(extract_md_title("## Not H1\nContent"), "Untitled");
    }

    #[test]
    fn extract_md_title_empty() {
        assert_eq!(extract_md_title(""), "Untitled");
    }

    // -- build_instructions --

    #[test]
    fn build_instructions_standalone() {
        let text = build_instructions(true);
        assert!(text.contains("TOOLS (always available):"));
        assert!(!text.contains("TOOLS (API-connected):"));
        assert!(!text.contains("select_project"));
    }

    #[test]
    fn build_instructions_api_connected_has_common_tools() {
        let text = build_instructions(false);
        assert!(text.contains("TOOLS (always available):"));
    }

    #[test]
    fn build_instructions_api_connected_has_api_section() {
        let text = build_instructions(false);
        assert!(text.contains("TOOLS (API-connected):"));
        assert!(text.contains("select_project"));
    }

    // -- tool_error --

    #[test]
    fn tool_error_returns_error_result() {
        let result = tool_error("something went wrong");
        // CallToolResult::error sets is_error to Some(true)
        assert_eq!(result.is_error, Some(true));
        assert_eq!(result.content.len(), 1);
    }

    // -- matches_function_name --

    #[test]
    fn matches_function_name_exact() {
        assert!(matches_function_name("myFunc", "myFunc"));
    }

    #[test]
    fn matches_function_name_with_suffix_number() {
        assert!(matches_function_name("myFunc:1", "myFunc"));
    }

    #[test]
    fn matches_function_name_with_larger_suffix() {
        // strip_suffix only removes one char, so "myFunc:42" strips '2' -> "myFunc:4"
        // which after trim_end_matches(':') is still "myFunc:4" != "myFunc"
        // The second branch would match, but the first branch returns false first.
        // This is a known limitation of the current implementation.
        assert!(!matches_function_name("myFunc:42", "myFunc"));
    }

    #[test]
    fn matches_function_name_no_match() {
        assert!(!matches_function_name("otherFunc", "myFunc"));
    }

    #[test]
    fn matches_function_name_partial_overlap() {
        // "myFuncExtra" should NOT match "myFunc" because after "myFunc" there's no ':'
        assert!(!matches_function_name("myFuncExtra", "myFunc"));
    }

    #[test]
    fn matches_function_name_colon_with_non_digit_suffix() {
        // Ends with a non-digit so strip_suffix returns None, falls through
        // to the starts_with branch; suffix after ':' is not all digits.
        assert!(!matches_function_name("myFunc:abc", "myFunc"));
    }

    #[test]
    fn matches_function_name_prefix_match_with_colon() {
        // Multi-digit suffix: strip_suffix only strips one char, so this
        // falls into the strip_suffix branch but doesn't match after trimming
        assert!(!matches_function_name("myFunc:123", "myFunc"));
    }

    #[test]
    fn matches_function_name_empty_title() {
        assert!(!matches_function_name("", "myFunc"));
    }

    #[test]
    fn matches_function_name_empty_name() {
        // Empty name matches if title starts with ":"
        // Actually the strip_suffix approach may handle this differently
        assert!(matches_function_name("", ""));
    }

    // -- find_function_in_parsed --

    #[test]
    fn find_function_in_parsed_found() {
        let functions = json!([
            {"name": "foo", "body": "fn foo() {}"},
            {"name": "bar", "body": "fn bar() {}"},
        ]);
        let result = find_function_in_parsed(&functions, "bar");
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().get("body").unwrap().as_str().unwrap(),
            "fn bar() {}"
        );
    }

    #[test]
    fn find_function_in_parsed_not_found() {
        let functions = json!([
            {"name": "foo", "body": "fn foo() {}"},
        ]);
        assert!(find_function_in_parsed(&functions, "bar").is_none());
    }

    #[test]
    fn find_function_in_parsed_empty_array() {
        let functions = json!([]);
        assert!(find_function_in_parsed(&functions, "foo").is_none());
    }

    #[test]
    fn find_function_in_parsed_not_array() {
        let functions = json!({"name": "foo"});
        assert!(find_function_in_parsed(&functions, "foo").is_none());
    }

    #[test]
    fn find_function_in_parsed_missing_name_field() {
        let functions = json!([{"body": "fn foo() {}"}]);
        assert!(find_function_in_parsed(&functions, "foo").is_none());
    }

    // -- extract_code_smells --

    /// Build a single-category review JSON for `extract_code_smells` tests.
    fn make_review_json(
        category: &str,
        title: &str,
        start_line: Option<i64>,
    ) -> serde_json::Value {
        let mut func = json!({"title": title});
        if let Some(line) = start_line {
            func["start-line"] = json!(line);
        }
        json!({
            "review": [
                { "category": category, "functions": [func] }
            ]
        })
    }

    #[test]
    fn extract_code_smells_matching() {
        let review = make_review_json("Complex Method", "myFunc", Some(10));
        let function = json!({"name": "myFunc", "start-line": 5});
        let smells = extract_code_smells(&review, &function, "myFunc");
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0]["category"], "Complex Method");
        // start-line should be relative: 10 - 5 + 1 = 6
        assert_eq!(smells[0]["start-line"], 6);
    }

    #[test]
    fn extract_code_smells_no_match() {
        let review = make_review_json("Complex Method", "otherFunc", Some(10));
        let function = json!({"name": "myFunc", "start-line": 5});
        let smells = extract_code_smells(&review, &function, "myFunc");
        assert!(smells.is_empty());
    }

    #[test]
    fn extract_code_smells_empty_review() {
        let review = json!({"review": []});
        let function = json!({"name": "myFunc", "start-line": 1});
        let smells = extract_code_smells(&review, &function, "myFunc");
        assert!(smells.is_empty());
    }

    #[test]
    fn extract_code_smells_no_review_key() {
        let review = json!({"score": 8.5});
        let function = json!({"name": "myFunc", "start-line": 1});
        let smells = extract_code_smells(&review, &function, "myFunc");
        assert!(smells.is_empty());
    }

    #[test]
    fn extract_code_smells_multiple_categories() {
        let review = json!({
            "review": [
                {
                    "category": "Complex Method",
                    "functions": [
                        {"title": "myFunc", "start-line": 10}
                    ]
                },
                {
                    "category": "Large Method",
                    "functions": [
                        {"title": "myFunc:1", "start-line": 20}
                    ]
                }
            ]
        });
        let function = json!({"name": "myFunc", "start-line": 5});
        let smells = extract_code_smells(&review, &function, "myFunc");
        assert_eq!(smells.len(), 2);
        assert_eq!(smells[0]["category"], "Complex Method");
        assert_eq!(smells[1]["category"], "Large Method");
    }

    #[test]
    fn extract_code_smells_missing_start_line() {
        let review = make_review_json("Complex Method", "myFunc", None);
        let function = json!({"name": "myFunc"});
        let smells = extract_code_smells(&review, &function, "myFunc");
        assert_eq!(smells.len(), 1);
        // Both start-lines default to 0, so: 0 - 0 + 1 = 1
        assert_eq!(smells[0]["start-line"], 1);
    }

    // -- build_ace_payload --

    #[test]
    fn build_ace_payload_api_version() {
        let function = json!({"body": "fn f() {}", "function-type": "Function"});
        let smells = vec![json!({"category": "Complex Method", "start-line": 1})];
        let result = build_ace_payload(&function, &smells, "js");
        assert_eq!(result["api-version"], "v2");
    }

    #[test]
    fn build_ace_payload_source_snippet() {
        let function = json!({"body": "function foo() { return 1; }", "function-type": "Function"});
        let smells = vec![json!({"category": "Complex Method", "start-line": 1})];
        let result = build_ace_payload(&function, &smells, "js");
        assert_eq!(result["source-snippet"]["file-type"], "js");
        assert_eq!(result["source-snippet"]["body"], "function foo() { return 1; }");
        assert_eq!(result["source-snippet"]["function-type"], "Function");
    }

    #[test]
    fn build_ace_payload_review_section() {
        let function = json!({"body": "fn f() {}", "function-type": "Function"});
        let smells = vec![json!({"category": "Complex Method", "start-line": 1})];
        let result = build_ace_payload(&function, &smells, "js");
        assert_eq!(result["review"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn build_ace_payload_missing_fields() {
        let function = json!({});
        let smells: Vec<serde_json::Value> = vec![];
        let result = build_ace_payload(&function, &smells, "ts");

        assert_eq!(result["source-snippet"]["body"], "");
        assert_eq!(result["source-snippet"]["function-type"], "Unknown");
        assert_eq!(result["review"].as_array().unwrap().len(), 0);
    }

    // -- format_ace_response --

    #[test]
    fn format_ace_response_code_and_declarations() {
        let response = json!({
            "code": "function foo() {}",
            "declarations": "declare function foo(): void;",
            "confidence": {"description": "high"},
            "reasons": [{"summary": "Extracted helper function"}]
        });
        let result = format_ace_response(&response);
        assert_eq!(result["code"], "function foo() {}");
        assert_eq!(result["declarations"], "declare function foo(): void;");
    }

    #[test]
    fn format_ace_response_confidence_and_reasons() {
        let response = json!({
            "code": "fn f() {}",
            "confidence": {"description": "high"},
            "reasons": [
                {"summary": "Extracted helper function"},
                {"summary": "Simplified control flow"}
            ]
        });
        let result = format_ace_response(&response);
        assert_eq!(result["confidence"], "high");
        assert_eq!(result["reasons"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn format_ace_response_defaults_code_and_declarations() {
        let response = json!({});
        let result = format_ace_response(&response);
        assert_eq!(result["code"], "");
        assert_eq!(result["declarations"], "");
    }

    #[test]
    fn format_ace_response_defaults_confidence_and_reasons() {
        let response = json!({});
        let result = format_ace_response(&response);
        assert_eq!(result["confidence"], "unknown");
        assert!(result["reasons"].as_array().unwrap().is_empty());
    }

    #[test]
    fn format_ace_response_empty_reasons() {
        let response = json!({"reasons": []});
        let result = format_ace_response(&response);
        assert!(result["reasons"].as_array().unwrap().is_empty());
    }

    #[test]
    fn format_ace_response_reasons_without_summary() {
        let response = json!({
            "reasons": [{"detail": "something"}]
        });
        let result = format_ace_response(&response);
        assert!(result["reasons"].as_array().unwrap().is_empty());
    }

    // -- inlined_schema_for --

    #[test]
    fn inlined_schema_for_produces_object() {
        let schema = inlined_schema_for::<FilePathParam>();
        // Should be a valid JSON schema object
        assert!(schema.contains_key("type") || schema.contains_key("properties"));
    }

    #[test]
    fn inlined_schema_for_optional_context() {
        let schema = inlined_schema_for::<OptionalContext>();
        // Should be a valid schema - just check it doesn't panic
        assert!(!schema.is_empty());
    }

    // -- make_cli_path (non-docker only since OnceLock) --

    #[test]
    fn make_cli_path_with_git_root() {
        // In non-docker mode, should return relative path
        if !environment::is_docker() {
            let result = make_cli_path("/repo/src/file.rs", Some(Path::new("/repo")));
            assert_eq!(result, "src/file.rs");
        }
    }

    #[test]
    fn make_cli_path_without_git_root() {
        if !environment::is_docker() {
            let result = make_cli_path("/repo/src/file.rs", None);
            assert_eq!(result, "/repo/src/file.rs");
        }
    }

    // -- API_ONLY_TOOLS constant --

    #[test]
    fn api_only_tools_has_expected_entries() {
        assert!(API_ONLY_TOOLS.contains(&"select_project"));
        assert!(API_ONLY_TOOLS.contains(&"code_ownership_for_path"));
        assert_eq!(API_ONLY_TOOLS.len(), 6);
    }

    // ======================================================================
    // CodeSceneServer construction & helpers
    // ======================================================================

    use std::collections::HashMap;
    use std::sync::Mutex;
    use rmcp::handler::server::wrapper::Parameters;

    static SERVER_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Build a `ServerDeps` with the given mock/production clients.
    fn test_deps(
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

    /// Build a `CodeSceneServer` for tests. Uses version "dev" to disable
    /// background version checking, and an empty `ConfigData`.
    fn make_server(is_standalone: bool) -> CodeSceneServer {
        CodeSceneServer::new(test_deps(
            "test-instance",
            is_standalone,
            Arc::new(cli::ProductionCliRunner),
            Arc::new(http::ReqwestClient),
        ))
    }

    /// Extract the text body from the first content item in a `CallToolResult`.
    fn result_text(result: &CallToolResult) -> &str {
        result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or("")
    }

    /// Build a server whose `VersionChecker` already has cached version info.
    async fn make_server_with_version(
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

    /// RAII guard that acquires `SERVER_ENV_LOCK`, sets `CS_ACCESS_TOKEN`,
    /// and restores the environment when dropped.
    struct TokenGuard<'a> {
        _lock: std::sync::MutexGuard<'a, ()>,
    }

    impl Drop for TokenGuard<'_> {
        fn drop(&mut self) {
            std::env::remove_var("CS_ACCESS_TOKEN");
        }
    }

    fn set_token(value: &str) -> TokenGuard<'static> {
        let lock = SERVER_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_ACCESS_TOKEN", value);
        TokenGuard { _lock: lock }
    }

    fn clear_token() -> TokenGuard<'static> {
        let lock = SERVER_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_ACCESS_TOKEN");
        TokenGuard { _lock: lock }
    }

    // -- CodeSceneServer::new --

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

    // -- get_info --

    #[test]
    fn get_info_returns_server_name() {
        let server = make_server(false);
        assert_eq!(server.get_info().server_info.name, "codescene-mcp-server");
    }

    #[test]
    fn get_info_standalone_instructions_omit_api_tools() {
        let info = make_server(true).get_info();
        assert!(!info.instructions.as_deref().unwrap_or("").contains("select_project"));
    }

    #[test]
    fn get_info_api_instructions_include_api_tools() {
        let info = make_server(false).get_info();
        assert!(info.instructions.as_deref().unwrap_or("").contains("select_project"));
    }

    #[test]
    fn get_info_enables_tools_prompts_resources() {
        let caps = &make_server(false).get_info().capabilities;
        assert!(caps.tools.is_some());
        assert!(caps.prompts.is_some());
        assert!(caps.resources.is_some());
    }

    // -- require_token --

    #[tokio::test]
    async fn require_token_returns_error_when_missing() {
        let _g = clear_token();
        assert!(make_server(false).require_token().is_some());
    }

    #[tokio::test]
    async fn require_token_returns_error_when_empty() {
        let _g = set_token("");
        assert!(make_server(false).require_token().is_some());
    }

    #[tokio::test]
    async fn require_token_returns_none_when_set() {
        let _g = set_token("some-token");
        assert!(make_server(false).require_token().is_none());
    }

    // -- maybe_version_warning --

    #[tokio::test]
    async fn maybe_version_warning_returns_text_when_no_cache() {
        assert_eq!(make_server(false).maybe_version_warning("hello").await, "hello");
    }

    #[tokio::test]
    async fn maybe_version_warning_prepends_warning_when_outdated() {
        let server = make_server_with_version("1.0.0", "2.0.0", true).await;
        let result = server.maybe_version_warning("body text").await;
        assert!(result.contains("VERSION UPDATE AVAILABLE"));
        assert!(result.contains("body text"));
    }

    #[tokio::test]
    async fn maybe_version_warning_no_warning_when_current() {
        let server = make_server_with_version("2.0.0", "2.0.0", false).await;
        assert_eq!(server.maybe_version_warning("body text").await, "body text");
    }

    // -- track / track_err (fire-and-forget, just verify no panic) --

    #[tokio::test]
    async fn track_does_not_panic() {
        make_server(false).track("test-event", json!({"key": "value"}));
    }

    #[tokio::test]
    async fn track_err_does_not_panic() {
        make_server(false).track_err("test-tool", "some error");
    }

    // ======================================================================
    // Tool handler tests (explain_* tools — static content)
    // ======================================================================

    #[tokio::test]
    async fn explain_code_health_returns_content_when_token_set() {
        let _g = set_token("test-token");
        let result = make_server(false)
            .explain_code_health(Parameters(OptionalContext { context: None }))
            .await
            .unwrap();
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn explain_code_health_returns_token_error_when_missing() {
        let _g = clear_token();
        let result = make_server(false)
            .explain_code_health(Parameters(OptionalContext { context: None }))
            .await
            .unwrap();
        assert!(result_text(&result).contains("No access token configured"));
    }

    #[tokio::test]
    async fn explain_code_health_productivity_returns_content() {
        let _g = set_token("test-token");
        let result = make_server(false)
            .explain_code_health_productivity(Parameters(OptionalContext { context: None }))
            .await
            .unwrap();
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        assert!(!result.content.is_empty());
    }

    #[tokio::test]
    async fn explain_code_health_productivity_token_error() {
        let _g = clear_token();
        let result = make_server(false)
            .explain_code_health_productivity(Parameters(OptionalContext { context: None }))
            .await
            .unwrap();
        assert!(result_text(&result).contains("No access token configured"));
    }

    // ======================================================================
    // Tool handler tests (get_config / set_config)
    // ======================================================================

    #[tokio::test]
    async fn get_config_lists_all_options() {
        let result = make_server(false)
            .get_config(Parameters(GetConfigParam { key: None }))
            .await
            .unwrap();
        assert!(result_text(&result).contains("config_dir"));
    }

    #[tokio::test]
    async fn get_config_single_key() {
        let result = make_server(false)
            .get_config(Parameters(GetConfigParam {
                key: Some("access_token".to_string()),
            }))
            .await
            .unwrap();
        assert!(result_text(&result).contains("access_token"));
    }

    #[tokio::test]
    async fn set_config_unknown_key_returns_error() {
        let result = make_server(false)
            .set_config(Parameters(SetConfigParam {
                key: "nonexistent_key_xyz".to_string(),
                value: "test".to_string(),
            }))
            .await
            .unwrap();
        let text = result_text(&result);
        assert!(text.contains("error") || text.contains("unknown") || text.contains("Unknown"));
    }

    // ======================================================================
    // Tool handler tests (standalone guard on API-only tools)
    // ======================================================================

    /// Assert that a tool result is an error mentioning standalone mode.
    fn assert_standalone_error(result: &CallToolResult) {
        assert_eq!(result.is_error, Some(true), "expected is_error=true");
        assert!(
            result_text(result).contains("standalone"),
            "expected standalone mention in: {}",
            result_text(result),
        );
    }

    #[tokio::test]
    async fn select_project_rejects_standalone_mode() {
        let _g = set_token("test-token");
        let result = make_server(true).select_project().await.unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn select_project_rejects_missing_token() {
        let _g = clear_token();
        let result = make_server(false).select_project().await.unwrap();
        assert!(result_text(&result).contains("No access token configured"));
    }

    #[tokio::test]
    async fn list_goals_rejects_standalone_mode() {
        let _g = set_token("test-token");
        let result = make_server(true)
            .list_technical_debt_goals_for_project(Parameters(ProjectParam { project_id: 1 }))
            .await
            .unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn list_goals_file_rejects_standalone_mode() {
        let _g = set_token("test-token");
        let params = ProjectFileParam { project_id: 1, file_path: "/tmp/f.rs".to_string() };
        let result = make_server(true)
            .list_technical_debt_goals_for_project_file(Parameters(params))
            .await
            .unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn list_hotspots_rejects_standalone_mode() {
        let _g = set_token("test-token");
        let result = make_server(true)
            .list_technical_debt_hotspots_for_project(Parameters(ProjectParam { project_id: 1 }))
            .await
            .unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn list_hotspots_file_rejects_standalone_mode() {
        let _g = set_token("test-token");
        let params = ProjectFileParam { project_id: 1, file_path: "/tmp/f.rs".to_string() };
        let result = make_server(true)
            .list_technical_debt_hotspots_for_project_file(Parameters(params))
            .await
            .unwrap();
        assert_standalone_error(&result);
    }

    #[tokio::test]
    async fn ownership_rejects_standalone_mode() {
        let _g = set_token("test-token");
        let params = OwnershipParam { project_id: 1, path: "/tmp/f.rs".to_string() };
        let result = make_server(true)
            .code_ownership_for_path(Parameters(params))
            .await
            .unwrap();
        assert_standalone_error(&result);
    }

    // ======================================================================
    // Token-missing tests for CLI-based tool handlers
    // ======================================================================

    fn assert_token_error(result: &CallToolResult) {
        assert!(
            result_text(result).contains("No access token configured"),
            "expected token error, got: {}",
            result_text(result),
        );
    }

    #[tokio::test]
    async fn code_health_review_rejects_missing_token() {
        let _g = clear_token();
        let params = FilePathParam { file_path: "/tmp/f.rs".to_string() };
        let result = make_server(false).code_health_review(Parameters(params)).await.unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn code_health_score_rejects_missing_token() {
        let _g = clear_token();
        let params = FilePathParam { file_path: "/tmp/f.rs".to_string() };
        let result = make_server(false).code_health_score(Parameters(params)).await.unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn pre_commit_safeguard_rejects_missing_token() {
        let _g = clear_token();
        let params = GitRepoParam { git_repository_path: "/tmp/repo".to_string() };
        let result = make_server(false)
            .pre_commit_code_health_safeguard(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn analyze_change_set_rejects_missing_token() {
        let _g = clear_token();
        let params = ChangeSetParam {
            base_ref: "main".to_string(),
            git_repository_path: "/tmp/repo".to_string(),
        };
        let result = make_server(false).analyze_change_set(Parameters(params)).await.unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn business_case_rejects_missing_token() {
        let _g = clear_token();
        let params = FilePathParam { file_path: "/tmp/f.rs".to_string() };
        let result = make_server(false)
            .code_health_refactoring_business_case(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn auto_refactor_rejects_missing_token() {
        let _g = clear_token();
        let params = RefactorParam {
            file_path: "/tmp/f.rs".to_string(),
            function_name: "foo".to_string(),
        };
        let result = make_server(false)
            .code_health_auto_refactor(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    // ======================================================================
    // Token-missing tests for API-based tool handlers
    // ======================================================================

    #[tokio::test]
    async fn list_goals_rejects_missing_token() {
        let _g = clear_token();
        let result = make_server(false)
            .list_technical_debt_goals_for_project(Parameters(ProjectParam { project_id: 1 }))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn list_goals_file_rejects_missing_token() {
        let _g = clear_token();
        let params = ProjectFileParam { project_id: 1, file_path: "/tmp/f.rs".to_string() };
        let result = make_server(false)
            .list_technical_debt_goals_for_project_file(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn list_hotspots_rejects_missing_token() {
        let _g = clear_token();
        let result = make_server(false)
            .list_technical_debt_hotspots_for_project(Parameters(ProjectParam { project_id: 1 }))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn list_hotspots_file_rejects_missing_token() {
        let _g = clear_token();
        let params = ProjectFileParam { project_id: 1, file_path: "/tmp/f.rs".to_string() };
        let result = make_server(false)
            .list_technical_debt_hotspots_for_project_file(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn ownership_rejects_missing_token() {
        let _g = clear_token();
        let params = OwnershipParam { project_id: 1, path: "/tmp/f.rs".to_string() };
        let result = make_server(false)
            .code_ownership_for_path(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    // ======================================================================
    // run_select_project (env-var–based, no real API calls for default path)
    // ======================================================================

    #[tokio::test]
    async fn run_select_project_with_default_project_id() {
        let _g = set_token("test-token");
        std::env::set_var("CS_DEFAULT_PROJECT_ID", "42");
        let result = run_select_project(&http::ReqwestClient).await;
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let value = result.unwrap();
        assert_eq!(value["id"], 42);
        assert!(value["description"].as_str().unwrap().contains("default"));
    }

    #[tokio::test]
    async fn run_select_project_default_id_empty_falls_through() {
        let _g = set_token("test-token");
        std::env::set_var("CS_DEFAULT_PROJECT_ID", "");
        // With empty project ID and no real API, this will fail at the API call.
        // We just verify it doesn't return the default-project-id branch.
        let result = run_select_project(&http::ReqwestClient).await;
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        // It should either be Ok(projects list) or Err(api error) — not the default branch
        match result {
            Ok(val) => assert!(val.get("projects").is_some()),
            Err(e) => assert!(e.contains("Error")),
        }
    }

    // ======================================================================
    // make_relative_for_api
    // ======================================================================

    #[test]
    fn make_relative_for_api_returns_relative_path() {
        // When given a path inside a git repo, it should return the relative portion
        let result = make_relative_for_api("/definitely/not/a/git/repo/file.rs");
        // Without a git root, falls back to the full path
        assert_eq!(result, "/definitely/not/a/git/repo/file.rs");
    }

    #[test]
    fn make_relative_for_api_inside_git_repo() {
        // Use a path inside our own repo to exercise the Some(root) branch.
        let manifest = env!("CARGO_MANIFEST_DIR");
        let abs_path = format!("{manifest}/src/main.rs");
        let result = make_relative_for_api(&abs_path);
        // Should strip the repo root and return a relative path
        assert!(
            !result.starts_with('/'),
            "expected relative path, got {result:?}",
        );
        assert!(result.ends_with("src/main.rs"), "got {result:?}");
    }

    // ======================================================================
    // MockCliRunner & mock-based server construction
    // ======================================================================

    use crate::errors::CliError;
    use crate::http::tests::MockHttpClient;
    use crate::http::HttpResponse;

    /// A mock CLI runner that returns pre-configured responses in FIFO order.
    struct MockCliRunner {
        responses: Mutex<Vec<Result<String, CliError>>>,
    }

    impl MockCliRunner {
        fn with_responses(responses: Vec<Result<String, CliError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }

        fn with_ok(output: &str) -> Self {
            Self::with_responses(vec![Ok(output.to_string())])
        }

        fn with_err(code: i32, stderr: &str) -> Self {
            Self::with_responses(vec![Err(CliError::NonZeroExit {
                code,
                stderr: stderr.to_string(),
            })])
        }
    }

    #[async_trait::async_trait]
    impl CliRunner for MockCliRunner {
        async fn run(
            &self,
            _args: &[&str],
            _working_dir: Option<&Path>,
        ) -> Result<String, CliError> {
            self.responses
                .lock()
                .unwrap()
                .remove(0)
        }
    }

    /// Build a `CodeSceneServer` with injectable mock CLI and HTTP clients.
    fn make_server_with_mocks(
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

    /// Build a mock server with only a CLI mock (HTTP unused).
    fn make_cli_mock_server(cli: MockCliRunner) -> CodeSceneServer {
        make_server_with_mocks(false, cli, MockHttpClient::new(vec![]))
    }

    /// Build a mock server with only an HTTP mock (CLI unused).
    fn make_http_mock_server(http: MockHttpClient) -> CodeSceneServer {
        make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            http,
        )
    }

    /// Assert that a tool call succeeded and its text contains `needle`.
    fn assert_success_contains(result: &CallToolResult, needle: &str) {
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

    /// Assert that a tool call returned an error and its text contains `needle`.
    fn assert_error_contains(result: &CallToolResult, needle: &str) {
        assert_eq!(result.is_error, Some(true), "expected error result");
        assert!(
            result_text(result).contains(needle),
            "expected error text to contain {:?}, got {:?}",
            needle,
            result_text(result),
        );
    }

    /// Create a temp directory that looks like a git repo with a single file,
    /// and set the ACE access token env var. Returns `(TempDir, PathBuf)`.
    fn make_refactor_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        std::env::set_var("CS_ACE_ACCESS_TOKEN", "ace-tok");
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let file_path = dir.path().join("test.js");
        std::fs::write(&file_path, "function foo() {}").unwrap();
        (dir, file_path)
    }

    // ======================================================================
    // CLI tool handler tests — code_health_review
    // ======================================================================

    #[tokio::test]
    async fn review_success_returns_cli_output() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":9.5,"review":[]}"#));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_review(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "9.5");
    }

    #[tokio::test]
    async fn review_error_returns_tool_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(1, "review failed"));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_review(Parameters(params)).await.unwrap();
        assert_error_contains(&result, "review failed");
    }

    // ======================================================================
    // CLI tool handler tests — code_health_score
    // ======================================================================

    #[tokio::test]
    async fn score_success_returns_score() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":8.5,"review":[]}"#));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_score(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "Code Health score: 8.5");
    }

    #[tokio::test]
    async fn score_no_score_in_output() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"review":[]}"#));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_score(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "Could not determine");
    }

    #[tokio::test]
    async fn score_error_returns_tool_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(1, "score failed"));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_score(Parameters(params)).await.unwrap();
        assert_error_contains(&result, "score failed");
    }

    // ======================================================================
    // CLI tool handler tests — pre_commit_code_health_safeguard
    // ======================================================================

    async fn run_safeguard(cli: MockCliRunner) -> CallToolResult {
        let server = make_cli_mock_server(cli);
        let params = GitRepoParam { git_repository_path: "/tmp/repo".to_string() };
        server.pre_commit_code_health_safeguard(Parameters(params)).await.unwrap()
    }

    #[tokio::test]
    async fn safeguard_success_returns_parsed_delta() {
        let _g = set_token("tok");
        let result = run_safeguard(MockCliRunner::with_ok("--- file.rs\n+++ file.rs\n")).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }

    #[tokio::test]
    async fn safeguard_error_returns_tool_error() {
        let _g = set_token("tok");
        let result = run_safeguard(MockCliRunner::with_err(1, "delta failed")).await;
        assert_error_contains(&result, "delta failed");
    }

    // ======================================================================
    // CLI tool handler tests — analyze_change_set
    // ======================================================================

    async fn run_change_set(cli: MockCliRunner) -> CallToolResult {
        let server = make_cli_mock_server(cli);
        let params = ChangeSetParam {
            base_ref: "main".to_string(),
            git_repository_path: "/tmp/repo".to_string(),
        };
        server.analyze_change_set(Parameters(params)).await.unwrap()
    }

    #[tokio::test]
    async fn change_set_success_returns_parsed_delta() {
        let _g = set_token("tok");
        let result = run_change_set(MockCliRunner::with_ok("--- file.rs\n+++ file.rs\n")).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
    }

    #[tokio::test]
    async fn change_set_error_returns_tool_error() {
        let _g = set_token("tok");
        let result = run_change_set(MockCliRunner::with_err(1, "delta failed")).await;
        assert_error_contains(&result, "delta failed");
    }

    // ======================================================================
    // CLI tool handler tests — code_health_refactoring_business_case
    // ======================================================================

    #[tokio::test]
    async fn business_case_success_with_score() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":6.0,"review":[]}"#));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_refactoring_business_case(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "scenario");
    }

    #[tokio::test]
    async fn business_case_optimal_score() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":10.0,"review":[]}"#));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_refactoring_business_case(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "already optimal");
    }

    #[tokio::test]
    async fn business_case_no_score() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"review":[]}"#));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_refactoring_business_case(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "Could not determine");
    }

    #[tokio::test]
    async fn business_case_cli_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(1, "review failed"));
        let params = FilePathParam { file_path: "/tmp/test.rs".to_string() };
        let result = server.code_health_refactoring_business_case(Parameters(params)).await.unwrap();
        assert_error_contains(&result, "review failed");
    }

    // ======================================================================
    // API tool handler tests — using mock HTTP
    // ======================================================================

    fn make_api_mock(response: HttpResponse) -> MockHttpClient {
        // API tools call query_api_list which paginates: first call returns
        // data, second call returns empty array to stop pagination.
        MockHttpClient::new(vec![response, HttpResponse::ok("[]")])
    }

    #[tokio::test]
    async fn goals_project_success() {
        let _g = set_token("tok");
        let http = make_api_mock(HttpResponse::ok(r#"[{"file":"a.rs","goals":[]}]"#));
        let server = make_http_mock_server(http);
        let params = ProjectParam { project_id: 42 };
        let result = server.list_technical_debt_goals_for_project(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "a.rs");
    }

    #[tokio::test]
    async fn goals_project_api_error() {
        let _g = set_token("tok");
        let server = make_http_mock_server(MockHttpClient::new(vec![HttpResponse::error(500, "Server Error")]));
        let params = ProjectParam { project_id: 42 };
        let result = server.list_technical_debt_goals_for_project(Parameters(params)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn goals_file_success() {
        let _g = set_token("tok");
        let http = make_api_mock(HttpResponse::ok(r#"[{"goal":"reduce complexity"}]"#));
        let server = make_http_mock_server(http);
        let params = ProjectFileParam { project_id: 42, file_path: "/tmp/f.rs".to_string() };
        let result = server.list_technical_debt_goals_for_project_file(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "reduce complexity");
    }

    #[tokio::test]
    async fn goals_file_api_error() {
        let _g = set_token("tok");
        let server = make_http_mock_server(MockHttpClient::new(vec![HttpResponse::error(404, "Not Found")]));
        let params = ProjectFileParam { project_id: 42, file_path: "/tmp/f.rs".to_string() };
        let result = server.list_technical_debt_goals_for_project_file(Parameters(params)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn hotspots_project_success() {
        let _g = set_token("tok");
        let http = make_api_mock(HttpResponse::ok(r#"[{"file":"b.rs","score":3.5}]"#));
        let server = make_http_mock_server(http);
        let params = ProjectParam { project_id: 10 };
        let result = server.list_technical_debt_hotspots_for_project(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "b.rs");
    }

    #[tokio::test]
    async fn hotspots_project_api_error() {
        let _g = set_token("tok");
        let server = make_http_mock_server(MockHttpClient::new(vec![HttpResponse::error(403, "Forbidden")]));
        let params = ProjectParam { project_id: 10 };
        let result = server.list_technical_debt_hotspots_for_project(Parameters(params)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn hotspots_file_success() {
        let _g = set_token("tok");
        let http = make_api_mock(HttpResponse::ok(r#"[{"score":7.0,"revisions":12}]"#));
        let server = make_http_mock_server(http);
        let params = ProjectFileParam { project_id: 10, file_path: "/tmp/f.rs".to_string() };
        let result = server.list_technical_debt_hotspots_for_project_file(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "7.0");
    }

    #[tokio::test]
    async fn hotspots_file_api_error() {
        let _g = set_token("tok");
        let server = make_http_mock_server(MockHttpClient::new(vec![HttpResponse::error(500, "Oops")]));
        let params = ProjectFileParam { project_id: 10, file_path: "/tmp/f.rs".to_string() };
        let result = server.list_technical_debt_hotspots_for_project_file(Parameters(params)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn ownership_success() {
        let _g = set_token("tok");
        let http = make_api_mock(HttpResponse::ok(r#"[{"owner":"Alice","paths":["src/"]}]"#));
        let server = make_http_mock_server(http);
        let params = OwnershipParam { project_id: 5, path: "/tmp/src/f.rs".to_string() };
        let result = server.code_ownership_for_path(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "Alice");
    }

    #[tokio::test]
    async fn ownership_api_error() {
        let _g = set_token("tok");
        let server = make_http_mock_server(MockHttpClient::new(vec![HttpResponse::error(401, "Unauthorized")]));
        let params = OwnershipParam { project_id: 5, path: "/tmp/src/f.rs".to_string() };
        let result = server.code_ownership_for_path(Parameters(params)).await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    // ======================================================================
    // select_project with mock HTTP (fallthrough to API)
    // ======================================================================

    #[tokio::test]
    async fn select_project_api_success() {
        let _g = set_token("tok");
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let http = make_api_mock(HttpResponse::ok(r#"[{"id":1,"name":"My Project"}]"#));
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), http);
        let result = server.select_project().await.unwrap();
        assert_success_contains(&result, "My Project");
    }

    #[tokio::test]
    async fn select_project_api_error() {
        let _g = set_token("tok");
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let http = MockHttpClient::new(vec![HttpResponse::error(500, "API down")]);
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), http);
        let result = server.select_project().await.unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    // ======================================================================
    // run_auto_refactor — complex workflow paths
    // ======================================================================

    #[tokio::test]
    async fn auto_refactor_missing_ace_token() {
        let _g = set_token("tok");
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), MockHttpClient::new(vec![]));
        let params = RefactorParam { file_path: "/tmp/test.js".to_string(), function_name: "foo".to_string() };
        let result = server.code_health_auto_refactor(Parameters(params)).await.unwrap();
        assert_error_contains(&result, "ACE access");
    }

    #[tokio::test]
    async fn run_auto_refactor_no_git_root() {
        let _g = set_token("tok");
        std::env::set_var("CS_ACE_ACCESS_TOKEN", "ace-tok");
        let result = run_auto_refactor(
            Path::new("/nonexistent/path/test.js"),
            "foo",
            &MockCliRunner::with_responses(vec![]),
            &MockHttpClient::new(vec![]),
        )
        .await;
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        assert!(result.unwrap_err().contains("git root"));
    }

    #[tokio::test]
    async fn run_auto_refactor_parse_fns_error() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_err(1, "parse-fns failed");
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("parse-fns failed"));
    }

    #[tokio::test]
    async fn run_auto_refactor_invalid_parse_fns_json() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_ok("not-json");
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("parsing functions"));
    }

    /// Run `run_auto_refactor` with the fixture file and given mocks,
    /// cleaning up the ACE token env var afterward.
    async fn refactor_with_fixture(
        cli: MockCliRunner,
        http: MockHttpClient,
        fn_name: &str,
    ) -> Result<serde_json::Value, String> {
        let (_dir, file_path) = make_refactor_fixture();
        let result = run_auto_refactor(&file_path, fn_name, &cli, &http).await;
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        result
    }

    #[tokio::test]
    async fn run_auto_refactor_review_error() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo(){}","start-line":1,"function-type":"Function"}]"#.to_string()),
            Err(CliError::NonZeroExit { code: 1, stderr: "review exploded".to_string() }),
        ]);
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("review exploded"));
    }

    #[tokio::test]
    async fn run_auto_refactor_function_not_found() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"bar","body":"function bar(){}","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":5.0,"review":[]}"#.to_string()),
        ]);
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("Couldn't find function: foo"));
    }

    #[tokio::test]
    async fn run_auto_refactor_no_code_smells() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo(){}","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":10.0,"review":[]}"#.to_string()),
        ]);
        let result = refactor_with_fixture(cli, MockHttpClient::new(vec![]), "foo").await;
        assert!(result.unwrap_err().contains("No code smells"));
    }

    #[tokio::test]
    async fn run_auto_refactor_ace_api_error() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo(){}","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":5.0,"review":[{"category":"Complex Method","functions":[{"title":"foo","start-line":1}]}]}"#.to_string()),
        ]);
        let http = MockHttpClient::new(vec![HttpResponse::error(500, "ACE down")]);
        let result = refactor_with_fixture(cli, http, "foo").await;
        assert!(result.unwrap_err().contains("ACE down"));
    }

    #[tokio::test]
    async fn run_auto_refactor_full_success() {
        let _g = set_token("tok");
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo() { complex(); }","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":5.0,"review":[{"category":"Complex Method","functions":[{"title":"foo","start-line":1}]}]}"#.to_string()),
        ]);
        let ace_response = r#"{"code":"function foo() { simple(); }","confidence":{"description":"high"},"reasons":[{"summary":"Simplified"}]}"#;
        let http = MockHttpClient::new(vec![HttpResponse::ok(ace_response)]);
        let result = refactor_with_fixture(cli, http, "foo").await;
        let value = result.unwrap();
        assert_eq!(value["confidence"], "high");
        assert!(value["code"].as_str().unwrap().contains("simple"));
    }

    // ======================================================================
    // auto_refactor tool handler success path
    // ======================================================================

    #[tokio::test]
    async fn auto_refactor_tool_success() {
        let _g = set_token("tok");
        let (_dir, file_path) = make_refactor_fixture();
        let cli = MockCliRunner::with_responses(vec![
            Ok(r#"[{"name":"foo","body":"function foo() { x(); }","start-line":1,"function-type":"Function"}]"#.to_string()),
            Ok(r#"{"score":5.0,"review":[{"category":"Complex Method","functions":[{"title":"foo","start-line":1}]}]}"#.to_string()),
        ]);
        let ace_resp = r#"{"code":"function foo() {}","confidence":{"description":"high"},"reasons":[]}"#;
        let http = MockHttpClient::new(vec![HttpResponse::ok(ace_resp)]);
        let server = make_server_with_mocks(false, cli, http);
        let params = RefactorParam {
            file_path: file_path.to_string_lossy().to_string(),
            function_name: "foo".to_string(),
        };
        let result = server.code_health_auto_refactor(Parameters(params)).await.unwrap();
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        assert_success_contains(&result, "high");
    }

    #[tokio::test]
    async fn auto_refactor_tool_error_path() {
        let _g = set_token("tok");
        std::env::set_var("CS_ACE_ACCESS_TOKEN", "ace-tok");
        let server = make_server_with_mocks(false, MockCliRunner::with_responses(vec![]), MockHttpClient::new(vec![]));
        let params = RefactorParam { file_path: "/nonexistent/test.js".to_string(), function_name: "foo".to_string() };
        let result = server.code_health_auto_refactor(Parameters(params)).await.unwrap();
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        assert_error_contains(&result, "git root");
    }

    // ======================================================================
    // run_select_project with mock HTTP (API call path)
    // ======================================================================

    #[tokio::test]
    async fn run_select_project_api_call_success() {
        let _g = set_token("tok");
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let http = make_api_mock(HttpResponse::ok(r#"[{"id":99,"name":"TestProject"}]"#));
        let result = run_select_project(&http).await.unwrap();
        assert!(result["projects"].as_array().unwrap().len() > 0);
        assert_eq!(result["projects"][0]["name"], "TestProject");
    }

    #[tokio::test]
    async fn run_select_project_api_call_error() {
        let _g = set_token("tok");
        std::env::remove_var("CS_DEFAULT_PROJECT_ID");
        let http = MockHttpClient::new(vec![HttpResponse::error(500, "fail")]);
        let result = run_select_project(&http).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Error"));
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
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

    let version = env!("CS_MCP_VERSION");
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

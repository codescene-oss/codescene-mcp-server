mod api_client;
mod auth;
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
mod server_handler;
mod skills;
mod startup;
#[cfg(test)]
mod test_utils;
mod tools;
mod tracking;
mod version_checker;

#[cfg(test)]
pub(crate) use test_utils as tests;

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content};
use rmcp::schemars::{self, JsonSchema};
use rmcp::service::ServerInitializeError;
use rmcp::{tool, tool_router, ErrorData, ServiceExt};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::auth::{AuthCredential, AuthManager};
use crate::cli::CliRunner;
use crate::config::ConfigData;
use crate::http::HttpClient;
use crate::tools::validation::{ValidationError, Validator};
use crate::tools::{
    ChangeSetParam, DownloadSkillParam, FilePathParam, GetConfigParam, GitRepoParam, LoginParam,
    OptionalContext, OwnershipParam, ProjectFileParam, ProjectParam,
    RulesConfigListThresholdsParam, RulesConfigSetRuleParam, RulesConfigSetThresholdParam,
    RulesConfigValidateParam, SetConfigParam, SkillNameParam, SyncSkillsParam,
};
use crate::version_checker::VersionChecker;

const TOKEN_MISSING_MSG: &str = "\
No access token configured.\n\n\
Call the `login` tool now to sign in with OAuth. Do not ask the user for a token first.\n\n\
Use a Personal Access Token only when OAuth is not suitable, such as CI/CD or other headless environments.\n\
If a token must be configured manually:\n\
1. Use the `set_config` tool: set_config(key=\"access_token\", value=\"your-token\")\n\
2. Set the CS_ACCESS_TOKEN environment variable in your MCP client configuration\n\n\
To get a Personal Access Token, see:\n\
https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/getting-a-personal-access-token.md";

const _VERSION_NOTICE_SUFFIX: &str = "\n\
Note: If the result contains version update information (indicated by\n\
\"VERSION UPDATE AVAILABLE\"), please inform the user about this update\n\
notice and recommend they update their CodeScene MCP Server.";

pub(crate) const API_ONLY_TOOLS: &[&str] = &[
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
];

/// Tools that cannot be disabled via `enabled_tools` config.
pub(crate) const ALWAYS_ENABLED_TOOLS: &[&str] = &["get_config", "set_config", "login"];

#[derive(Debug)]
pub(crate) enum CliAction {
    RunServer,
    PrintHelp,
    PrintVersion(String),
    PrintCliVersion,
}

pub(crate) fn display_version(raw_version: &str) -> &str {
    raw_version.strip_prefix("MCP-").unwrap_or(raw_version)
}

pub(crate) fn help_text() -> &'static str {
    "CodeScene MCP Server\n\nUsage: cs-mcp [OPTIONS]\n\nOptions:\n  -h, --help       Show this help message and exit\n  -v, --version    Show version and exit\n  --cli-version    Show embedded CLI version and exit"
}

async fn ensure_oauth_client_configured() {
    if crate::config::try_read_env("CS_OAUTH_CLIENT").is_some() {
        return;
    }

    if let Err(e) = crate::config::write_env("oauth_client", "mcp").await {
        tracing::warn!(error = %e, "failed to persist default OAuth client");
    }
}

pub(crate) fn parse_cli_args(args: &[String], raw_version: &str) -> Result<CliAction, String> {
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

pub(crate) async fn fetch_cli_version(cli_runner: &dyn cli::CliRunner) -> anyhow::Result<String> {
    Ok(cli_runner.run(&["version"], None).await?)
}

pub(crate) fn inlined_schema_for<T: JsonSchema + 'static>(
) -> Arc<serde_json::Map<String, serde_json::Value>> {
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

pub(crate) struct ServerDeps {
    pub(crate) config_data: ConfigData,
    pub(crate) instance_id: String,
    pub(crate) is_standalone: bool,
    pub(crate) version_checker: VersionChecker,
    pub(crate) auth_manager: AuthManager,
    pub(crate) cli_runner: Arc<dyn CliRunner>,
    pub(crate) http_client: Arc<dyn HttpClient>,
    pub(crate) validator: Arc<dyn Validator>,
}

#[derive(Clone)]
pub(crate) struct CodeSceneServer {
    pub(crate) tool_router: ToolRouter<Self>,
    pub(crate) version_checker: VersionChecker,
    pub(crate) auth_manager: AuthManager,
    pub(crate) config_data: Arc<ConfigData>,
    pub(crate) instance_id: String,
    pub(crate) is_standalone: bool,
    pub(crate) cli_runner: Arc<dyn CliRunner>,
    pub(crate) http_client: Arc<dyn HttpClient>,
    pub(crate) validator: Arc<dyn Validator>,
}

impl CodeSceneServer {
    pub(crate) async fn require_token(&self) -> Option<CallToolResult> {
        match self
            .auth_manager
            .resolve_credential(&*self.cli_runner)
            .await
        {
            Ok(Some(credential)) => {
                tracing::info!(
                    source = credential_source(&credential),
                    has_web_root = credential.web_root().is_some(),
                    "token requirement satisfied"
                );
                None
            }
            Ok(None) => {
                tracing::warn!(
                    has_configured_token = crate::auth::configured_credential().is_some(),
                    has_oauth_token = self.auth_manager.try_cached_access_token().is_some(),
                    oauth_expires_at = crate::config::try_read_env("CS_OAUTH_EXPIRES_AT"),
                    signed_out_sentinel = crate::config::try_read_env("CS_OAUTH_EXPIRES_AT")
                        .as_deref()
                        == Some("0"),
                    oauth_client = crate::config::try_read_env("CS_OAUTH_CLIENT"),
                    "token requirement failed: no usable credential resolved"
                );
                Some(CallToolResult::success(vec![Content::text(
                    TOKEN_MISSING_MSG,
                )]))
            }
            Err(e) => {
                tracing::warn!(error = %e, "auth token check failed");
                Some(CallToolResult::success(vec![Content::text(
                    TOKEN_MISSING_MSG,
                )]))
            }
        }
    }

    pub(crate) async fn resolve_auth_credential(&self) -> Result<AuthCredential, CallToolResult> {
        match self
            .auth_manager
            .resolve_credential(&*self.cli_runner)
            .await
        {
            Ok(Some(credential)) => {
                tracing::info!(
                    source = credential_source(&credential),
                    has_web_root = credential.web_root().is_some(),
                    "resolved auth credential for API tool"
                );
                Ok(credential)
            }
            Ok(None) => {
                tracing::warn!(
                    has_configured_token = crate::auth::configured_credential().is_some(),
                    has_oauth_token = self.auth_manager.try_cached_access_token().is_some(),
                    oauth_expires_at = crate::config::try_read_env("CS_OAUTH_EXPIRES_AT"),
                    signed_out_sentinel = crate::config::try_read_env("CS_OAUTH_EXPIRES_AT")
                        .as_deref()
                        == Some("0"),
                    oauth_client = crate::config::try_read_env("CS_OAUTH_CLIENT"),
                    "failed to resolve auth credential for API tool"
                );
                Err(CallToolResult::success(vec![Content::text(
                    TOKEN_MISSING_MSG,
                )]))
            }
            Err(e) => {
                tracing::warn!(error = %e, "auth token check failed");
                Err(CallToolResult::success(vec![Content::text(
                    TOKEN_MISSING_MSG,
                )]))
            }
        }
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
        tracking::track_event(event, props, &self.instance_id, &self.tracking_auth());
    }

    pub(crate) fn track_err(&self, tool: &str, err: &errors::CliError) {
        tracing::warn!(tool, error = %err, "tool error");
        self.track_error_event(err.kind(), tool, None);
    }

    pub(crate) fn track_api_err(&self, tool: &str, err: &errors::ApiError) {
        tracing::warn!(tool, error = %err, "API error");
        self.track_error_event(err.kind(), tool, None);
    }

    pub(crate) fn track_validation_err(&self, tool: &str, err: &ValidationError) {
        tracing::warn!(tool, error = %err, "tool error");
        self.track_error_event(err.kind, tool, err.detail.as_deref());
    }

    pub(crate) fn track_err_msg(&self, tool: &str, error_kind: &str, err: &str) {
        tracing::warn!(tool, error = err, "tool error");
        self.track_error_event(error_kind, tool, None);
    }

    fn track_error_event(&self, error_kind: &str, tool: &str, detail: Option<&str>) {
        let auth = self.tracking_auth();
        tracking::track_error(&tracking::ErrorEvent {
            error_kind,
            tool_name: tool,
            instance_id: &self.instance_id,
            detail,
            auth: &auth,
        });
    }

    /// Resolve the best available token and API root for tracking.
    /// Prefers configured PAT, falls back to cached OAuth token.
    fn tracking_auth(&self) -> tracking::TrackingAuth {
        if let Some(cred) = auth::configured_credential() {
            return tracking::TrackingAuth {
                access_token: cred.access_token().to_string(),
                api_root: cred.api_root().ok(),
            };
        }
        tracking::TrackingAuth {
            access_token: self
                .auth_manager
                .try_cached_access_token()
                .unwrap_or_default(),
            api_root: self.auth_manager.try_cached_api_root(),
        }
    }
}

fn credential_source(credential: &AuthCredential) -> &'static str {
    match credential {
        AuthCredential::Configured { .. } => "configured",
        AuthCredential::OAuth { .. } => "oauth",
    }
}

fn remove_standalone_tools(router: &mut ToolRouter<CodeSceneServer>) {
    for name in API_ONLY_TOOLS {
        router.remove_route(name);
    }
}

fn apply_enabled_tools_filter(router: &mut ToolRouter<CodeSceneServer>, config_data: &ConfigData) {
    let enabled = match config::enabled_tools(config_data) {
        Some(set) => set,
        None => return,
    };
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

#[tool_router]
impl CodeSceneServer {
    fn new(deps: ServerDeps) -> Self {
        let mut router = Self::tool_router();
        if deps.is_standalone {
            remove_standalone_tools(&mut router);
        }
        apply_enabled_tools_filter(&mut router, &deps.config_data);

        Self {
            tool_router: router,
            version_checker: deps.version_checker,
            auth_manager: deps.auth_manager,
            config_data: Arc::new(deps.config_data),
            instance_id: deps.instance_id,
            is_standalone: deps.is_standalone,
            cli_runner: deps.cli_runner,
            http_client: deps.http_client,
            validator: deps.validator,
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
        description = "Review the Code Health of a single source file and return a detailed\nCLI review output that includes the score and code smell findings.\n\nWhen to use:\n    Use this tool when you need actionable maintainability diagnostics\n    for one file (not just the score).\n\nLimitations:\n    - Analyzes one file at a time.\n    - Requires a supported source file.\n    - Returns CLI review text, not a normalized JSON schema.\n\nReturns:\n    A review string from the CodeScene CLI. The output typically\n    includes a Code Health score and code smell details to explain\n    why the score is high or low.\n\n    The Code Health scores are interpreted as:\n      * Optimal Code: a Code Health 10.0 is optimized for both human and AI comprehension\n      * Green Code: high quality with a score of 9.0-9.9\n      * Yellow Code: problematic technical debt with a score of 4.0-8.9\n      * Red Code: severe technical debt, maintainability issues, and expensive onboarding with a score of 1.0-3.9\n\nOn failure:\n    If this tool returns an error (e.g., the CLI cannot run, or Code Health\n    cannot be computed), run the verify_installation tool to diagnose the\n    cause, then report the failing checks and how to fix them.\n\nExample:\n    Call with file_path=\"/repo/src/app.py\" and summarize the returned\n    smells into prioritized refactoring actions.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_review(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_review::handle(self, params).await
    }

    #[tool(
        description = "Calculate the Code Health score for a single source file.\nThe tool returns one numeric score from 10.0 (optimal) to 1.0 (worst).\n\nWhen to use:\n    Use this tool for quick triage, ranking files by maintainability,\n    or checking whether a refactoring improved file-level quality.\n\nLimitations:\n    - Analyzes one file at a time.\n    - Returns only the score summary, not the full smell breakdown.\n    - Requires a supported source file.\n\nReturns:\n    A string in the format \"Code Health score: <score>\".\n\n    The Code Health scores are interpreted as:\n      * Optimal Code: Code Health 10.0 optimized for human and AI comprehension\n      * Green Code: high quality with a score of 9.0-9.9\n      * Yellow Code: problematic technical debt with a score of 4.0-8.9\n      * Red Code: severe technical debt with a score of 1.0-3.9\n\nOn failure:\n    If this tool returns an error (e.g., the CLI cannot run, or the score\n    cannot be computed), run the verify_installation tool to diagnose the\n    cause, then report the failing checks and how to fix them.\n\nExample:\n    Call with file_path=\"/repo/src/module.py\" and compare the score\n    before and after a refactoring.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_score(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_score::handle(self, params).await
    }

    #[tool(
        description = "Review all modified and staged files in a repository and report\nCode Health degradations before commit.\n\nWhen to use:\n    Use this tool as a pre-commit safeguard on local changes to catch\n    regressions and code smells before creating a commit.\n\nLimitations:\n    - Requires a valid git repository path.\n    - Evaluates current local modifications/staged changes only.\n    - Output is JSON text from the CLI command.\n\nReturns:\n    A JSON object containing:\n      - quality_gates: the central outcome, summarizing whether the commit passes or fails Code Health thresholds for each file.\n      - files: an array of objects for each file with:\n          - name: the name of the file whose Code Health is impacted (positively or negatively).\n          - findings: an array describing improvements/degradation for each code smell.\n      - Each quality gate indicates if the file meets the required Code Health standards, helping teams enforce healthy code before commit.\n\nOn failure:\n    If this tool returns an error (e.g., the CLI cannot run, or the git\n    repository cannot be analyzed), run the verify_installation tool to\n    diagnose the cause, then report the failing checks and how to fix them.\n\nExample:\n    Run on git_repository_path=\"/repo\" and block commit preparation if\n    any quality gate fails.",
        input_schema = inlined_schema_for::<GitRepoParam>()
    )]
    async fn pre_commit_code_health_safeguard(
        &self,
        Parameters(params): Parameters<GitRepoParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::pre_commit_code_health_safeguard::handle(self, params).await
    }

    #[tool(
        description = "Run a branch-level Code Health review for all files that differ between\ncurrent HEAD and base_ref.\n\nWhen to use:\n    Use this as a local PR pre-flight check before opening a pull\n    request, so regressions are caught across the full change set.\n\nLimitations:\n    - Requires a valid git repository path.\n    - base_ref must exist and be resolvable by git in that repository.\n    - Reviews only files that differ from base_ref.\n    - Output is JSON text from the CLI command.\n\nThe result can be used to decide whether to refactor before creating\nor updating a pull request.\n\nReturns:\n    A JSON object containing:\n      - quality_gates: the central outcome, summarizing whether the change\n        set passes or fails Code Health thresholds (\"passed\" or \"failed\").\n      - results: an array of objects for each affected file with:\n          - name: the name of the file whose Code Health is impacted.\n          - verdict: \"improved\", \"degraded\", or \"stable\".\n          - findings: an array describing improvements/degradation for each code smell.\n\nOn failure:\n    If this tool returns an error (e.g., the CLI cannot run, or base_ref\n    cannot be resolved by git), run the verify_installation tool to\n    diagnose the cause, then report the failing checks and how to fix them.\n\nExample:\n    Compare against base_ref=\"main\" for git_repository_path=\"/repo\" and\n    fail the local PR check if any file verdict is \"degraded\".",
        input_schema = inlined_schema_for::<ChangeSetParam>()
    )]
    async fn analyze_change_set(
        &self,
        Parameters(params): Parameters<ChangeSetParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::analyze_change_set::handle(self, params).await
    }

    #[tool(
        description = "Generate a data-driven business case for refactoring a source file.\n\nWhen to use:\n    Use this tool to justify refactoring investment with quantified\n    predictions tied to the file's current Code Health.\n\nLimitations:\n    - Estimates are model-based projections, not guarantees.\n    - Evaluates one file at a time.\n    - Requires an analyzable source file.\n\nReturns:\n    A JSON object with:\n        - scenario: Recommended target Code Health level.\n        - optimistic_outcome: Upper bound estimate for improvements\n          in development speed and defect reduction.\n        - pessimistic_outcome: Lower bound estimate for improvements.\n        - confidence_interval: The optimistic to pessimistic range,\n          representing a 90% confidence interval for the expected impact.\n\nOn failure:\n    If this tool returns an error (e.g., the CLI cannot run, or the file\n    cannot be analyzed), run the verify_installation tool to diagnose the\n    cause, then report the failing checks and how to fix them.\n\nExample:\n    Call with file_path=\"/repo/src/service.py\" and use the optimistic\n    and pessimistic outcomes to frame a refactoring proposal.",
        input_schema = inlined_schema_for::<FilePathParam>()
    )]
    async fn code_health_refactoring_business_case(
        &self,
        Parameters(params): Parameters<FilePathParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::code_health_refactoring_business_case::handle(self, params).await
    }

    #[tool(
        description = "Validate a Code Health rules configuration file (code-health-rules.json).\nThis is the file that customizes CodeScene's Code Health analysis by\nadjusting rule weights and thresholds.\n\nWhen to use:\n    Use this tool after creating or editing a Code Health rules file to\n    confirm it is well-formed before relying on it for analysis.\n\nLimitations:\n    - Local, filesystem-only operation. No CodeScene API or access token\n      is required.\n    - config_path, when provided, must be an absolute path.\n    - Without config_path, the CLI looks for\n      .codescene/code-health-rules.json in the current git repository.\n\nReturns:\n    A JSON object with:\n        - status: \"ok\" when the configuration is valid.\n        - summary: A human-readable count of rule sets, rule overrides,\n          and threshold overrides.\n\nOn failure:\n    If the file is missing or malformed, the error message describes the\n    problem and a suggested remedy. Fix the file (or supply a valid\n    config_path) and validate again.\n\nExample:\n    Call with config_path=\"/repo/.codescene/code-health-rules.json\" and\n    report whether the Code Health rules configuration is valid.",
        input_schema = inlined_schema_for::<RulesConfigValidateParam>()
    )]
    async fn rules_config_validate(
        &self,
        Parameters(params): Parameters<RulesConfigValidateParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::rules_config_validate::handle(self, params).await
    }

    #[tool(
        description = "List the default Code Health thresholds for a programming language.\nThresholds define what Code Health issues such as \"Complex Method\" or\n\"Large Method\" mean for a given language, and are the values you can\noverride in a code-health-rules.json file.\n\nWhen to use:\n    Use this tool to discover the built-in threshold names and default\n    values for a language before overriding any of them with\n    rules_config_set_threshold.\n\nLimitations:\n    - Local, filesystem-only operation. No CodeScene API or access token\n      is required.\n    - Requires a supported language name (e.g., Python, JavaScript, Java,\n      C#).\n    - config_path, when provided, must be an absolute path; the returned\n      values then reflect that file's overrides. Without it, built-in\n      defaults are shown.\n\nReturns:\n    A JSON object keyed by rule-set name, each containing a `thresholds`\n    array of `{ name, value }` entries.\n\nOn failure:\n    If the language is not supported, the error message lists supported\n    languages. Retry with a valid language name.\n\nExample:\n    Call with language=\"Python\" and present the threshold names and\n    values the user can customize.",
        input_schema = inlined_schema_for::<RulesConfigListThresholdsParam>()
    )]
    async fn rules_config_list_thresholds(
        &self,
        Parameters(params): Parameters<RulesConfigListThresholdsParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::rules_config_list_thresholds::handle(self, params).await
    }

    #[tool(
        description = "Enable or disable a Code Health rule in a code-health-rules.json file.\nDisabling a rule removes its impact from CodeScene's Code Health score;\nenabling it restores the default behavior.\n\nWhen to use:\n    Use this tool to turn a specific Code Health rule (e.g., \"Complex\n    Method\") on or off for a repository. This edits the rules file on\n    disk.\n\nLimitations:\n    - Local, filesystem-only operation that WRITES to the rules file. No\n      CodeScene API or access token is required.\n    - rule_name must be a supported Code Health rule name.\n    - config_path, when provided, must be an absolute path. Without it,\n      the CLI edits .codescene/code-health-rules.json in the current git\n      repository.\n    - matching_content_path is required only when the file defines\n      multiple rule sets; it selects which one to edit.\n\nReturns:\n    A confirmation message naming the rule, its new enabled/disabled\n    state, and the rule set that was edited.\n\nOn failure:\n    If the rule name is unknown the error suggests valid names; if\n    multiple rule sets exist without a selector the error lists the\n    available selectors. Retry with the corrected input.\n\nExample:\n    Call with rule_name=\"Complex Method\" and enabled=false to disable the\n    Complex Method rule, then confirm the change to the user.",
        input_schema = inlined_schema_for::<RulesConfigSetRuleParam>()
    )]
    async fn rules_config_set_rule(
        &self,
        Parameters(params): Parameters<RulesConfigSetRuleParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::rules_config_set_rule::handle(self, params).await
    }

    #[tool(
        description = "Set a Code Health threshold value in a code-health-rules.json file.\nThresholds tune the sensitivity of Code Health issues (for example, the\nnumber of lines at which a function is flagged as a \"Large Method\").\n\nWhen to use:\n    Use this tool to override a specific Code Health threshold for a\n    repository. This edits the rules file on disk. Use\n    rules_config_list_thresholds first to find valid threshold names.\n\nLimitations:\n    - Local, filesystem-only operation that WRITES to the rules file. No\n      CodeScene API or access token is required.\n    - threshold_name must be a supported Code Health threshold name.\n    - value must be a positive integer.\n    - config_path, when provided, must be an absolute path. Without it,\n      the CLI edits .codescene/code-health-rules.json in the current git\n      repository.\n    - matching_content_path is required only when the file defines\n      multiple rule sets; it selects which one to edit.\n\nReturns:\n    A confirmation message naming the threshold, its new value, and the\n    rule set that was edited.\n\nOn failure:\n    If the threshold name is unknown or the value is not a positive\n    integer, the error message explains the problem. Retry with corrected\n    input.\n\nExample:\n    Call with threshold_name=\"function_lines_of_code_warning\" and\n    value=120 to raise the Large Method warning threshold, then confirm\n    the change.",
        input_schema = inlined_schema_for::<RulesConfigSetThresholdParam>()
    )]
    async fn rules_config_set_threshold(
        &self,
        Parameters(params): Parameters<RulesConfigSetThresholdParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::rules_config_set_threshold::handle(self, params).await
    }

    #[tool(
        description = "Lists all projects for an organization for selection by the user.\nThe user can select the desired project by either its name or ID.\n\nWhen to use:\n    Use this tool before project-scoped API tools so the user can pick\n    the project context explicitly.\n\nLimitations:\n    - If default_project_id is configured, the server returns that\n      project and selection is effectively locked.\n\nReturns:\n    A JSON object with the project name and ID, formatted in a Markdown table\n    with the columns \"Project Name\" and \"Project ID\". If the output contains a\n    `description` field, it indicates that a default project is configured\n    (`default_project_id`), and the user cannot select a different project.\n    Explain this to the user.\n\n    Additionally, a `link` field is provided to guide the user to the\n    Codescene projects page where the user can find more detailed information about each project.\n    Make sure to include this link in the output, and explain its purpose clearly.\n\nOn failure:\n    If this tool returns an error (e.g., the CodeScene API cannot be\n    reached, or the access token is missing/invalid), run the\n    verify_installation tool to diagnose the cause, then report the\n    failing checks and how to fix them.\n\nExample:\n    Call without arguments. If default_project_id is configured,\n    explain that the returned project is fixed unless that config is changed via set_config."
    )]
    async fn select_project(&self) -> Result<CallToolResult, ErrorData> {
        tools::select_project::handle(self).await
    }

    #[tool(
        description = "Lists the technical debt goals for a project.\n\nWhen to use:\n    Use this tool to see all files in a project that currently have\n    explicit technical debt goals in CodeScene.\n\nLimitations:\n    - Requires a valid project_id.\n    - Returns goal data from the latest available analysis.\n    - Includes only files with non-empty goals.\n\nReturns:\n    A JSON object with two fields:\n    - `data`: an array of objects each containing the path of a file and its goals.\n    - `link`: a URL to the CodeScene Code Biomarkers page for the project.\n\n    Show the goals for each file in a structured format that is easy to read and explain\n    the goal description for each file. It also includes a description, please include that in your output.\n    Always include the `link` in the output and explain that more details about the\n    technical debt goals can be found on that page.\n\nOn failure:\n    If this tool returns an error (e.g., the CodeScene API cannot be\n    reached, or the access token is missing/invalid), run the\n    verify_installation tool to diagnose the cause, then report the\n    failing checks and how to fix them.\n\nExample:\n    Call with project_id=42 and summarize each file's goals with the\n    biomarkers link for deeper inspection.",
        input_schema = inlined_schema_for::<ProjectParam>()
    )]
    async fn list_technical_debt_goals_for_project(
        &self,
        Parameters(params): Parameters<ProjectParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_goals_for_project::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt goals for a specific file in a project.\n\nWhen to use:\n    Use this tool when you need goal details for one file before\n    planning targeted refactoring work.\n\nLimitations:\n    - Requires a valid project_id.\n    - Returns data from the latest available analysis only.\n    - A file may return zero goals, which is a valid outcome.\n\nReturns:\n    A JSON object with two fields:\n    - `data`: an array containing the goals for the specified file.\n    - `link`: a URL to the CodeScene Code Biomarkers page for the specific file.\n\n    Show the goals in a structured format that is easy to read and explain\n    the goal description. It also includes a description, please include that in your output.\n    Always include the `link` in the output and explain that more details about the\n    technical debt goals can be found on that page.\n\nOn failure:\n    If this tool returns an error (e.g., the CodeScene API cannot be\n    reached, or the access token is missing/invalid), run the\n    verify_installation tool to diagnose the cause, then report the\n    failing checks and how to fix them.\n\nExample:\n    Call with file_path=\"/repo/src/module.py\" and project_id=42, then\n    use the goals and link to propose file-level improvements.",
        input_schema = inlined_schema_for::<ProjectFileParam>()
    )]
    async fn list_technical_debt_goals_for_project_file(
        &self,
        Parameters(params): Parameters<ProjectFileParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_goals_for_project_file::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt hotspots for a project.\n\nWhen to use:\n    Use this tool to identify high-impact technical debt hotspots across\n    a project and prioritize refactoring targets.\n\nLimitations:\n    - Requires a valid project_id.\n    - Returns hotspots from the latest available project analysis.\n    - Uses the API's hotspot filtering and pagination behavior.\n\nReturns:\n    A JSON object with two fields:\n    - `data`: an array of objects each containing the path of a file, code health score, revisions count and lines of code count.\n    - `link`: a URL to the CodeScene technical debt hotspots page for the project.\n\n    Describe the hotspots for each file in a structured format that is easy to read and explain.\n    It also includes a description, please include that in your output.\n    Always include the `link` in the output and explain that the user can find more\n    detailed information about each hotspot on that page.\n\nOn failure:\n    If this tool returns an error (e.g., the CodeScene API cannot be\n    reached, or the access token is missing/invalid), run the\n    verify_installation tool to diagnose the cause, then report the\n    failing checks and how to fix them.\n\nExample:\n    Call with project_id=42 and rank returned hotspots by code health\n    and revision frequency before proposing refactoring work.",
        input_schema = inlined_schema_for::<ProjectParam>()
    )]
    async fn list_technical_debt_hotspots_for_project(
        &self,
        Parameters(params): Parameters<ProjectParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_hotspots_for_project::handle(self, params).await
    }

    #[tool(
        description = "Lists the technical debt hotspots for a specific file in a project.\n\nWhen to use:\n    Use this tool to inspect hotspot metrics for one file before\n    deciding if it should be a refactoring candidate.\n\nLimitations:\n    - Requires a valid project_id.\n    - Returns at most one hotspot object for the filtered file path.\n    - If no hotspot exists for the file, returns an empty hotspot object.\n\nReturns:\n    A JSON object with two fields:\n    - `data`: an array containing the code health score, revisions count and lines of code count for the specified file.\n    - `link`: a URL to the CodeScene technical debt hotspots page for the project.\n\n    Describe the hotspot in a structured format that is easy to read and explain.\n    It also includes a description, please include that in your output.\n    Always include the `link` in the output and explain that the user can find more\n    detailed information about each hotspot on that page.\n\nOn failure:\n    If this tool returns an error (e.g., the CodeScene API cannot be\n    reached, or the access token is missing/invalid), run the\n    verify_installation tool to diagnose the cause, then report the\n    failing checks and how to fix them.\n\nExample:\n    Call with file_path=\"/repo/src/module.py\" and project_id=42. If\n    hotspot is empty, report that the file is not currently a hotspot.",
        input_schema = inlined_schema_for::<ProjectFileParam>()
    )]
    async fn list_technical_debt_hotspots_for_project_file(
        &self,
        Parameters(params): Parameters<ProjectFileParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::list_technical_debt_hotspots_for_project_file::handle(self, params).await
    }

    #[tool(
        description = "Find the owner or owners of a specific path in a project.\n\nWhen to use:\n    Use this tool to identify likely reviewers or domain experts for\n    code reviews and technical questions about a file or directory.\n\nLimitations:\n    - Requires a valid project_id.\n    - Uses the latest project analysis data available in CodeScene.\n    - If no matching ownership data is found, an empty JSON array is returned.\n\nReturns:\n    A list of owners and their paths that they own. The name of the owner who can be responsible\n    for code reviews or inquiries about the file and a link to the CodeScene System Map page filtered\n    by the owner. Explain that this link can be used to see more details\n    about the owner's contributions and interactions within the project.\n    You MUST always show a link after every owner. Show resulting data in a Markdown\n    table with columns: Owner, Key Areas, Link.\n\nOn failure:\n    If this tool returns an error (e.g., the CodeScene API cannot be\n    reached, or the access token is missing/invalid), run the\n    verify_installation tool to diagnose the cause, then report the\n    failing checks and how to fix them.\n\nExample:\n    Call with project_id=42 and path=\"/repo/src/service.py\", then\n    present each owner row with its corresponding system-map link.",
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

    #[tool(
        description = "Sign in to CodeScene using OAuth.\n\nWhen to use:\n    Use this tool when no CS_ACCESS_TOKEN env to authenticate. Can be used during read-only mode. If the browser cannot be opened automatically, the URL\n    is printed so the user can open it manually.\n\nLimitations:\n    - Not needed when CS_ACCESS_TOKEN is already set.\n    - Requires the embedded CLI to be available.\n    - Waits up to 2 minutes for the user to complete the browser flow.\n    - For on-prem instances, configure CS_ONPREM_URL before calling this tool.\n\nReturns:\n    A success message when signed in, or an error describing the failure.",
        input_schema = inlined_schema_for::<LoginParam>()
    )]
    async fn login(
        &self,
        Parameters(params): Parameters<LoginParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::login::handle(self, params).await
    }

    #[tool(
        description = "List all available skills embedded in this MCP server.\n\nWhen to use:\n    Use this tool to discover what skills are available for\n    download or inspection.\n\nLimitations:\n    - Returns only skills embedded at compile time.\n    - Does not scan external skill directories.\n\nReturns:\n    A formatted list of skill names with their descriptions.\n\nExample:\n    Call this tool to see all available skills, then use\n    download_skill or sync_skills to install them locally."
    )]
    async fn list_skills(&self) -> Result<CallToolResult, ErrorData> {
        tools::list_skills::handle(self).await
    }

    #[tool(
        description = "Get the file manifest for a specific skill.\n\nWhen to use:\n    Use this tool to inspect what files a skill contains,\n    their sizes, and SHA256 hashes before downloading.\n\nLimitations:\n    - Requires a valid skill name from list_skills.\n\nReturns:\n    A JSON manifest with the skill name and an array of files,\n    each with path, size in bytes, and sha256 hash.\n\nExample:\n    Call with skill_name=\"safeguarding-ai-generated-code\" to\n    see the manifest, then use download_skill to install it.",
        input_schema = inlined_schema_for::<SkillNameParam>()
    )]
    async fn get_skill_manifest(
        &self,
        Parameters(params): Parameters<SkillNameParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::get_skill_manifest::handle(self, params).await
    }

    #[tool(
        description = "Download a single skill to a local directory.\n\nWhen to use:\n    Use this tool to install a specific skill into your local\n    skills directory (e.g., ~/.claude/skills/).\n\nLimitations:\n    - By default, refuses to overwrite existing skills.\n    - Set overwrite=true to replace an existing skill.\n    - Creates the destination directory if it does not exist.\n\nReturns:\n    A confirmation message with the path where the skill was written.\n\nExample:\n    Call with skill_name=\"safeguarding-ai-generated-code\" and\n    destination_dir=\"~/.claude/skills\" to install the skill.",
        input_schema = inlined_schema_for::<DownloadSkillParam>()
    )]
    async fn download_skill(
        &self,
        Parameters(params): Parameters<DownloadSkillParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::download_skill::handle(self, params).await
    }

    #[tool(
        description = "Download all available skills to a local directory.\n\nWhen to use:\n    Use this tool to install every embedded skill into your\n    local skills directory at once.\n\nLimitations:\n    - By default, skips skills that already exist locally.\n    - Set overwrite=true to replace all existing skills.\n    - Creates the destination directory if it does not exist.\n\nReturns:\n    A summary showing how many skills were downloaded and how\n    many were skipped (if any already existed).\n\nExample:\n    Call with destination_dir=\"~/.claude/skills\" to install\n    all skills. Use overwrite=true to force-update them.",
        input_schema = inlined_schema_for::<SyncSkillsParam>()
    )]
    async fn sync_skills(
        &self,
        Parameters(params): Parameters<SyncSkillsParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::sync_skills::handle(self, params).await
    }

    #[tool(
        description = "Check if the CodeScene MCP Server is correctly installed and configured.\n\nWhen to use:\n    Use this tool to diagnose setup issues such as missing tokens,\n    unavailable git, or environment misconfigurations.\n\nLimitations:\n    - Does not modify any configuration.\n    - Token validation requires a git repository path.\n\nReturns:\n    A summary of verification checks with PASS/FAIL status for each:\n      - Git: whether git is installed and accessible.\n      - Git Repository: whether the given path is inside a git repository.\n      - Access Token: whether CS_ACCESS_TOKEN is set and valid (verified via the CLI).\n      - API Connectivity: whether the MCP server can reach the CodeScene API (cloud or on-prem). Catches TLS/CA certificate misconfiguration. Skipped for standalone licenses.\n      - CLI Connectivity: whether the CLI can reach CodeScene. Catches TLS/CA certificate misconfiguration on the CLI path.\n      - Runtime Environment: whether running as binary or Docker.\n\nExample:\n    Call this tool when a user reports issues or after initial setup\n    to confirm everything is working. Pass the project root directory\n    as git_repository_path.",
        input_schema = inlined_schema_for::<GitRepoParam>()
    )]
    async fn verify_installation(
        &self,
        Parameters(params): Parameters<GitRepoParam>,
    ) -> Result<CallToolResult, ErrorData> {
        tools::verify_installation::handle(self, params).await
    }
}

/// Initialise tracing with stderr output and optional file logging.
///
/// Returns the non-blocking file-appender guard when file logging is
/// active.  The guard must be held for the lifetime of the program so
/// that buffered log entries are flushed on shutdown.
fn init_tracing(
    config_data: &config::ConfigData,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let env_filter = EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into());
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false);

    let retention_days = config::log_retention_days(config_data);
    if retention_days > 0 {
        let log_dir = config::log_dir();
        if let Ok(()) = std::fs::create_dir_all(&log_dir) {
            cleanup_old_logs(&log_dir, retention_days);
            let file_appender = tracing_appender::rolling::daily(&log_dir, "mcp.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            let file_layer = tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false);
            tracing_subscriber::registry()
                .with(env_filter)
                .with(stderr_layer)
                .with(file_layer)
                .init();
            return Some(guard);
        }
    }

    // Stderr-only: file logging disabled or log directory not writable
    // (e.g. non-root container with a read-only config path).
    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .init();
    None
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

    config::snapshot_client_env_vars();
    ensure_oauth_client_configured().await;
    let config_data = config::load().unwrap_or_default();
    config::apply_to_env(&config_data);

    let _file_guard = init_tracing(&config_data);

    startup::print_startup_logo();
    tracing::info!("CodeScene MCP server started");
    tracing::info!("Waiting for MCP client initialization...");

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
        auth_manager: AuthManager::new(),
        cli_runner: Arc::new(cli::ProductionCliRunner),
        http_client: Arc::new(http::ReqwestClient),
        validator: Arc::new(tools::validation::ProductionCliValidator),
    });

    // Covered by e2e test: test_shutdown_during_handshake.py
    let Some(service) = serve_or_handle_disconnect(server, rmcp::transport::stdio()).await? else {
        return Ok(());
    };

    tracing::info!("CodeScene MCP server ready");

    service.waiting().await?;
    Ok(())
}

pub(crate) async fn serve_or_handle_disconnect<T, E, A>(
    server: CodeSceneServer,
    transport: T,
) -> anyhow::Result<Option<rmcp::service::RunningService<rmcp::service::RoleServer, CodeSceneServer>>>
where
    T: rmcp::transport::IntoTransport<rmcp::service::RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    match server.serve(transport).await {
        Ok(service) => Ok(Some(service)),
        Err(err) => {
            handle_serve_error(err)?;
            Ok(None)
        }
    }
}

/// Convert a `serve()` error into the desired process exit behavior.
///
/// MCP clients (e.g. VS Code, Zed) routinely close the server's stdin
/// when the user closes the agent — sometimes before the MCP handshake
/// has completed. That looks like a `ConnectionClosed` error during
/// initialization, but is a normal shutdown from the client's
/// perspective. Treat it as a clean exit so the client does not
/// surface a "fatal error" dialog.
pub(crate) fn handle_serve_error(err: ServerInitializeError) -> anyhow::Result<()> {
    if let ServerInitializeError::ConnectionClosed(context) = &err {
        tracing::info!(
            "MCP client disconnected during initialization ({context}); shutting down cleanly"
        );
        if context.contains("initialize request") {
            tracing::info!(
                "No MCP initialize request received. If you ran `cs-mcp` directly in a terminal, run it through an MCP client instead."
            );
        }
        return Ok(());
    }

    tracing::error!("serving error: {err:?}");
    Err(err.into())
}

/// Remove log files older than `retention_days` from the given directory.
fn cleanup_old_logs(log_dir: &std::path::Path, retention_days: u32) {
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(u64::from(retention_days) * 24 * 60 * 60);
    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified < cutoff {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

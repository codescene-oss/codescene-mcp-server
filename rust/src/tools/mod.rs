use rmcp::schemars;
use serde::Deserialize;

pub mod analyze_change_set;
pub mod code_health_auto_refactor;
pub mod code_health_refactoring_business_case;
pub mod code_health_review;
pub mod code_health_score;
pub mod code_ownership_for_path;
pub mod common;
pub mod explain_code_health;
pub mod explain_code_health_productivity;
pub mod get_config;
pub mod list_technical_debt_goals_for_project;
pub mod list_technical_debt_goals_for_project_file;
pub mod list_technical_debt_hotspots_for_project;
pub mod list_technical_debt_hotspots_for_project_file;
pub mod pre_commit_code_health_safeguard;
pub mod select_project;
pub mod set_config;

/// Optional context parameter used by explain tools.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OptionalContext {
    /// Optional context string from the MCP protocol.
    /// This argument is not used to customize output.
    #[allow(dead_code)]
    #[serde(default)]
    pub context: Option<String>,
}

/// File path parameter for single-file analysis tools.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FilePathParam {
    /// Absolute path to the source code file to analyze.
    pub file_path: String,
}

/// Git repository path parameter.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GitRepoParam {
    /// Absolute path to the local git repository to analyze.
    pub git_repository_path: String,
}

/// Parameters for analyzing a change set (branch diff).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ChangeSetParam {
    /// Git reference to compare against (e.g., "main" or "origin/main").
    pub base_ref: String,

    /// Absolute path to the local git repository.
    pub git_repository_path: String,
}

/// Parameters for auto-refactoring a function.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RefactorParam {
    /// Absolute path to the source file containing the target function.
    pub file_path: String,

    /// Exact function name to refactor. Include class scope prefix
    /// when relevant.
    pub function_name: String,
}

/// Parameters for selecting/listing projects.
// No additional params needed — the tool lists all projects.

/// Parameters for project-scoped tools.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectParam {
    /// The Project ID selected by the user.
    pub project_id: i64,
}

/// Parameters for project + file path tools.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProjectFileParam {
    /// The absolute path to the source code file.
    pub file_path: String,

    /// The Project ID selected by the user.
    pub project_id: i64,
}

/// Parameters for code ownership lookup.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OwnershipParam {
    /// CodeScene project identifier.
    pub project_id: i64,

    /// Absolute or repository-relative path to a file or directory.
    pub path: String,
}

/// Parameters for get_config (optional key).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetConfigParam {
    /// Optional config key to query. Omit to list all options.
    #[serde(default)]
    pub key: Option<String>,
}

/// Parameters for set_config.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetConfigParam {
    /// The configuration key to set.
    pub key: String,

    /// The value to store. Pass an empty string to remove the key.
    pub value: String,
}

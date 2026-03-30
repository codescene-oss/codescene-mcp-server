use rmcp::schemars;
use serde::Deserialize;

pub mod analyze_change_set;
pub mod code_health_auto_refactor;
pub mod code_health_refactoring_business_case;
pub mod code_health_review;
pub mod code_health_score;
pub mod code_ownership_for_path;
pub mod codescene_links;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_context_deserializes_with_value() {
        let json = r#"{"context": "some context"}"#;
        let p: OptionalContext = serde_json::from_str(json).unwrap();
        assert_eq!(p.context.as_deref(), Some("some context"));
    }

    #[test]
    fn optional_context_deserializes_without_value() {
        let json = r#"{}"#;
        let p: OptionalContext = serde_json::from_str(json).unwrap();
        assert!(p.context.is_none());
    }

    #[test]
    fn file_path_param_deserializes() {
        let json = r#"{"file_path": "/tmp/foo.rs"}"#;
        let p: FilePathParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.file_path, "/tmp/foo.rs");
    }

    #[test]
    fn git_repo_param_deserializes() {
        let json = r#"{"git_repository_path": "/my/repo"}"#;
        let p: GitRepoParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.git_repository_path, "/my/repo");
    }

    #[test]
    fn change_set_param_deserializes() {
        let json = r#"{"base_ref": "main", "git_repository_path": "/repo"}"#;
        let p: ChangeSetParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.base_ref, "main");
        assert_eq!(p.git_repository_path, "/repo");
    }

    #[test]
    fn refactor_param_deserializes() {
        let json = r#"{"file_path": "/src/lib.rs", "function_name": "do_stuff"}"#;
        let p: RefactorParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.file_path, "/src/lib.rs");
        assert_eq!(p.function_name, "do_stuff");
    }

    #[test]
    fn project_param_deserializes() {
        let json = r#"{"project_id": 42}"#;
        let p: ProjectParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.project_id, 42);
    }

    #[test]
    fn project_file_param_deserializes() {
        let json = r#"{"file_path": "/a/b.rs", "project_id": 7}"#;
        let p: ProjectFileParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.file_path, "/a/b.rs");
        assert_eq!(p.project_id, 7);
    }

    #[test]
    fn ownership_param_deserializes() {
        let json = r#"{"project_id": 1, "path": "src/"}"#;
        let p: OwnershipParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.project_id, 1);
        assert_eq!(p.path, "src/");
    }

    #[test]
    fn get_config_param_deserializes_with_key() {
        let json = r#"{"key": "access_token"}"#;
        let p: GetConfigParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.key.as_deref(), Some("access_token"));
    }

    #[test]
    fn get_config_param_deserializes_without_key() {
        let json = r#"{}"#;
        let p: GetConfigParam = serde_json::from_str(json).unwrap();
        assert!(p.key.is_none());
    }

    #[test]
    fn set_config_param_deserializes() {
        let json = r#"{"key": "access_token", "value": "abc123"}"#;
        let p: SetConfigParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.key, "access_token");
        assert_eq!(p.value, "abc123");
    }
}

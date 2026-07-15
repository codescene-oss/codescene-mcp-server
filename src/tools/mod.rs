use rmcp::schemars;
use serde::Deserialize;

pub mod analyze_change_set;
pub mod code_health_refactoring_business_case;
pub mod code_health_review;
pub mod code_health_score;
pub mod code_ownership_for_path;
pub mod codescene_links;
pub mod common;
pub mod download_skill;
pub mod explain_code_health;
pub mod explain_code_health_productivity;
pub mod get_config;
pub mod get_skill_manifest;
pub mod list_skills;
pub mod list_technical_debt_goals_for_project;
pub mod list_technical_debt_goals_for_project_file;
pub mod list_technical_debt_hotspots_for_project;
pub mod list_technical_debt_hotspots_for_project_file;
pub mod login;
pub mod pre_commit_code_health_safeguard;
pub mod rules_config;
pub mod rules_config_list_thresholds;
pub mod rules_config_set_rule;
pub mod rules_config_set_threshold;
pub mod rules_config_validate;
pub mod select_project;
pub mod set_config;
pub mod sync_skills;
pub mod validation;
pub mod verify_installation;

/// Parameters for the `login` tool (currently accepts no arguments).
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LoginParam {}

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

/// Parameters for `rules_config_validate` — validate a Code Health rules file.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RulesConfigValidateParam {
    /// Absolute path to the `code-health-rules.json` file to validate.
    /// Omit to use `.codescene/code-health-rules.json` in the current git repo.
    #[serde(default)]
    pub config_path: Option<String>,
}

/// Parameters for `rules_config_list_thresholds` — list a language's default
/// Code Health thresholds.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RulesConfigListThresholdsParam {
    /// Language name whose default Code Health thresholds should be listed
    /// (e.g., "Python", "JavaScript", "Java", "C#").
    pub language: String,

    /// Absolute path to the `code-health-rules.json` file to read overrides
    /// from. Omit to use `.codescene/code-health-rules.json` in the current
    /// git repo, or to show built-in defaults when no config exists.
    #[serde(default)]
    pub config_path: Option<String>,
}

/// Parameters for `rules_config_set_rule` — enable or disable a Code Health
/// rule in a rules file.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RulesConfigSetRuleParam {
    /// Name of the Code Health rule to toggle (e.g., "Complex Method").
    pub rule_name: String,

    /// Whether the rule should be enabled (`true`) or disabled (`false`).
    pub enabled: bool,

    /// Glob selecting which rule set to edit (e.g., "**/*.js"). Required only
    /// when the config defines multiple rule sets; omit for a single rule set.
    #[serde(default)]
    pub matching_content_path: Option<String>,

    /// Absolute path to the `code-health-rules.json` file to edit. Omit to use
    /// `.codescene/code-health-rules.json` in the current git repo.
    #[serde(default)]
    pub config_path: Option<String>,
}

/// Parameters for `rules_config_set_threshold` — set a Code Health threshold
/// value in a rules file.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RulesConfigSetThresholdParam {
    /// Name of the Code Health threshold to set
    /// (e.g., "function_lines_of_code_warning").
    pub threshold_name: String,

    /// New threshold value. Must be a positive integer.
    pub value: u32,

    /// Glob selecting which rule set to edit (e.g., "**/*.js"). Required only
    /// when the config defines multiple rule sets; omit for a single rule set.
    #[serde(default)]
    pub matching_content_path: Option<String>,

    /// Absolute path to the `code-health-rules.json` file to edit. Omit to use
    /// `.codescene/code-health-rules.json` in the current git repo.
    #[serde(default)]
    pub config_path: Option<String>,
}

/// Parameters for get_skill_manifest.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SkillNameParam {
    /// Name of the skill (e.g., "safeguarding-ai-generated-code").
    pub skill_name: String,
}

/// Parameters for download_skill.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DownloadSkillParam {
    /// Name of the skill to download (e.g., "safeguarding-ai-generated-code").
    pub skill_name: String,

    /// Directory to write the skill into. A subdirectory named after
    /// the skill will be created containing SKILL.md.
    pub destination_dir: String,

    /// Whether to overwrite an existing skill. Defaults to false.
    #[serde(default)]
    pub overwrite: bool,
}

/// Parameters for sync_skills.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SyncSkillsParam {
    /// Directory to write all skills into. Each skill gets its own
    /// subdirectory containing SKILL.md.
    pub destination_dir: String,

    /// Whether to overwrite existing skills. Defaults to false.
    #[serde(default)]
    pub overwrite: bool,
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

    #[test]
    fn skill_name_param_deserializes() {
        let json = r#"{"skill_name": "my-skill"}"#;
        let p: SkillNameParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.skill_name, "my-skill");
    }

    #[test]
    fn rules_config_validate_param_defaults_config_path_to_none() {
        let p: RulesConfigValidateParam = serde_json::from_str("{}").unwrap();
        assert!(p.config_path.is_none());
    }

    #[test]
    fn rules_config_list_thresholds_param_deserializes() {
        let json = r#"{"language": "Python"}"#;
        let p: RulesConfigListThresholdsParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.language, "Python");
        assert!(p.config_path.is_none());
    }

    #[test]
    fn rules_config_set_rule_param_deserializes() {
        let json = r#"{"rule_name": "Complex Method", "enabled": false}"#;
        let p: RulesConfigSetRuleParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.rule_name, "Complex Method");
        assert!(!p.enabled);
        assert!(p.matching_content_path.is_none());
        assert!(p.config_path.is_none());
    }

    #[test]
    fn rules_config_set_threshold_param_deserializes() {
        let json = r#"{"threshold_name": "function_lines_of_code_warning", "value": 120}"#;
        let p: RulesConfigSetThresholdParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.threshold_name, "function_lines_of_code_warning");
        assert_eq!(p.value, 120);
    }

    #[test]
    fn download_skill_param_deserializes() {
        let json = r#"{"skill_name": "my-skill", "destination_dir": "/tmp/skills"}"#;
        let p: DownloadSkillParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.skill_name, "my-skill");
        assert_eq!(p.destination_dir, "/tmp/skills");
        assert!(!p.overwrite);
    }

    #[test]
    fn download_skill_param_with_overwrite() {
        let json = r#"{"skill_name": "x", "destination_dir": "/tmp", "overwrite": true}"#;
        let p: DownloadSkillParam = serde_json::from_str(json).unwrap();
        assert!(p.overwrite);
    }

    #[test]
    fn sync_skills_param_deserializes() {
        let json = r#"{"destination_dir": "/tmp/skills"}"#;
        let p: SyncSkillsParam = serde_json::from_str(json).unwrap();
        assert_eq!(p.destination_dir, "/tmp/skills");
        assert!(!p.overwrite);
    }
}

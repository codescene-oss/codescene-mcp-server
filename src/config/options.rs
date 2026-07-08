pub struct ConfigOption {
    pub key: &'static str,
    pub env_var: &'static str,
    pub description: &'static str,
    pub sensitive: bool,
    pub hidden: bool,
    pub api_only: bool,
    pub aliases: &'static [&'static str],
    pub docs_url: &'static str,
}

pub const OPTIONS: &[ConfigOption] = &[
    ConfigOption {
        key: "access_token",
        env_var: "CS_ACCESS_TOKEN",
        description: "CodeScene access token (PAT or standalone license)",
        sensitive: true,
        hidden: false,
        api_only: false,
        aliases: &["token", "pat"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
    ConfigOption {
        key: "onprem_url",
        env_var: "CS_ONPREM_URL",
        description: "CodeScene on-premise server URL",
        sensitive: false,
        hidden: false,
        api_only: true,
        aliases: &["url", "server_url"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
    ConfigOption {
        key: "default_project_id",
        env_var: "CS_DEFAULT_PROJECT_ID",
        description: "Default CodeScene project ID",
        sensitive: false,
        hidden: false,
        api_only: true,
        aliases: &["project_id", "project"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
    ConfigOption {
        key: "disable_tracking",
        env_var: "CS_DISABLE_TRACKING",
        description: "Disable anonymous usage analytics",
        sensitive: false,
        hidden: true,
        api_only: false,
        aliases: &["no_tracking"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
    ConfigOption {
        key: "disable_version_check",
        env_var: "CS_DISABLE_VERSION_CHECK",
        description: "Disable automatic version update checks",
        sensitive: false,
        hidden: true,
        api_only: false,
        aliases: &["no_version_check"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
    ConfigOption {
        key: "tracking_environment",
        env_var: "CS_ENVIRONMENT",
        description: "Override analytics environment label sent in tracking events",
        sensitive: false,
        hidden: true,
        api_only: false,
        aliases: &["environment"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
    ConfigOption {
        key: "ca_bundle",
        env_var: "REQUESTS_CA_BUNDLE",
        description: "Path to custom CA certificate bundle (PEM)",
        sensitive: false,
        hidden: false,
        api_only: false,
        aliases: &["ssl_cert", "cert"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
    ConfigOption {
        key: "enabled_tools",
        env_var: "CS_ENABLED_TOOLS",
        description: "Comma-separated allowlist of tool names to enable (unset = all enabled). Requires restart.",
        sensitive: false,
        hidden: false,
        api_only: false,
        aliases: &["tools"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
    ConfigOption {
        key: "log_retention_days",
        env_var: "CS_LOG_RETENTION_DAYS",
        description: "Number of days to keep log files (default: 7, set to 0 to disable file logging)",
        sensitive: false,
        hidden: false,
        api_only: false,
        aliases: &["log_days"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
];

/// Tool names that can be enabled/disabled via the `enabled_tools` config.
/// Excludes `get_config` and `set_config` which are always available.
pub const CONFIGURABLE_TOOL_NAMES: &[&str] = &[
    "explain_code_health",
    "explain_code_health_productivity",
    "code_health_review",
    "code_health_score",
    "pre_commit_code_health_safeguard",
    "analyze_change_set",
    "code_health_refactoring_business_case",
    "rules_config_validate",
    "rules_config_list_thresholds",
    "rules_config_set_rule",
    "rules_config_set_threshold",
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
];

pub fn find_option(key: &str) -> Option<&'static ConfigOption> {
    OPTIONS
        .iter()
        .find(|o| o.key == key || o.env_var == key || o.aliases.contains(&key))
}

#[allow(dead_code)]
pub fn is_valid_key(key: &str) -> bool {
    find_option(key).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_option_by_key() {
        let opt = find_option("access_token");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().env_var, "CS_ACCESS_TOKEN");
    }

    #[test]
    fn find_option_by_alias() {
        let opt = find_option("token");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().key, "access_token");
    }

    #[test]
    fn find_option_by_env_var() {
        let opt = find_option("CS_ACCESS_TOKEN");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().key, "access_token");
    }

    #[test]
    fn find_option_unknown_returns_none() {
        assert!(find_option("nonexistent_key").is_none());
    }

    #[test]
    fn is_valid_key_known() {
        assert!(is_valid_key("access_token"));
        assert!(is_valid_key("onprem_url"));
        assert!(is_valid_key("ca_bundle"));
    }

    #[test]
    fn is_valid_key_unknown() {
        assert!(!is_valid_key("bad_key"));
    }

    #[test]
    fn options_list_is_not_empty() {
        assert!(!OPTIONS.is_empty());
        for opt in OPTIONS {
            assert!(!opt.key.is_empty());
            assert!(!opt.env_var.is_empty());
            assert!(!opt.description.is_empty());
        }
    }

    #[test]
    fn configurable_tool_names_excludes_config_tools() {
        assert!(!CONFIGURABLE_TOOL_NAMES.contains(&"get_config"));
        assert!(!CONFIGURABLE_TOOL_NAMES.contains(&"set_config"));
    }

    #[test]
    fn configurable_tool_names_includes_core_tools() {
        assert!(CONFIGURABLE_TOOL_NAMES.contains(&"code_health_review"));
        assert!(CONFIGURABLE_TOOL_NAMES.contains(&"code_health_score"));
        assert!(CONFIGURABLE_TOOL_NAMES.contains(&"analyze_change_set"));
    }

    #[test]
    fn find_option_enabled_tools() {
        let opt = find_option("enabled_tools");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().env_var, "CS_ENABLED_TOOLS");
    }

    #[test]
    fn find_option_enabled_tools_by_alias() {
        let opt = find_option("tools");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().key, "enabled_tools");
    }

    #[test]
    fn find_option_tracking_environment_by_env_var() {
        let opt = find_option("CS_ENVIRONMENT");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().key, "tracking_environment");
    }

    #[test]
    fn find_option_tracking_environment_by_alias() {
        let opt = find_option("environment");
        assert!(opt.is_some());
        assert_eq!(opt.unwrap().key, "tracking_environment");
    }
}

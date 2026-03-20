/// Configuration system — mirrors Python's `config.py`.
///
/// Manages `~/.config/codehealth-mcp/config.json` with 7 config options,
/// an instance ID (UUID4) for analytics, and environment variable integration.
/// Environment variables from the MCP client always take precedence.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ConfigError;

/// Config directory override for testing.
static CONFIG_DIR_OVERRIDE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Env vars that were already set by the MCP client before startup.
/// Captured once by `snapshot_client_env_vars()` so we can distinguish
/// client-provided values from those applied from the config file.
static CLIENT_ENV_VARS: OnceLock<HashSet<String>> = OnceLock::new();

/// One configurable option with metadata.
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

/// All known configuration options.
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
        key: "ace_access_token",
        env_var: "CS_ACE_ACCESS_TOKEN",
        description: "ACE (Auto Code Enhancement) access token for refactoring",
        sensitive: true,
        hidden: false,
        api_only: false,
        aliases: &["ace_token"],
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
        key: "ca_bundle",
        env_var: "REQUESTS_CA_BUNDLE",
        description: "Path to custom CA certificate bundle (PEM)",
        sensitive: false,
        hidden: false,
        api_only: false,
        aliases: &["ssl_cert", "cert"],
        docs_url: "https://codescene.io/docs/integrations/mcp.html#configuration",
    },
];

/// Persistent config data stored in `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigData {
    #[serde(default)]
    pub instance_id: Option<String>,

    #[serde(flatten)]
    pub values: HashMap<String, String>,
}

/// Resolve the config directory path.
pub fn config_dir() -> PathBuf {
    if let Some(Some(dir)) = CONFIG_DIR_OVERRIDE.get() {
        return dir.clone();
    }

    if let Ok(dir) = std::env::var("CS_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("codehealth-mcp")
}

/// Read config from disk, creating defaults if missing.
pub fn load() -> Result<ConfigData, ConfigError> {
    let path = config_dir().join("config.json");
    if !path.exists() {
        let data = ConfigData {
            instance_id: Some(Uuid::new_v4().to_string()),
            values: HashMap::new(),
        };
        save(&data)?;
        return Ok(data);
    }

    let content = std::fs::read_to_string(&path)?;
    let mut data: ConfigData = serde_json::from_str(&content)?;

    if data.instance_id.is_none() {
        data.instance_id = Some(Uuid::new_v4().to_string());
        save(&data)?;
    }

    Ok(data)
}

/// Write config to disk.
pub fn save(data: &ConfigData) -> Result<(), ConfigError> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.json");
    let content = serde_json::to_string_pretty(data)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Record which config-related env vars are already set by the MCP client.
/// Call this once at startup, *before* `apply_to_env`.
pub fn snapshot_client_env_vars() {
    let mut set = HashSet::new();
    for option in OPTIONS {
        if std::env::var(option.env_var).is_ok() {
            set.insert(option.env_var.to_string());
        }
    }
    let _ = CLIENT_ENV_VARS.set(set);
}

/// Returns `true` if this env var was set by the MCP client at startup.
pub fn is_client_env_var(env_var: &str) -> bool {
    CLIENT_ENV_VARS
        .get()
        .map_or(false, |s| s.contains(env_var))
}

/// Read a non-empty environment variable.
fn non_empty_env(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|v| !v.is_empty())
}

/// Get the effective value for a config option.
/// Priority: client env var > config file value > applied env var.
pub fn get_effective(option: &ConfigOption, data: &ConfigData) -> Option<String> {
    if is_client_env_var(option.env_var) {
        if let Some(val) = non_empty_env(option.env_var) {
            return Some(val);
        }
    }

    let file_val = data.values.get(option.key).filter(|v| !v.is_empty());
    if let Some(val) = file_val {
        return Some(val.clone());
    }

    non_empty_env(option.env_var)
}

/// Where the effective value comes from.
pub fn value_source(option: &ConfigOption, data: &ConfigData) -> &'static str {
    if is_client_env_var(option.env_var) && non_empty_env(option.env_var).is_some() {
        return "environment";
    }

    if data.values.contains_key(option.key) {
        return "config_file";
    }

    if non_empty_env(option.env_var).is_some() {
        return "config_file";
    }

    "not set"
}

/// Look up a config option by key or alias.
pub fn find_option(key: &str) -> Option<&'static ConfigOption> {
    OPTIONS.iter().find(|o| {
        o.key == key || o.env_var == key || o.aliases.contains(&key)
    })
}

/// Apply config values to environment variables at startup.
/// Only sets env vars that are not already set by the MCP client.
pub fn apply_to_env(data: &ConfigData) {
    for option in OPTIONS {
        if std::env::var(option.env_var).is_ok() {
            continue;
        }
        let val = data.values.get(option.key).filter(|v| !v.is_empty());
        if let Some(val) = val {
            std::env::set_var(option.env_var, val);
        }
    }
}

/// Get the instance ID, creating one if needed.
pub fn instance_id(data: &ConfigData) -> String {
    data.instance_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// Get the config file path for display.
#[allow(dead_code)]
pub fn config_file_path() -> PathBuf {
    config_dir().join("config.json")
}

/// Returns `true` if the key is a recognized config option.
#[allow(dead_code)]
pub fn is_valid_key(key: &str) -> bool {
    find_option(key).is_some()
}

/// Mask sensitive values for display.
/// Shows `...` followed by the last 6 characters, matching Python's format.
const SENSITIVE_TAIL_LENGTH: usize = 6;

pub fn mask_if_sensitive(option: &ConfigOption, value: &str) -> String {
    if option.sensitive && !value.is_empty() {
        if value.len() <= SENSITIVE_TAIL_LENGTH {
            return "***".to_string();
        }
        format!("...{}", &value[value.len() - SENSITIVE_TAIL_LENGTH..])
    } else {
        value.to_string()
    }
}

/// Allow tests to override the config directory.
#[cfg(test)]
pub fn set_config_dir_override(dir: Option<PathBuf>) {
    let _ = CONFIG_DIR_OVERRIDE.set(dir);
}

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(test)]
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ConfigError;

/// Env vars that were already set by the MCP client before startup.
/// Captured once by `snapshot_client_env_vars()` so we can distinguish
/// client-provided values from those applied from the config file.
#[cfg(not(test))]
static CLIENT_ENV_VARS: OnceLock<HashSet<String>> = OnceLock::new();

#[cfg(test)]
static CLIENT_ENV_VARS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[cfg(test)]
static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    TEST_ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigData {
    #[serde(default)]
    pub instance_id: Option<String>,

    #[serde(flatten)]
    pub values: HashMap<String, String>,
}

pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CS_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("codehealth-mcp")
}

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

    #[cfg(not(test))]
    let _ = CLIENT_ENV_VARS.set(set);

    #[cfg(test)]
    {
        let store = CLIENT_ENV_VARS.get_or_init(|| Mutex::new(HashSet::new()));
        let mut guard = store.lock().unwrap_or_else(|e| e.into_inner());
        guard.extend(set);
    }
}

pub fn is_client_env_var(env_var: &str) -> bool {
    #[cfg(not(test))]
    {
        return CLIENT_ENV_VARS
            .get()
            .map_or(false, |s| s.contains(env_var));
    }

    #[cfg(test)]
    {
        return CLIENT_ENV_VARS
            .get()
            .and_then(|s| s.lock().ok().map(|g| g.contains(env_var)))
            .unwrap_or(false);
    }
}

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

pub fn instance_id(data: &ConfigData) -> String {
    data.instance_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

#[allow(dead_code)]
pub fn config_file_path() -> PathBuf {
    config_dir().join("config.json")
}

#[allow(dead_code)]
pub fn is_valid_key(key: &str) -> bool {
    find_option(key).is_some()
}

/// Mask sensitive values for display.
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
    fn mask_sensitive_long_value() {
        let opt = find_option("access_token").unwrap();
        let masked = mask_if_sensitive(opt, "my-super-secret-token-value");
        assert_eq!(masked, "...-value");
    }

    #[test]
    fn mask_sensitive_short_value() {
        let opt = find_option("access_token").unwrap();
        let masked = mask_if_sensitive(opt, "short");
        assert_eq!(masked, "***");
    }

    #[test]
    fn mask_sensitive_empty_value() {
        let opt = find_option("access_token").unwrap();
        let masked = mask_if_sensitive(opt, "");
        assert_eq!(masked, "");
    }

    #[test]
    fn mask_non_sensitive_passes_through() {
        let opt = find_option("onprem_url").unwrap();
        let masked = mask_if_sensitive(opt, "https://example.com");
        assert_eq!(masked, "https://example.com");
    }

    #[test]
    fn config_data_default() {
        let data = ConfigData::default();
        assert!(data.instance_id.is_none());
        assert!(data.values.is_empty());
    }

    #[test]
    fn instance_id_uses_existing() {
        let data = ConfigData {
            instance_id: Some("test-id-123".to_string()),
            values: HashMap::new(),
        };
        assert_eq!(instance_id(&data), "test-id-123");
    }

    #[test]
    fn instance_id_generates_when_missing() {
        let data = ConfigData {
            instance_id: None,
            values: HashMap::new(),
        };
        let id = instance_id(&data);
        assert!(!id.is_empty());
        assert_eq!(id.len(), 36);
    }

    #[test]
    fn get_effective_from_config_file() {
        let opt = find_option("onprem_url").unwrap();
        let mut data = ConfigData::default();
        data.values
            .insert("onprem_url".to_string(), "https://my-server.com".to_string());
        let val = get_effective(opt, &data);
        assert_eq!(val, Some("https://my-server.com".to_string()));
    }

    #[test]
    fn get_effective_ignores_empty_config_value() {
        let opt = find_option("onprem_url").unwrap();
        let mut data = ConfigData::default();
        data.values.insert("onprem_url".to_string(), "".to_string());
        let val = get_effective(opt, &data);
        assert!(val.is_none() || val.as_deref() != Some(""));
    }

    #[test]
    fn value_source_not_set() {
        let _lock = lock_test_env();
        let opt = find_option("ca_bundle").unwrap();
        let data = ConfigData::default();
        std::env::remove_var("REQUESTS_CA_BUNDLE");
        let source = value_source(opt, &data);
        assert_eq!(source, "not set");
    }

    #[test]
    fn value_source_from_config_file() {
        let _lock = lock_test_env();
        let opt = find_option("ca_bundle").unwrap();
        let mut data = ConfigData::default();
        data.values
            .insert("ca_bundle".to_string(), "/path/to/cert.pem".to_string());
        std::env::remove_var("REQUESTS_CA_BUNDLE");
        let source = value_source(opt, &data);
        assert_eq!(source, "config_file");
    }

    #[test]
    fn config_file_path_ends_with_config_json() {
        let path = config_file_path();
        assert!(path.to_string_lossy().ends_with("config.json"));
    }

    #[test]
    fn save_and_read_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("config.json");

        let mut data = ConfigData {
            instance_id: Some("round-trip-test".to_string()),
            values: HashMap::new(),
        };
        data.values
            .insert("onprem_url".to_string(), "https://test.com".to_string());

        let content = serde_json::to_string_pretty(&data).unwrap();
        std::fs::write(&json_path, &content).unwrap();

        let read_back = std::fs::read_to_string(&json_path).unwrap();
        let loaded: ConfigData = serde_json::from_str(&read_back).unwrap();

        assert_eq!(loaded.instance_id.as_deref(), Some("round-trip-test"));
        assert_eq!(
            loaded.values.get("onprem_url").map(|s| s.as_str()),
            Some("https://test.com")
        );
    }

    #[test]
    fn config_data_serde_round_trip() {
        let data = ConfigData::default();
        assert!(data.instance_id.is_none());

        let json = r#"{}"#;
        let loaded: ConfigData = serde_json::from_str(json).unwrap();
        assert!(loaded.instance_id.is_none());
        assert!(loaded.values.is_empty());

        let json = r#"{"instance_id":"keep-me"}"#;
        let loaded: ConfigData = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.instance_id.as_deref(), Some("keep-me"));

        let json = r#"{"instance_id":"id","onprem_url":"https://x.com"}"#;
        let loaded: ConfigData = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.values.get("onprem_url").map(|s| s.as_str()), Some("https://x.com"));
    }

    #[test]
    fn apply_to_env_sets_unset_vars() {
        let _lock = lock_test_env();
        let mut data = ConfigData::default();
        data.values
            .insert("onprem_url".to_string(), "https://apply-test.com".to_string());
        std::env::remove_var("CS_ONPREM_URL");

        apply_to_env(&data);

        let val = std::env::var("CS_ONPREM_URL").unwrap_or_default();
        assert_eq!(val, "https://apply-test.com");
        std::env::remove_var("CS_ONPREM_URL");
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
    fn config_dir_uses_env_override() {
        let _lock = lock_test_env();
        std::env::set_var("CS_CONFIG_DIR", "/tmp/test-config-dir");
        let dir = config_dir();
        assert_eq!(dir.to_string_lossy(), "/tmp/test-config-dir");
        std::env::remove_var("CS_CONFIG_DIR");
    }

    // ---- load / save via CS_CONFIG_DIR ----

    /// Helper: run a closure with CS_CONFIG_DIR pointing to a fresh temp dir,
    /// holding the env lock to prevent parallel test interference.
    fn with_temp_config_dir<F, R>(f: F) -> R
    where
        F: FnOnce(&std::path::Path) -> R,
    {
        let _lock = lock_test_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CS_CONFIG_DIR", dir.path().as_os_str());
        let result = f(dir.path());
        std::env::remove_var("CS_CONFIG_DIR");
        result
    }

    #[test]
    fn save_creates_directory_and_file() {
        with_temp_config_dir(|dir| {
            let data = ConfigData {
                instance_id: Some("save-test".to_string()),
                values: HashMap::new(),
            };
            save(&data).unwrap();
            assert!(dir.join("config.json").exists());
        });
    }

    #[test]
    fn load_creates_default_when_missing() {
        with_temp_config_dir(|dir| {
            let data = load().unwrap();
            assert!(data.instance_id.is_some());
            assert_eq!(data.instance_id.as_ref().unwrap().len(), 36);
            assert!(dir.join("config.json").exists());
        });
    }

    #[test]
    fn load_reads_existing_config() {
        with_temp_config_dir(|dir| {
            let json = r#"{"instance_id":"existing-id","onprem_url":"https://x.com"}"#;
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("config.json"), json).unwrap();

            let data = load().unwrap();
            assert_eq!(data.instance_id.as_deref(), Some("existing-id"));
            assert_eq!(
                data.values.get("onprem_url").map(|s| s.as_str()),
                Some("https://x.com")
            );
        });
    }

    #[test]
    fn load_adds_instance_id_when_missing() {
        with_temp_config_dir(|dir| {
            let json = r#"{"onprem_url":"https://x.com"}"#;
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("config.json"), json).unwrap();

            let data = load().unwrap();
            assert!(data.instance_id.is_some());
            assert_eq!(data.instance_id.as_ref().unwrap().len(), 36);
        });
    }

    #[test]
    fn save_then_load_round_trip() {
        with_temp_config_dir(|_dir| {
            let mut original = ConfigData {
                instance_id: Some("round-trip-id".to_string()),
                values: HashMap::new(),
            };
            original
                .values
                .insert("access_token".to_string(), "secret123".to_string());
            save(&original).unwrap();

            let loaded = load().unwrap();
            assert_eq!(loaded.instance_id.as_deref(), Some("round-trip-id"));
            assert_eq!(
                loaded.values.get("access_token").map(|s| s.as_str()),
                Some("secret123")
            );
        });
    }

    #[test]
    fn apply_to_env_skips_already_set_vars() {
        let _lock = lock_test_env();
        std::env::set_var("CS_ONPREM_URL", "already-set");
        let mut data = ConfigData::default();
        data.values
            .insert("onprem_url".to_string(), "from-config".to_string());

        apply_to_env(&data);

        let val = std::env::var("CS_ONPREM_URL").unwrap();
        assert_eq!(val, "already-set");
        std::env::remove_var("CS_ONPREM_URL");
    }

    #[test]
    fn apply_to_env_skips_empty_values() {
        let _lock = lock_test_env();
        std::env::remove_var("CS_ONPREM_URL");
        let mut data = ConfigData::default();
        data.values
            .insert("onprem_url".to_string(), "".to_string());

        apply_to_env(&data);

        assert!(std::env::var("CS_ONPREM_URL").is_err());
    }

    #[test]
    fn value_source_env_applied_returns_config_file() {
        let _lock = lock_test_env();
        let opt = find_option("ca_bundle").unwrap();
        let data = ConfigData::default();
        std::env::set_var("REQUESTS_CA_BUNDLE", "/some/cert.pem");
        let source = value_source(opt, &data);
        assert_eq!(source, "config_file");
        std::env::remove_var("REQUESTS_CA_BUNDLE");
    }

    // ---- snapshot_client_env_vars + client env var paths ----
    // CLIENT_ENV_VARS is a OnceLock, so can only be set once per process.
    // This single test exercises snapshot_client_env_vars(), is_client_env_var(),
    // and the client-env-var branches of get_effective() and value_source().

    #[test]
    fn snapshot_and_client_env_var_paths() {
        let _lock = lock_test_env();
        // Set an env var BEFORE snapshotting so it's captured as a client var.
        // Use disable_tracking since it's hidden and unlikely to be set by other tests.
        std::env::set_var("CS_DISABLE_TRACKING", "snapshot-test");

        snapshot_client_env_vars();

        // is_client_env_var should return true for the captured var
        assert!(is_client_env_var("CS_DISABLE_TRACKING"));
        // and false for one that wasn't set
        assert!(!is_client_env_var("CS_ONPREM_URL"));

        // get_effective should return the env var value for a client var
        let opt = find_option("disable_tracking").unwrap();
        let data = ConfigData::default();
        let val = get_effective(opt, &data);
        assert_eq!(val, Some("snapshot-test".to_string()));

        // value_source should return "environment" for a client var
        let source = value_source(opt, &data);
        assert_eq!(source, "environment");

        // Cleanup
        std::env::remove_var("CS_DISABLE_TRACKING");
    }
}

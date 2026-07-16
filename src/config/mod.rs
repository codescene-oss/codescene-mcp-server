pub mod options;
pub mod validation;

pub use options::*;
pub use validation::*;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(test)]
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
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

// ---------------------------------------------------------------------------
// RwLock-guarded access to config-backed environment variables.
//
// All reads and writes of config-backed env vars (the vars listed in OPTIONS)
// should go through `read_env` / `write_env` / `write_env_multi` to ensure
// consistent concurrent access and atomic persistence to the config file.
// ---------------------------------------------------------------------------

/// Global RwLock guarding all config-backed environment variable access.
/// Readers can proceed concurrently; writers get exclusive access and
/// persist changes to both process env and the config file atomically.
/// Uses tokio::sync::RwLock so that the write guard can be held across
/// await points (e.g. CLI subprocess calls).
static CONFIG_ENV_LOCK: OnceLock<RwLock<()>> = OnceLock::new();

fn config_env_lock() -> &'static RwLock<()> {
    CONFIG_ENV_LOCK.get_or_init(|| RwLock::new(()))
}

/// Synchronous read of a config-backed env var. Uses `try_read` to avoid
/// blocking; returns `None` if the lock is contended or the var is unset.
pub fn try_read_env(env_var: &str) -> Option<String> {
    let _guard = config_env_lock().try_read().ok()?;
    std::env::var(env_var).ok().filter(|v| !v.is_empty())
}

/// Synchronous read of multiple config-backed env vars. Uses `try_read`.
/// Returns `None` if the lock is contended.
pub fn try_read_env_multi(env_vars: &[&str]) -> Option<Vec<Option<String>>> {
    let _guard = config_env_lock().try_read().ok()?;
    Some(
        env_vars
            .iter()
            .map(|var| std::env::var(var).ok().filter(|v| !v.is_empty()))
            .collect(),
    )
}

/// Write a single config-backed env var under the exclusive write lock.
/// Also persists the new value to the config file.
/// Pass an empty value to remove the key.
pub async fn write_env(key: &str, value: &str) -> Result<(), ConfigError> {
    write_env_multi(&[(key, value)]).await
}

/// Write multiple config-backed env vars atomically under the exclusive write lock.
/// Also persists all new values to the config file.
/// Pass an empty value for a key to remove it.
pub async fn write_env_multi(entries: &[(&str, &str)]) -> Result<(), ConfigError> {
    let _guard = config_env_lock().write().await;
    write_env_multi_inner(entries)
}

/// Acquire the exclusive config write lock. While held, no other thread can
/// read or write config-backed env vars. Use this when you need to hold the
/// lock across an external operation (e.g. a CLI call) and then write the
/// result atomically.
pub async fn acquire_write_lock() -> ConfigEnvWriteGuard {
    let guard = config_env_lock().write().await;
    ConfigEnvWriteGuard { _guard: guard }
}

/// RAII guard for the config env write lock. While held, provides methods
/// to read and write env vars without re-acquiring the lock.
pub struct ConfigEnvWriteGuard {
    _guard: tokio::sync::RwLockWriteGuard<'static, ()>,
}

impl ConfigEnvWriteGuard {
    /// Read an env var while holding the write lock (for double-check patterns).
    pub fn read_env(&self, env_var: &str) -> Option<String> {
        std::env::var(env_var).ok().filter(|v| !v.is_empty())
    }

    /// Write multiple entries to env + config file while holding the lock.
    pub fn write_env_multi(&self, entries: &[(&str, &str)]) -> Result<(), ConfigError> {
        write_env_multi_inner(entries)
    }
}

/// Shared implementation for writing env vars + persisting to config.
/// Caller must already hold the write lock.
fn write_env_multi_inner(entries: &[(&str, &str)]) -> Result<(), ConfigError> {
    let mut data = load().unwrap_or_default();

    for &(key, value) in entries {
        let option = match find_option(key) {
            Some(o) => o,
            None => continue,
        };

        if value.is_empty() {
            data.values.remove(option.key);
            if !is_client_env_var(option.env_var) {
                std::env::remove_var(option.env_var);
            }
        } else {
            data.values
                .insert(option.key.to_string(), value.to_string());
            if !is_client_env_var(option.env_var) {
                std::env::set_var(option.env_var, value);
            }
        }
    }

    save(&data)
}

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
    restrict_path_permissions(&dir);
    let path = dir.join("config.json");
    let content = serde_json::to_string_pretty(data)?;
    // Atomic write: temp file + rename to avoid truncation on crash
    let temp = tempfile::NamedTempFile::new_in(&dir)?;
    std::fs::write(temp.path(), &content)?;
    restrict_path_permissions(temp.path());
    temp.persist(&path)
        .map_err(|e| ConfigError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
    Ok(())
}

/// Restrict path permissions to owner-only (0700 for dirs, 0600 for files) on Unix.
fn restrict_path_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if path.is_dir() { 0o700 } else { 0o600 };
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
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
        return CLIENT_ENV_VARS.get().map_or(false, |s| s.contains(env_var));
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

/// Apply config values to environment variables at startup.
/// Only sets env vars that are not already set by the MCP client.
pub fn apply_to_env(data: &ConfigData) {
    for option in OPTIONS {
        if std::env::var(option.env_var).is_ok() {
            continue;
        }
        let Some(val) = data.values.get(option.key).filter(|v| !v.is_empty()) else {
            continue;
        };
        if let Err(e) = validate_https_url(option.key, val) {
            tracing::warn!("{e} — skipping config value");
            continue;
        }
        std::env::set_var(option.env_var, val);
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

/// Parse the `enabled_tools` config value into a set of tool names.
/// Returns `None` when unset or empty (meaning all tools are enabled).
pub fn enabled_tools(data: &ConfigData) -> Option<HashSet<String>> {
    let option = find_option("enabled_tools")?;
    let value = get_effective(option, data)?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

/// Returns the number of days to retain log files. Defaults to 7.
/// Returns 0 if file logging is disabled.
pub fn log_retention_days(data: &ConfigData) -> u32 {
    let option = match find_option("log_retention_days") {
        Some(o) => o,
        None => return 7,
    };
    match get_effective(option, data) {
        Some(v) => v.trim().parse().unwrap_or(7),
        None => 7,
    }
}

/// Returns the directory for log files.
pub fn log_dir() -> PathBuf {
    config_dir().join("logs")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        data.values.insert(
            "onprem_url".to_string(),
            "https://my-server.com".to_string(),
        );
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
        assert_eq!(
            loaded.values.get("onprem_url").map(|s| s.as_str()),
            Some("https://x.com")
        );
    }

    #[test]
    fn apply_to_env_sets_unset_vars() {
        let _lock = lock_test_env();
        let mut data = ConfigData::default();
        data.values.insert(
            "onprem_url".to_string(),
            "https://apply-test.com".to_string(),
        );
        std::env::remove_var("CS_ONPREM_URL");

        apply_to_env(&data);

        let val = std::env::var("CS_ONPREM_URL").unwrap_or_default();
        assert_eq!(val, "https://apply-test.com");
        std::env::remove_var("CS_ONPREM_URL");
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
    fn save_persist_failure_returns_io_error() {
        with_temp_config_dir(|dir| {
            // Create a directory where config.json should be, so persist (rename) fails
            let blocker = dir.join("config.json");
            std::fs::create_dir_all(&blocker).unwrap();
            std::fs::write(blocker.join("block"), "x").unwrap(); // non-empty dir can't be replaced

            let data = ConfigData {
                instance_id: Some("persist-fail".to_string()),
                values: HashMap::new(),
            };
            let err = save(&data).unwrap_err();
            assert!(
                matches!(err, ConfigError::Io(_)),
                "expected ConfigError::Io, got: {err:?}"
            );
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

    fn apply_to_env_value(config_key: &str, value: &str) {
        let mut data = ConfigData::default();
        data.values.insert(config_key.to_string(), value.to_string());
        apply_to_env(&data);
    }

    fn enabled_tools_from(value: &str) -> Option<HashSet<String>> {
        let mut data = ConfigData::default();
        data.values
            .insert("enabled_tools".to_string(), value.to_string());
        enabled_tools(&data)
    }

    #[test]
    fn apply_to_env_skips_already_set_vars() {
        let _lock = lock_test_env();
        std::env::set_var("CS_ONPREM_URL", "already-set");
        apply_to_env_value("onprem_url", "from-config");

        let val = std::env::var("CS_ONPREM_URL").unwrap();
        assert_eq!(val, "already-set");
        std::env::remove_var("CS_ONPREM_URL");
    }

    #[test]
    fn apply_to_env_skips_empty_values() {
        let _lock = lock_test_env();
        std::env::remove_var("CS_ONPREM_URL");
        apply_to_env_value("onprem_url", "");

        assert!(std::env::var("CS_ONPREM_URL").is_err());
    }

    #[test]
    fn apply_to_env_sets_oauth_client() {
        let _lock = lock_test_env();
        std::env::remove_var("CS_OAUTH_CLIENT");
        apply_to_env_value("oauth_client", "mcp");

        let val = std::env::var("CS_OAUTH_CLIENT").unwrap_or_default();
        assert_eq!(val, "mcp");
        std::env::remove_var("CS_OAUTH_CLIENT");
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

    #[test]
    fn snapshot_and_client_env_var_paths() {
        let _lock = lock_test_env();
        std::env::set_var("CS_DISABLE_TRACKING", "snapshot-test");

        snapshot_client_env_vars();

        assert!(is_client_env_var("CS_DISABLE_TRACKING"));
        assert!(!is_client_env_var("CS_ONPREM_URL"));

        let opt = find_option("disable_tracking").unwrap();
        let data = ConfigData::default();
        let val = get_effective(opt, &data);
        assert_eq!(val, Some("snapshot-test".to_string()));

        let source = value_source(opt, &data);
        assert_eq!(source, "environment");

        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    // ---- enabled_tools ----

    #[test]
    fn enabled_tools_returns_none_when_unset() {
        let data = ConfigData::default();
        assert!(enabled_tools(&data).is_none());
    }

    #[test]
    fn enabled_tools_returns_none_when_empty() {
        assert!(enabled_tools_from("").is_none());
    }

    #[test]
    fn enabled_tools_returns_none_when_whitespace_only() {
        assert!(enabled_tools_from("  ").is_none());
    }

    #[test]
    fn enabled_tools_parses_single_tool() {
        let result = enabled_tools_from("code_health_review").unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains("code_health_review"));
    }

    #[test]
    fn enabled_tools_parses_multiple_tools() {
        let result =
            enabled_tools_from("code_health_review,code_health_score,analyze_change_set")
                .unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains("code_health_review"));
        assert!(result.contains("code_health_score"));
        assert!(result.contains("analyze_change_set"));
    }

    #[test]
    fn enabled_tools_trims_whitespace() {
        let result = enabled_tools_from(" code_health_review , code_health_score ").unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains("code_health_review"));
        assert!(result.contains("code_health_score"));
    }

    #[test]
    fn enabled_tools_ignores_empty_segments() {
        let result = enabled_tools_from("code_health_review,,code_health_score,").unwrap();
        assert_eq!(result.len(), 2);
    }
}

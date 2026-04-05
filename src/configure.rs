use serde_json::json;

use crate::config::{self, ConfigData};

pub fn get_single(key: &str, data: &ConfigData, is_standalone: bool) -> String {
    let option = match config::find_option(key) {
        Some(o) => o,
        None => return unknown_key_error(key),
    };

    if option.api_only && is_standalone {
        return api_only_error(key, is_standalone);
    }

    let mut result = format_option_json(option, data);
    if option.key == "enabled_tools" {
        result["available_tools"] = json!(config::CONFIGURABLE_TOOL_NAMES);
    }
    serde_json::to_string(&result).unwrap_or_default()
}

pub fn get_all(data: &ConfigData, is_standalone: bool) -> String {
    let options: Vec<serde_json::Value> = config::OPTIONS
        .iter()
        .filter(|o| !o.hidden && !(o.api_only && is_standalone))
        .map(|o| format_option_json(o, data))
        .collect();

    let result = json!({
        "config_dir": config::config_dir().to_string_lossy(),
        "options": options,
    });
    serde_json::to_string(&result).unwrap_or_default()
}

pub fn set_value(key: &str, value: &str) -> String {
    let option = match config::find_option(key) {
        Some(o) => o,
        None => return unknown_key_error(key),
    };

    let value = if option.key.ends_with("_token") {
        value.trim()
    } else {
        value
    };

    let mut data = config::load().unwrap_or_default();

    if value.is_empty() {
        delete_key(option, &mut data)
    } else {
        save_key(option, value, &mut data)
    }
}

fn format_option_json(option: &config::ConfigOption, data: &ConfigData) -> serde_json::Value {
    let effective = config::get_effective(option, data);
    let source = config::value_source(option, data);
    let display_value: serde_json::Value = match &effective {
        Some(v) => json!(config::mask_if_sensitive(option, v)),
        None => serde_json::Value::Null,
    };

    let mut entry = json!({
        "key": option.key,
        "env_var": option.env_var,
        "value": display_value,
        "source": source,
        "description": option.description,
    });
    if !option.aliases.is_empty() {
        entry["aliases"] = json!(option.aliases);
    }
    attach_docs_url(&mut entry, option);
    entry
}

fn unknown_key_error(key: &str) -> String {
    let valid: Vec<&str> = config::OPTIONS.iter().map(|o| o.key).collect();
    serde_json::to_string(&json!({
        "error": format!("Unknown configuration key: '{key}'"),
        "valid_keys": valid,
    }))
    .unwrap_or_default()
}

fn api_only_error(key: &str, is_standalone: bool) -> String {
    let valid: Vec<&str> = config::OPTIONS
        .iter()
        .filter(|o| !o.api_only || !is_standalone)
        .map(|o| o.key)
        .collect();
    serde_json::to_string(&json!({
        "error": format!("Configuration key '{key}' is not available with a standalone license."),
        "valid_keys": valid,
    }))
    .unwrap_or_default()
}

fn delete_key(option: &config::ConfigOption, data: &mut ConfigData) -> String {
    data.values.remove(option.key);
    if let Err(e) = config::save(data) {
        return format!("Error saving config: {e}");
    }
    if !config::is_client_env_var(option.env_var) {
        std::env::remove_var(option.env_var);
    }
    let mut result = json!({ "status": "removed", "key": option.key });
    attach_docs_url(&mut result, option);
    serde_json::to_string(&result).unwrap_or_default()
}

fn save_key(option: &config::ConfigOption, value: &str, data: &mut ConfigData) -> String {
    data.values
        .insert(option.key.to_string(), value.to_string());
    if let Err(e) = config::save(data) {
        return format!("Error saving config: {e}");
    }
    if !config::is_client_env_var(option.env_var) {
        std::env::set_var(option.env_var, value);
    }

    let mut result = json!({
        "status": "saved",
        "key": option.key,
        "config_dir": config::config_dir().to_string_lossy(),
    });
    if let Some(warning) = env_override_warning(option) {
        result["warning"] = json!(warning);
    }
    if let Some(restart) = restart_warning(option) {
        result["restart_required"] = json!(restart);
    }
    if let Some(tool_warning) = unknown_tool_names_warning(option, value) {
        result["tool_name_warning"] = json!(tool_warning);
    }
    attach_docs_url(&mut result, option);
    serde_json::to_string(&result).unwrap_or_default()
}

fn env_override_warning(option: &config::ConfigOption) -> Option<String> {
    if !config::is_client_env_var(option.env_var) {
        return None;
    }
    Some(format!(
        "The environment variable {} is also set (via your MCP client config). \
         The environment value takes precedence over the config file at runtime.",
        option.env_var
    ))
}

fn restart_warning(option: &config::ConfigOption) -> Option<&'static str> {
    match option.key {
        "access_token" => Some(
            "Changing the access token may affect which tools are \
             available (API vs standalone mode). A server restart is required \
             for tool registration changes to take effect.",
        ),
        "enabled_tools" => {
            Some("A server restart is required for tool registration changes to take effect.")
        }
        _ => None,
    }
}

fn unknown_tool_names_warning(option: &config::ConfigOption, value: &str) -> Option<String> {
    if option.key != "enabled_tools" || value.is_empty() {
        return None;
    }
    let unknown: Vec<&str> = value
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !config::CONFIGURABLE_TOOL_NAMES.contains(s))
        .collect();
    if unknown.is_empty() {
        return None;
    }
    Some(format!(
        "Unrecognized tool name(s): {}. Valid names: {}",
        unknown.join(", "),
        config::CONFIGURABLE_TOOL_NAMES.join(", "),
    ))
}

fn attach_docs_url(result: &mut serde_json::Value, option: &config::ConfigOption) {
    if !option.docs_url.is_empty() {
        result["docs_url"] = json!(option.docs_url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use std::collections::HashMap;

    fn empty_config() -> ConfigData {
        ConfigData {
            instance_id: Some("test-id".to_string()),
            values: HashMap::new(),
        }
    }

    // ---- get_single ----

    #[test]
    fn get_single_unknown_key_returns_error() {
        let data = empty_config();
        let result = get_single("nonexistent_key", &data, false);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("error").is_some());
        assert!(parsed["error"].as_str().unwrap().contains("Unknown"));
        assert!(parsed.get("valid_keys").is_some());
    }

    #[test]
    fn get_single_valid_key_returns_json() {
        let data = empty_config();
        let result = get_single("access_token", &data, false);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["key"], json!("access_token"));
        assert!(parsed.get("env_var").is_some());
        assert!(parsed.get("description").is_some());
    }

    #[test]
    fn get_single_api_only_key_in_standalone_returns_error() {
        let data = empty_config();
        // "onprem_url" is api_only
        let result = get_single("onprem_url", &data, true);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("error").is_some());
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("not available with a standalone"));
    }

    #[test]
    fn get_single_api_only_key_in_api_mode_works() {
        let data = empty_config();
        let result = get_single("onprem_url", &data, false);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["key"], json!("onprem_url"));
    }

    #[test]
    fn get_single_with_value_in_config() {
        let mut data = empty_config();
        data.values
            .insert("onprem_url".to_string(), "https://test.com".to_string());
        let result = get_single("onprem_url", &data, false);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["key"], json!("onprem_url"));
    }

    #[test]
    fn get_single_sensitive_value_is_masked() {
        let mut data = empty_config();
        data.values
            .insert("access_token".to_string(), "my-secret-token".to_string());
        let result = get_single("access_token", &data, false);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        // The value should be masked
        let value = parsed["value"].as_str().unwrap_or("");
        assert!(value.contains("***") || value != "my-secret-token");
    }

    // ---- get_all ----

    #[test]
    fn get_all_returns_config_dir_and_options() {
        let data = empty_config();
        let result = get_all(&data, false);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("config_dir").is_some());
        assert!(parsed.get("options").is_some());
        let options = parsed["options"].as_array().unwrap();
        assert!(!options.is_empty());
    }

    #[test]
    fn get_all_standalone_excludes_api_only() {
        let data = empty_config();
        let result_full = get_all(&data, false);
        let result_standalone = get_all(&data, true);
        let parsed_full: serde_json::Value = serde_json::from_str(&result_full).unwrap();
        let parsed_standalone: serde_json::Value =
            serde_json::from_str(&result_standalone).unwrap();
        let full_count = parsed_full["options"].as_array().unwrap().len();
        let standalone_count = parsed_standalone["options"].as_array().unwrap().len();
        assert!(standalone_count <= full_count);
    }

    // ---- format_option_json ----

    #[test]
    fn format_option_json_includes_docs_url() {
        let option = &config::OPTIONS[0]; // access_token
        let data = empty_config();
        let result = format_option_json(option, &data);
        assert!(result.get("docs_url").is_some());
    }

    #[test]
    fn format_option_json_includes_aliases() {
        let option = config::find_option("access_token").unwrap();
        let data = empty_config();
        let result = format_option_json(option, &data);
        if !option.aliases.is_empty() {
            assert!(result.get("aliases").is_some());
        }
    }

    // ---- unknown_key_error ----

    #[test]
    fn unknown_key_error_lists_valid_keys() {
        let result = unknown_key_error("bogus");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let valid = parsed["valid_keys"].as_array().unwrap();
        assert!(!valid.is_empty());
        assert!(valid.iter().any(|k| k.as_str() == Some("access_token")));
    }

    // ---- env_override_warning ----

    #[test]
    fn env_override_warning_returns_none_when_not_client_var() {
        let option = config::find_option("access_token").unwrap();
        // By default in tests, CS_ACCESS_TOKEN is not in the CLIENT_ENV_VARS snapshot
        let result = env_override_warning(option);
        assert!(result.is_none());
    }

    #[test]
    fn env_override_warning_returns_some_for_client_var() {
        let _lock = config::lock_test_env();
        // Ensure the var is set before snapshotting.
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        config::snapshot_client_env_vars();
        let option = config::find_option("disable_tracking").unwrap();
        let result = env_override_warning(option);
        assert!(result.is_some());
        let warning = result.unwrap();
        assert!(warning.contains("CS_DISABLE_TRACKING"));
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    // ---- restart_warning ----

    #[test]
    fn restart_warning_for_access_token() {
        let option = config::find_option("access_token").unwrap();
        assert!(restart_warning(option).is_some());
    }

    #[test]
    fn restart_warning_for_other_key() {
        let onprem = config::find_option("onprem_url").unwrap();
        let ca = config::find_option("ca_bundle").unwrap();
        assert!(restart_warning(onprem).is_none());
        assert!(restart_warning(ca).is_none());
    }

    // ---- attach_docs_url ----

    #[test]
    fn attach_docs_url_adds_url() {
        let option = config::find_option("access_token").unwrap();
        let mut result = json!({});
        attach_docs_url(&mut result, option);
        assert!(result.get("docs_url").is_some());
    }

    // ---- api_only_error ----

    #[test]
    fn api_only_error_contains_error_and_valid_keys() {
        let result = api_only_error("onprem_url", true);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("not available with a standalone"));
        let valid = parsed["valid_keys"].as_array().unwrap();
        // api_only keys should be excluded in standalone mode
        assert!(valid.iter().all(|k| {
            let key = k.as_str().unwrap();
            let opt = config::find_option(key).unwrap();
            !opt.api_only
        }));
    }

    // ---- set_value / delete_key / save_key via CS_CONFIG_DIR ----

    /// Helper: run a closure with CS_CONFIG_DIR pointing to a fresh temp dir,
    /// holding the env lock to prevent parallel test interference.
    fn with_temp_config_dir<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _lock = config::lock_test_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CS_CONFIG_DIR", dir.path().as_os_str());
        let result = f();
        std::env::remove_var("CS_CONFIG_DIR");
        result
    }

    #[test]
    fn set_value_unknown_key_returns_error() {
        let result = set_value("nonexistent", "val");
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("error").is_some());
    }

    #[test]
    fn set_value_saves_key_to_config() {
        with_temp_config_dir(|| {
            let result = set_value("ca_bundle", "/path/to/cert.pem");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(parsed["status"], json!("saved"));
            assert_eq!(parsed["key"], json!("ca_bundle"));
            assert!(parsed.get("config_dir").is_some());
        });
    }

    #[test]
    fn set_value_access_token_includes_restart_warning() {
        with_temp_config_dir(|| {
            let result = set_value("access_token", "my-token");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(parsed["status"], json!("saved"));
            assert!(parsed.get("restart_required").is_some());
        });
    }

    #[test]
    fn set_value_trims_access_token() {
        with_temp_config_dir(|| {
            let _ = set_value("access_token", "  my-token  ");
            let data = config::load().unwrap();
            assert_eq!(
                data.values.get("access_token").map(|s| s.as_str()),
                Some("my-token")
            );
        });
    }

    #[test]
    fn set_value_includes_docs_url() {
        with_temp_config_dir(|| {
            let result = set_value("ca_bundle", "/cert.pem");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(parsed.get("docs_url").is_some());
        });
    }

    #[test]
    fn set_value_empty_deletes_key() {
        with_temp_config_dir(|| {
            // First save a value
            set_value("ca_bundle", "/cert.pem");
            // Then delete by setting empty
            let result = set_value("ca_bundle", "");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(parsed["status"], json!("removed"));
            assert_eq!(parsed["key"], json!("ca_bundle"));
        });
    }

    #[test]
    fn set_value_delete_includes_docs_url() {
        with_temp_config_dir(|| {
            set_value("ca_bundle", "/cert.pem");
            let result = set_value("ca_bundle", "");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(parsed.get("docs_url").is_some());
        });
    }

    #[test]
    fn set_value_client_var_includes_env_override_warning() {
        with_temp_config_dir(|| {
            // Ensure snapshot captures CS_DISABLE_TRACKING as a client var.
            std::env::set_var("CS_DISABLE_TRACKING", "1");
            config::snapshot_client_env_vars();
            let result = set_value("disable_tracking", "true");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(parsed["status"], json!("saved"));
            assert!(parsed.get("warning").is_some());
            let warning = parsed["warning"].as_str().unwrap();
            assert!(warning.contains("CS_DISABLE_TRACKING"));
            std::env::remove_var("CS_DISABLE_TRACKING");
        });
    }

    // ---- save failure paths ----

    /// Helper: run a closure with CS_CONFIG_DIR pointing to an unwritable
    /// location so that `config::save()` will fail.
    fn with_unwritable_config_dir<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _lock = config::lock_test_env();
        // /dev/null is a file, not a directory — create_dir_all will fail
        std::env::set_var("CS_CONFIG_DIR", "/dev/null/impossible");
        let result = f();
        std::env::remove_var("CS_CONFIG_DIR");
        result
    }

    // ---- restart_warning for enabled_tools ----

    #[test]
    fn restart_warning_for_enabled_tools() {
        let option = config::find_option("enabled_tools").unwrap();
        let result = restart_warning(option);
        assert!(result.is_some());
        assert!(result.unwrap().contains("restart"));
    }

    // ---- unknown_tool_names_warning ----

    #[test]
    fn unknown_tool_names_warning_returns_none_for_other_keys() {
        let option = config::find_option("access_token").unwrap();
        assert!(unknown_tool_names_warning(option, "some-value").is_none());
    }

    #[test]
    fn unknown_tool_names_warning_returns_none_for_empty_value() {
        let option = config::find_option("enabled_tools").unwrap();
        assert!(unknown_tool_names_warning(option, "").is_none());
    }

    #[test]
    fn unknown_tool_names_warning_returns_none_for_valid_tools() {
        let option = config::find_option("enabled_tools").unwrap();
        assert!(
            unknown_tool_names_warning(option, "code_health_review,code_health_score").is_none()
        );
    }

    #[test]
    fn unknown_tool_names_warning_returns_warning_for_unknown_tools() {
        let option = config::find_option("enabled_tools").unwrap();
        let result = unknown_tool_names_warning(option, "code_health_review,nonexistent_tool");
        assert!(result.is_some());
        let warning = result.unwrap();
        assert!(warning.contains("nonexistent_tool"));
        assert!(warning.contains("Unrecognized"));
    }

    #[test]
    fn unknown_tool_names_warning_ignores_always_enabled_tools() {
        // get_config and set_config are not in CONFIGURABLE_TOOL_NAMES, so they
        // should be flagged as unknown if someone puts them in the allowlist
        let option = config::find_option("enabled_tools").unwrap();
        let result = unknown_tool_names_warning(option, "get_config");
        assert!(result.is_some());
    }

    // ---- get_single for enabled_tools shows available_tools ----

    #[test]
    fn get_single_enabled_tools_includes_available_tools() {
        let data = empty_config();
        let result = get_single("enabled_tools", &data, false);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("available_tools").is_some());
        let tools = parsed["available_tools"].as_array().unwrap();
        assert!(tools.len() > 0);
        // Should include configurable tools
        assert!(tools
            .iter()
            .any(|t| t.as_str() == Some("code_health_review")));
        // Should NOT include always-on tools
        assert!(!tools.iter().any(|t| t.as_str() == Some("get_config")));
    }

    // ---- set_value for enabled_tools ----

    #[test]
    fn set_value_enabled_tools_includes_restart_warning() {
        with_temp_config_dir(|| {
            let result = set_value("enabled_tools", "code_health_review,code_health_score");
            std::env::remove_var("CS_ENABLED_TOOLS");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(parsed["status"], json!("saved"));
            assert!(parsed.get("restart_required").is_some());
        });
    }

    #[test]
    fn set_value_enabled_tools_with_unknown_name_includes_warning() {
        with_temp_config_dir(|| {
            let result = set_value("enabled_tools", "code_health_review,bogus_tool");
            std::env::remove_var("CS_ENABLED_TOOLS");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(parsed["status"], json!("saved"));
            assert!(parsed.get("tool_name_warning").is_some());
            let warning = parsed["tool_name_warning"].as_str().unwrap();
            assert!(warning.contains("bogus_tool"));
        });
    }

    #[test]
    fn set_value_enabled_tools_valid_names_no_tool_warning() {
        with_temp_config_dir(|| {
            let result = set_value("enabled_tools", "code_health_review,code_health_score");
            std::env::remove_var("CS_ENABLED_TOOLS");
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(parsed["status"], json!("saved"));
            assert!(parsed.get("tool_name_warning").is_none());
        });
    }

    // ---- save failure paths ----

    #[test]
    fn save_key_returns_error_when_save_fails() {
        with_unwritable_config_dir(|| {
            let result = set_value("ca_bundle", "/cert.pem");
            assert!(
                result.contains("Error saving config"),
                "expected save error, got: {result}"
            );
        });
    }

    #[test]
    fn delete_key_returns_error_when_save_fails() {
        with_unwritable_config_dir(|| {
            // First, set a value in ConfigData (won't persist since save fails)
            // Then try deleting — delete_key also calls save
            let result = set_value("ca_bundle", "");
            assert!(
                result.contains("Error saving config"),
                "expected save error, got: {result}"
            );
        });
    }
}

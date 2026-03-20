/// Configure tool handlers — get/set config values and produce JSON output.
///
/// Mirrors Python's `configure/configure.py` and `configure/helpers.py`.

use serde_json::json;

use crate::config::{self, ConfigData};

/// Get a single config option as JSON.
pub fn get_single(key: &str, data: &ConfigData, is_standalone: bool) -> String {
    let option = match config::find_option(key) {
        Some(o) => o,
        None => return unknown_key_error(key),
    };

    if option.api_only && is_standalone {
        return api_only_error(key, is_standalone);
    }

    let result = format_option_json(option, data);
    serde_json::to_string(&result).unwrap_or_default()
}

/// Get all listable config options as JSON.
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

/// Set or remove a config value and return JSON result.
pub fn set_value(key: &str, value: &str) -> String {
    let option = match config::find_option(key) {
        Some(o) => o,
        None => return unknown_key_error(key),
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
    data.values.insert(option.key.to_string(), value.to_string());
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
    if let Some(restart) = restart_warning(option.key) {
        result["restart_required"] = json!(restart);
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

fn restart_warning(key: &str) -> Option<&'static str> {
    if key != "access_token" {
        return None;
    }
    Some(
        "Changing the access token may affect which tools are \
         available (API vs standalone mode). A server restart is required \
         for tool registration changes to take effect.",
    )
}

fn attach_docs_url(result: &mut serde_json::Value, option: &config::ConfigOption) {
    if !option.docs_url.is_empty() {
        result["docs_url"] = json!(option.docs_url);
    }
}

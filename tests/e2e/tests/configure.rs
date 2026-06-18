//! Integration tests for `get_config` and `set_config` tools.
//!
//! Validates configuration management: reading, writing, masking
//! sensitive values, environment variable overrides, hidden options,
//! and standalone mode filtering.

use super::*;

use std::collections::HashMap;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(30);

const VALID_STANDALONE_JWT: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6ImNzbWNwIiwidHlwIjoiSldTIn0.eyJpc3MiOiJjb2Rlc2NlbmUtbWNwIiwiYXVkIjoiY29kZXNjZW5lLWNsaSIsImlhdCI6MTc3MTk0NTM1NSwiZXhwIjoxNzcyMjgxNjUzLCJzdWIiOiIyYTM5NDAyNS1kYjg2LTQwMDAtYWE0NS1lODY2Yjk5YmJhMzcifQ.V0UxjlS1ZK-hcF1M7edu6GfvMAjv1XukFe8m6iHzS9guh_4rqu4HGbRTzl217qMemCjwyHtAG9pO6NUu3SWbCQ";

const VISIBLE_KEYS: &[&str] = &["access_token", "onprem_url", "default_project_id", "ca_bundle"];
const HIDDEN_KEYS: &[&str] = &["disable_tracking", "disable_version_check", "tracking_environment"];

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct TestEnv {
    command: Vec<String>,
    env: Vec<(String, String)>,
    repo_dir: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

fn configure_setup() -> TestEnv {
    configure_setup_with_extra_env(&[])
}

fn configure_setup_with_extra_env(extra: &[(&str, &str)]) -> TestEnv {
    let executable = super::find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_configure_").expect("temp dir");

    let mut sample_files = HashMap::new();
    sample_files.insert("hello.py", "def hello():\n    return 'world'\n");

    let repo_dir = create_git_repo(temp_dir.path(), &sample_files).expect("git repo");

    let config_dir = repo_dir.join(".cs_config");
    std::fs::create_dir_all(&config_dir).expect("config dir");

    let base = base_env();
    let mut env_map = backend.get_env(&base, &repo_dir);
    env_map.insert("CS_CONFIG_DIR".to_string(), docker_config_dir(&config_dir, &repo_dir));

    for (k, v) in extra {
        env_map.insert(k.to_string(), v.to_string());
    }

    let env_vec: Vec<(String, String)> = env_map.into_iter().collect();
    let command = backend.get_command(&repo_dir);

    TestEnv {
        command,
        env: env_vec,
        repo_dir,
        _tmp: temp_dir,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn start(env: &TestEnv) -> MCPClient {
    let mut client = make_client(&env.command, &env.env, &env.repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    client
}

fn set_config(client: &mut MCPClient, key: &str, value: &str) -> String {
    let response = client
        .call_tool("set_config", json!({"key": key, "value": value}), TIMEOUT)
        .expect("set_config should succeed");
    extract_result_text(&response)
}

fn get_config(client: &mut MCPClient, key: &str) -> String {
    let response = client
        .call_tool("get_config", json!({"key": key}), TIMEOUT)
        .expect("get_config should succeed");
    extract_result_text(&response)
}

fn get_config_all(client: &mut MCPClient) -> String {
    let response = client
        .call_tool("get_config", json!({}), TIMEOUT)
        .expect("get_config should succeed");
    extract_result_text(&response)
}

fn set_then_get(client: &mut MCPClient, key: &str, value: &str) -> String {
    set_config(client, key, value);
    get_config(client, key)
}

fn tool_names(client: &mut MCPClient) -> Vec<String> {
    let response = client
        .send_request("tools/list", json!({}), TIMEOUT)
        .expect("tools/list should succeed");

    response["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name").to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

pub fn test_tools_visible() {
    let env = configure_setup();
    let mut client = start(&env);

    let names = tool_names(&mut client);

    assert!(names.contains(&"get_config".to_string()), "get_config should be listed");
    assert!(names.contains(&"set_config".to_string()), "set_config should be listed");
}

pub fn test_set_then_get() {
    let env = configure_setup();
    let mut client = start(&env);

    let result = set_then_get(&mut client, "onprem_url", "https://my.server.com");

    assert!(
        result.contains("https://my.server.com"),
        "Should contain the configured URL, got: {result}"
    );
}

pub fn test_sensitive_masking() {
    let env = configure_setup();
    let mut client = start(&env);

    let token = "super-secret-token-12345";
    let result = set_then_get(&mut client, "access_token", token);

    assert!(
        !result.contains(token),
        "Full token must NOT appear in output, got: {result}"
    );
    assert!(
        result.contains("..."),
        "Masked value should contain '...', got: {result}"
    );
}

pub fn test_list_all() {
    let env = configure_setup();
    let mut client = start(&env);

    let result = get_config_all(&mut client);
    let lower = result.to_lowercase();

    for key in VISIBLE_KEYS {
        assert!(lower.contains(key), "Visible key '{key}' should appear in output");
    }
    for key in HIDDEN_KEYS {
        assert!(!lower.contains(key), "Hidden key '{key}' should NOT appear in listing");
    }
}

pub fn test_invalid_key() {
    let env = configure_setup();
    let mut client = start(&env);

    let result = set_config(&mut client, "no_such_key", "whatever");
    let lower = result.to_lowercase();

    assert!(lower.contains("unknown"), "Should mention 'unknown', got: {result}");

    let mentions_valid = VISIBLE_KEYS.iter().any(|k| lower.contains(k));
    assert!(mentions_valid, "Should list at least one valid key, got: {result}");
}

pub fn test_delete_value() {
    let env = configure_setup();
    let mut client = start(&env);

    set_config(&mut client, "default_project_id", "42");
    let result = set_then_get(&mut client, "default_project_id", "");
    let lower = result.to_lowercase();

    assert!(
        lower.contains("null") || lower.contains("not set"),
        "Deleted value should show 'null' or 'not set', got: {result}"
    );
}

pub fn test_env_override() {
    let env = configure_setup_with_extra_env(&[("CS_ONPREM_URL", "https://env.server.com")]);
    let mut client = start(&env);

    set_config(&mut client, "onprem_url", "https://config.server.com");
    let result = get_config(&mut client, "onprem_url");
    let lower = result.to_lowercase();

    assert!(
        result.contains("https://env.server.com"),
        "Env value should take precedence, got: {result}"
    );
    assert!(
        lower.contains("environment"),
        "Source should mention 'environment', got: {result}"
    );
}

pub fn test_hidden_option_accessible_by_key() {
    let env = configure_setup();
    let mut client = start(&env);

    let result_tracking = set_then_get(&mut client, "disable_tracking", "true");
    assert!(
        result_tracking.contains("true"),
        "disable_tracking should be readable by key, got: {result_tracking}"
    );

    let result_env = set_then_get(&mut client, "tracking_environment", "test-env");
    assert!(
        result_env.contains("test-env"),
        "tracking_environment should be readable by key, got: {result_env}"
    );
}

pub fn test_standalone_hides_api_only() {
    let env = configure_setup_with_extra_env(&[("CS_ACCESS_TOKEN", VALID_STANDALONE_JWT)]);
    let mut client = start(&env);

    let result = get_config_all(&mut client);
    let lower = result.to_lowercase();

    assert!(
        !lower.contains("onprem_url"),
        "onprem_url should be hidden in standalone mode, got: {result}"
    );
    assert!(
        !lower.contains("default_project_id"),
        "default_project_id should be hidden in standalone mode, got: {result}"
    );
    assert!(
        lower.contains("access_token"),
        "access_token should still appear in standalone mode, got: {result}"
    );
    assert!(
        lower.contains("ca_bundle"),
        "ca_bundle should still appear in standalone mode, got: {result}"
    );
}

// ---------------------------------------------------------------------------
// HTTPS URL validation tests
// ---------------------------------------------------------------------------

pub fn test_set_config_rejects_http_url() {
    let env = configure_setup();
    let mut client = start(&env);

    let result = set_config(&mut client, "onprem_url", "http://insecure.example.com");
    let lower = result.to_lowercase();

    assert!(
        lower.contains("https"),
        "Error should mention HTTPS requirement, got: {result}"
    );
    assert!(
        lower.contains("error") || lower.contains("must"),
        "Should indicate an error, got: {result}"
    );
}

pub fn test_set_config_accepts_https_url() {
    let env = configure_setup();
    let mut client = start(&env);

    let result = set_then_get(&mut client, "onprem_url", "https://secure.example.com");

    assert!(
        result.contains("https://secure.example.com"),
        "Should accept and store HTTPS URL, got: {result}"
    );
}

pub fn test_set_config_http_rejection_does_not_persist() {
    let env = configure_setup();
    let mut client = start(&env);

    // Try to set an HTTP URL (should be rejected)
    set_config(&mut client, "onprem_url", "http://insecure.example.com");

    // Verify nothing was persisted
    let result = get_config(&mut client, "onprem_url");
    let lower = result.to_lowercase();

    assert!(
        !result.contains("http://insecure.example.com"),
        "Rejected HTTP URL must not be persisted, got: {result}"
    );
    assert!(
        lower.contains("null") || lower.contains("not set") || !lower.contains("insecure"),
        "Value should be unset after rejection, got: {result}"
    );
}

pub fn test_set_config_non_url_key_unaffected() {
    let env = configure_setup();
    let mut client = start(&env);

    // Non-URL keys should accept any value without HTTPS validation
    let result = set_then_get(&mut client, "default_project_id", "42");

    assert!(
        result.contains("42"),
        "Non-URL key should accept any value, got: {result}"
    );
}

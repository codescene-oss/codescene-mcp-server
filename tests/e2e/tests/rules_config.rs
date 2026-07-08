//! Integration tests for the `rules_config_*` tools.
//!
//! Validates that the MCP server correctly wraps the `cs rules-config` CLI
//! subcommands for managing Code Health rules configuration files:
//!
//! - rules_config_validate: validates a code-health-rules.json file
//! - rules_config_list_thresholds: lists a language's default thresholds
//! - rules_config_set_rule: enables/disables a Code Health rule
//! - rules_config_set_threshold: sets a Code Health threshold value
//!
//! These tools are local, filesystem-only operations that require no access
//! token. Tests exercise the real CLI against a config file inside the test
//! git repository, so `config_path` is passed as a host path that the server
//! adapts for Docker automatically.

use super::*;

use std::collections::HashMap;

const TIMEOUT: Duration = Duration::from_secs(60);

const RULES_CONFIG: &str = r#"{
  "rule_sets": [
    {
      "matching_content_path": "**/*",
      "rules": [
        { "name": "Complex Method", "weight": 1.0 },
        { "name": "Large Method", "weight": 1.0 }
      ],
      "thresholds": [
        { "name": "function_lines_of_code_warning", "value": 70 }
      ]
    }
  ]
}
"#;

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct TestEnv {
    command: Vec<String>,
    env: Vec<(String, String)>,
    repo_dir: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

/// Prepare a git repo containing `.codescene/code-health-rules.json`.
fn rules_config_setup() -> TestEnv {
    let executable = super::find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_rules_config_").expect("temp dir");

    let mut sample_files = HashMap::new();
    sample_files.insert("hello.py", "def hello():\n    return 'world'\n");
    sample_files.insert(".codescene/code-health-rules.json", RULES_CONFIG);

    let repo_dir = create_git_repo(temp_dir.path(), &sample_files).expect("git repo");

    let base = base_env();
    let env_map = backend.get_env(&base, &repo_dir);
    let env_vec: Vec<(String, String)> = env_map.into_iter().collect();
    let command = backend.get_command(&repo_dir);

    TestEnv {
        command,
        env: env_vec,
        repo_dir,
        _tmp: temp_dir,
    }
}

/// The host path of the rules config file inside the test repo.
fn config_path(env: &TestEnv) -> String {
    env.repo_dir
        .join(".codescene/code-health-rules.json")
        .to_string_lossy()
        .to_string()
}

/// Read and parse the rules config file back from disk so tests can assert
/// that an edit was actually persisted (not just confirmed in the response).
fn read_config(env: &TestEnv) -> serde_json::Value {
    let raw = std::fs::read_to_string(config_path(env)).expect("read rules config file");
    serde_json::from_str(&raw).expect("rules config should be valid JSON")
}

/// Find the weight of a named rule in the (single) rule set.
fn rule_weight(config: &serde_json::Value, rule_name: &str) -> Option<f64> {
    config["rule_sets"][0]["rules"]
        .as_array()?
        .iter()
        .find(|r| r["name"] == rule_name)?
        .get("weight")?
        .as_f64()
}

/// Find the value of a named threshold in the (single) rule set.
fn threshold_value(config: &serde_json::Value, threshold_name: &str) -> Option<i64> {
    config["rule_sets"][0]["thresholds"]
        .as_array()?
        .iter()
        .find(|t| t["name"] == threshold_name)?
        .get("value")?
        .as_i64()
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

fn call(client: &mut MCPClient, tool: &str, args: serde_json::Value) -> String {
    let response = client
        .call_tool(tool, args, TIMEOUT)
        .unwrap_or_else(|e| panic!("{tool} call should succeed: {e:?}"));
    extract_result_text(&response)
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

pub fn test_rules_config_tools_listed() {
    let env = rules_config_setup();
    let mut client = start(&env);

    let names = tool_names(&mut client);
    for tool in [
        "rules_config_validate",
        "rules_config_list_thresholds",
        "rules_config_set_rule",
        "rules_config_set_threshold",
    ] {
        assert!(
            names.contains(&tool.to_string()),
            "{tool} should be listed, got: {names:?}"
        );
    }
}

pub fn test_validate_reports_valid_config() {
    let env = rules_config_setup();
    let mut client = start(&env);

    let result = call(
        &mut client,
        "rules_config_validate",
        json!({"config_path": config_path(&env)}),
    );
    let lower = result.to_lowercase();

    assert!(
        lower.contains("ok") || lower.contains("valid"),
        "validate should report a valid config, got: {result}"
    );
    assert_no_errors(&result);
}

pub fn test_list_thresholds_returns_defaults() {
    let env = rules_config_setup();
    let mut client = start(&env);

    // No config_path needed: the CLI returns built-in defaults for the language.
    let result = call(
        &mut client,
        "rules_config_list_thresholds",
        json!({"language": "Python"}),
    );

    assert!(
        result.contains("function_lines_of_code_warning"),
        "thresholds should include known Python threshold names, got: {result}"
    );
    assert_no_errors(&result);
}

pub fn test_list_thresholds_rejects_unknown_language() {
    let env = rules_config_setup();
    let mut client = start(&env);

    let result = call(
        &mut client,
        "rules_config_list_thresholds",
        json!({"language": "Klingon"}),
    );

    assert!(
        result.to_lowercase().contains("language"),
        "unknown language should produce a language error, got: {result}"
    );
}

/// Call a mutating rules-config tool and assert the confirmation text mentions
/// each expected (lower-cased) fragment.
fn assert_edit_confirmed(
    client: &mut MCPClient,
    tool: &str,
    args: serde_json::Value,
    expected_fragments: &[&str],
) {
    let result = call(client, tool, args);
    let lower = result.to_lowercase();
    for fragment in expected_fragments {
        assert!(
            lower.contains(fragment),
            "{tool} should confirm '{fragment}', got: {result}"
        );
    }
    assert_no_errors(&result);
}

pub fn test_set_rule_disable_and_enable_persist() {
    let env = rules_config_setup();
    let mut client = start(&env);
    let path = config_path(&env);

    // Sanity: the rule starts enabled (weight 1.0).
    assert_eq!(rule_weight(&read_config(&env), "Complex Method"), Some(1.0));

    // Disable → weight 0.0 on disk.
    assert_edit_confirmed(
        &mut client,
        "rules_config_set_rule",
        json!({"rule_name": "Complex Method", "enabled": false, "config_path": path}),
        &["complex method", "disabled"],
    );
    assert_eq!(
        rule_weight(&read_config(&env), "Complex Method"),
        Some(0.0),
        "disabling a rule should persist weight 0.0"
    );

    // Re-enable → weight 1.0 on disk.
    assert_edit_confirmed(
        &mut client,
        "rules_config_set_rule",
        json!({"rule_name": "Complex Method", "enabled": true, "config_path": path}),
        &["complex method", "enabled"],
    );
    assert_eq!(
        rule_weight(&read_config(&env), "Complex Method"),
        Some(1.0),
        "re-enabling a rule should persist weight 1.0"
    );
}

pub fn test_set_threshold_persists_value() {
    let env = rules_config_setup();
    let mut client = start(&env);
    let path = config_path(&env);

    // Sanity: starts at the value from the fixture.
    assert_eq!(
        threshold_value(&read_config(&env), "function_lines_of_code_warning"),
        Some(70)
    );

    assert_edit_confirmed(
        &mut client,
        "rules_config_set_threshold",
        json!({
            "threshold_name": "function_lines_of_code_warning",
            "value": 120,
            "config_path": path,
        }),
        &["function_lines_of_code_warning", "120"],
    );
    assert_eq!(
        threshold_value(&read_config(&env), "function_lines_of_code_warning"),
        Some(120),
        "set_threshold should persist the new value to disk"
    );
}

pub fn test_set_threshold_rejects_invalid_value() {
    let env = rules_config_setup();
    let mut client = start(&env);
    let path = config_path(&env);

    // Zero is not a positive integer; the CLI must reject it and leave the
    // file unchanged.
    let result = call(
        &mut client,
        "rules_config_set_threshold",
        json!({
            "threshold_name": "function_lines_of_code_warning",
            "value": 0,
            "config_path": path,
        }),
    );
    assert!(
        result.to_lowercase().contains("positive"),
        "value 0 should be rejected as non-positive, got: {result}"
    );
    assert_eq!(
        threshold_value(&read_config(&env), "function_lines_of_code_warning"),
        Some(70),
        "a rejected edit must not modify the file"
    );
}

pub fn test_set_then_validate_roundtrip() {
    let env = rules_config_setup();
    let mut client = start(&env);
    let path = config_path(&env);

    // Edit the file, then confirm it is still valid afterwards.
    call(
        &mut client,
        "rules_config_set_threshold",
        json!({
            "threshold_name": "function_lines_of_code_warning",
            "value": 90,
            "config_path": path,
        }),
    );
    let result = call(
        &mut client,
        "rules_config_validate",
        json!({"config_path": path}),
    );

    assert!(
        result.to_lowercase().contains("valid") || result.to_lowercase().contains("ok"),
        "config should remain valid after an edit, got: {result}"
    );
    assert_no_errors(&result);
}

pub fn test_relative_config_path_is_rejected() {
    let env = rules_config_setup();
    let mut client = start(&env);

    let result = call(
        &mut client,
        "rules_config_validate",
        json!({"config_path": "relative/code-health-rules.json"}),
    );

    assert!(
        result.to_lowercase().contains("absolute"),
        "a relative config_path should be rejected, got: {result}"
    );
}

pub fn test_works_without_access_token() {
    // rules_config tools are local operations and must work without a token,
    // unlike the analysis and explain tools which are token-gated.
    let mut env = rules_config_setup();
    env.env.retain(|(k, _)| k != "CS_ACCESS_TOKEN");
    let mut client = start(&env);

    let result = call(
        &mut client,
        "rules_config_list_thresholds",
        json!({"language": "Python"}),
    );

    assert!(
        !result.contains("No access token configured"),
        "list_thresholds should work without a token, got: {result}"
    );
    assert!(
        result.contains("function_lines_of_code_warning"),
        "should still return thresholds without a token, got: {result}"
    );
}

fn assert_no_errors(result: &str) {
    let lower = result.to_lowercase();
    for pattern in ["traceback", "no such file", "os error 2"] {
        assert!(
            !lower.contains(pattern),
            "response must not contain '{pattern}': {result}"
        );
    }
}

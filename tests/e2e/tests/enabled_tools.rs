//! Integration tests for the `enabled_tools` configuration option.
//!
//! Validates that the MCP server correctly filters tools based on the
//! `CS_ENABLED_TOOLS` environment variable and the `set_config` /
//! `get_config` tool interactions.

use super::*;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the set of tool names advertised by the server.
fn get_tool_names(client: &mut MCPClient) -> HashSet<String> {
    let response = client
        .send_request("tools/list", json!({}), Duration::from_secs(15))
        .expect("tools/list should succeed");

    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools should be an array");

    tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name").to_string())
        .collect()
}

/// Create the minimal test environment used by all tests in this module.
///
/// Returns `(command, env, repo_dir, config_dir, _temp_dir)`.
fn enabled_tools_setup() -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let executable = super::find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_enabled_tools_").expect("temp dir");

    let mut sample_files = HashMap::new();
    sample_files.insert("hello.py", "def hello():\n    return 'world'\n");

    let repo_dir = create_git_repo(temp_dir.path(), &sample_files).expect("git repo");

    let config_dir = repo_dir.join(".cs_config");
    std::fs::create_dir_all(&config_dir).expect("config dir");

    let base = base_env();
    let mut env_map = backend.get_env(&base, &repo_dir);
    env_map.insert(
        "CS_CONFIG_DIR".to_string(),
        docker_config_dir(&config_dir, &repo_dir),
    );

    let env_vec: Vec<(String, String)> = env_map.into_iter().collect();
    let command = backend.get_command(&repo_dir);

    (command, env_vec, repo_dir, config_dir, temp_dir)
}

/// Start and initialize an MCP client with the given env overrides applied.
fn start_client(
    command: &[String],
    base_env: &[(String, String)],
    cwd: &std::path::Path,
    extra_env: &[(&str, &str)],
) -> MCPClient {
    let mut env: Vec<(String, String)> = base_env.to_vec();
    for (k, v) in extra_env {
        env.push((k.to_string(), v.to_string()));
    }

    let mut client = make_client(command, &env, cwd);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    client
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
pub fn test_all_tools_without_filter() {
    let (command, env, repo_dir, _config_dir, _tmp) = enabled_tools_setup();
    let mut client = start_client(&command, &env, &repo_dir, &[]);

    let names = get_tool_names(&mut client);

    assert!(names.contains("get_config"), "get_config should be listed");
    assert!(names.contains("set_config"), "set_config should be listed");
    assert!(
        names.contains("code_health_review"),
        "code_health_review should be listed"
    );
    assert!(
        names.contains("code_health_score"),
        "code_health_score should be listed"
    );
    assert!(
        names.contains("explain_code_health"),
        "explain_code_health should be listed"
    );
    assert!(
        names.len() >= 10,
        "Expected >= 10 tools, found {}",
        names.len()
    );
}

#[test]
pub fn test_filter_restricts_tools() {
    let (command, env, repo_dir, _config_dir, _tmp) = enabled_tools_setup();
    let mut client = start_client(
        &command,
        &env,
        &repo_dir,
        &[("CS_ENABLED_TOOLS", "code_health_review,code_health_score")],
    );

    let names = get_tool_names(&mut client);

    assert!(
        names.contains("code_health_review"),
        "code_health_review listed"
    );
    assert!(
        names.contains("code_health_score"),
        "code_health_score listed"
    );
    assert!(names.contains("get_config"), "get_config always listed");
    assert!(names.contains("set_config"), "set_config always listed");
    assert!(
        !names.contains("explain_code_health"),
        "explain_code_health should NOT be listed"
    );
    assert_eq!(
        names.len(),
        4,
        "Expected exactly 4 tools, found {}: {:?}",
        names.len(),
        names
    );
}

#[test]
pub fn test_config_tools_always_present() {
    let (command, env, repo_dir, _config_dir, _tmp) = enabled_tools_setup();
    let mut client = start_client(
        &command,
        &env,
        &repo_dir,
        &[("CS_ENABLED_TOOLS", "explain_code_health")],
    );

    let names = get_tool_names(&mut client);

    assert!(names.contains("get_config"), "get_config always present");
    assert!(names.contains("set_config"), "set_config always present");
    assert!(
        names.contains("explain_code_health"),
        "explain_code_health present"
    );
}

fn call_config_tool(tool_name: &str, args: serde_json::Value) -> String {
    let (command, env, repo_dir, _config_dir, _tmp) = enabled_tools_setup();
    let mut client = start_client(&command, &env, &repo_dir, &[]);
    let response = client
        .call_tool(tool_name, args, Duration::from_secs(30))
        .expect("Config tool call should succeed");
    extract_result_text(&response)
}

#[test]
pub fn test_set_enabled_tools_restart_warning() {
    let text = call_config_tool(
        "set_config",
        json!({"key": "enabled_tools", "value": "code_health_review,code_health_score"}),
    )
    .to_lowercase();

    assert!(text.contains("saved"), "Response should mention 'saved'");
    assert!(
        text.contains("restart"),
        "Response should mention 'restart'"
    );
}

#[test]
pub fn test_set_invalid_tool_name_warning() {
    let text = call_config_tool(
        "set_config",
        json!({"key": "enabled_tools", "value": "code_health_review,nonexistent_tool"}),
    )
    .to_lowercase();

    assert!(
        text.contains("nonexistent_tool"),
        "Should mention the invalid tool name"
    );
    assert!(text.contains("unrecognized"), "Should say 'unrecognized'");
}

#[test]
pub fn test_get_enabled_tools_shows_available() {
    let text = call_config_tool("get_config", json!({"key": "enabled_tools"}));

    assert!(
        text.contains("available_tools"),
        "Response should contain 'available_tools'"
    );
    assert!(
        text.contains("code_health_review"),
        "Should list code_health_review"
    );
}

//! Tests that access-token gating works correctly.
//!
//! Guarded tools must reject requests when no token is configured,
//! while configuration tools must remain accessible.

use super::*;

const HELLO_PY: &str = "def hello():\n    print('hello')\n";

/// Shared helper: start a client, call one tool, return the result text.
fn run_tool_with_env(
    command: &[String],
    env: &[(String, String)],
    cwd: &Path,
    tool_name: &str,
    args: serde_json::Value,
    timeout: Duration,
) -> String {
    let mut client = make_client(command, env, cwd);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(tool_name, args, timeout)
        .expect("Tool call should succeed");

    extract_result_text(&response)
}

/// Build the test environment with a minimal repo and no access token.
fn setup_no_token() -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let (command, env, repo_dir, tmp) = setup();

    let config_dir = repo_dir.join(".cs_config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let env: Vec<(String, String)> = env
        .into_iter()
        .filter(|(k, _)| k != "CS_ACCESS_TOKEN")
        .chain(std::iter::once((
            "CS_CONFIG_DIR".to_string(),
            config_dir.to_string_lossy().to_string(),
        )))
        .collect();

    (command, env, repo_dir, tmp)
}

pub fn test_guarded_tool_blocked_without_token() {
    let (command, env, repo_dir, _tmp) = setup_no_token();
    let file_path = repo_dir.join("hello.py");
    std::fs::write(&file_path, HELLO_PY).expect("write hello.py");

    let result = run_tool_with_env(
        &command,
        &env,
        &repo_dir,
        "code_health_score",
        json!({"file_path": file_path.to_string_lossy()}),
        Duration::from_secs(30),
    );

    assert!(
        result.contains("No access token configured"),
        "Guarded tool should report missing token, got: {result}"
    );
    assert!(
        result.contains("set_config"),
        "Should mention set_config as remedy, got: {result}"
    );
}

pub fn test_explain_tool_blocked_without_token() {
    let (command, env, repo_dir, _tmp) = setup_no_token();

    let result = run_tool_with_env(
        &command,
        &env,
        &repo_dir,
        "explain_code_health",
        json!({}),
        Duration::from_secs(30),
    );

    assert!(
        result.contains("No access token configured"),
        "Explain tool should report missing token, got: {result}"
    );
    assert!(
        result.contains("set_config"),
        "Should mention set_config as remedy, got: {result}"
    );
}

pub fn test_get_config_works_without_token() {
    let (command, env, repo_dir, _tmp) = setup_no_token();

    let result = run_tool_with_env(
        &command,
        &env,
        &repo_dir,
        "get_config",
        json!({}),
        Duration::from_secs(30),
    );

    assert!(
        !result.contains("No access token configured"),
        "get_config should work without token, got: {result}"
    );
}

pub fn test_set_config_works_without_token() {
    let (command, env, repo_dir, _tmp) = setup_no_token();

    let result = run_tool_with_env(
        &command,
        &env,
        &repo_dir,
        "set_config",
        json!({"key": "access_token", "value": "test-token"}),
        Duration::from_secs(30),
    );

    assert!(
        !result.contains("No access token configured"),
        "set_config should work without token, got: {result}"
    );
}

pub fn test_guarded_tool_works_with_token() {
    let (command, env, repo_dir, _tmp) = setup();
    let file_path = repo_dir.join("src/utils/calculator.py");

    let result = run_tool_with_env(
        &command,
        &env,
        &repo_dir,
        "code_health_score",
        json!({"file_path": file_path.to_string_lossy()}),
        Duration::from_secs(60),
    );

    assert!(
        !result.contains("No access token configured"),
        "Should not be blocked with token, got: {result}"
    );
    assert!(
        !result.is_empty(),
        "Should return content when token is present"
    );
}

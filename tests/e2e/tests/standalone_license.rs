//! Integration tests for standalone license (JWT) vs PAT token behaviour.
//!
//! Validates that:
//! - A standalone JWT hides API-dependent tools while keeping CLI tools.
//! - A PAT token exposes all tools (API + CLI).

use super::*;

use std::collections::HashSet;
use std::time::Duration;

const VALID_STANDALONE_JWT: &str = "\
    eyJhbGciOiJFZERTQSIsImtpZCI6ImNzbWNwIiwidHlwIjoiSldTIn0.\
    eyJpc3MiOiJjb2Rlc2NlbmUtbWNwIiwiYXVkIjoiY29kZXNjZW5lLWNsaSIsImlhdCI6MTc3MTk0NTM1NSwiZXhwIjoxNzcyMjgxNjUzLCJzdWIiOiIyYTM5NDAyNS1kYjg2LTQwMDAtYWE0NS1lODY2Yjk5YmJhMzcifQ.\
    V0UxjlS1ZK-hcF1M7edu6GfvMAjv1XukFe8m6iHzS9guh_4rqu4HGbRTzl217qMemCjwyHtAG9pO6NUu3SWbCQ";

const API_TOOLS: &[&str] = &[
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
];

const CLI_TOOLS: &[&str] = &[
    "pre_commit_code_health_safeguard",
    "analyze_change_set",
    "code_health_refactoring_business_case",
    "code_health_score",
    "code_health_review",
    "explain_code_health",
    "explain_code_health_productivity",
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a server with the given token and return the advertised tool names.
fn get_tool_names_with_token(
    command: &[String],
    env: &[(String, String)],
    cwd: &Path,
    token: &str,
) -> HashSet<String> {
    let env: Vec<(String, String)> = env
        .iter()
        .filter(|(k, _)| k != "CS_ACCESS_TOKEN")
        .cloned()
        .chain(std::iter::once((
            "CS_ACCESS_TOKEN".to_string(),
            token.to_string(),
        )))
        .collect();

    let mut client = make_client(command, &env, cwd);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .send_request("tools/list", json!({}), Duration::from_secs(15))
        .expect("tools/list should succeed");

    response["result"]["tools"]
        .as_array()
        .expect("tools should be an array")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name").to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

pub fn test_standalone_hides_api_tools() {
    let (command, env, repo_dir, _tmp) = setup();
    let tools = get_tool_names_with_token(&command, &env, &repo_dir, VALID_STANDALONE_JWT);

    for api_tool in API_TOOLS {
        assert!(
            !tools.contains(*api_tool),
            "Standalone JWT should hide {api_tool}, but it was listed"
        );
    }
}

pub fn test_standalone_keeps_cli_tools() {
    let (command, env, repo_dir, _tmp) = setup();
    let tools = get_tool_names_with_token(&command, &env, &repo_dir, VALID_STANDALONE_JWT);

    for cli_tool in CLI_TOOLS {
        assert!(
            tools.contains(*cli_tool),
            "Standalone JWT should keep {cli_tool}, but it was missing"
        );
    }
}

pub fn test_pat_exposes_all_tools() {
    let (command, env, repo_dir, _tmp) = setup();
    let token = "cst_fake_pat_for_integration_test";
    let tools = get_tool_names_with_token(&command, &env, &repo_dir, token);

    let all_expected: Vec<&str> = API_TOOLS.iter().chain(CLI_TOOLS.iter()).copied().collect();
    for tool in &all_expected {
        assert!(
            tools.contains(*tool),
            "PAT should expose {tool}, but it was missing"
        );
    }
}

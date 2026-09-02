//! End-to-end verification of MCP tool behavior annotations.
//!
//! Validates that `tools/list` exposes an explicit, correct `readOnlyHint`
//! classification for every tool available through each server backend.

use super::*;

use std::collections::HashMap;

const TIMEOUT: Duration = Duration::from_secs(15);
const READ_ONLY_TOOLS: &[&str] = &[
    "show_mcp_usage_overview",
    "explain_code_health",
    "explain_code_health_productivity",
    "code_health_review",
    "code_health_score",
    "code_health_refactoring_business_case",
    "rules_config_validate",
    "rules_config_list_thresholds",
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
    "get_config",
    "list_skills",
    "get_skill_manifest",
    "verify_installation",
];
const MUTATING_TOOLS: &[&str] = &[
    "pre_commit_code_health_safeguard",
    "analyze_change_set",
    "rules_config_set_rule",
    "rules_config_set_threshold",
    "set_config",
    "login",
    "logout",
    "switch_account",
    "download_skill",
    "sync_skills",
];

fn expected_annotations() -> HashMap<&'static str, bool> {
    READ_ONLY_TOOLS
        .iter()
        .map(|name| (*name, true))
        .chain(MUTATING_TOOLS.iter().map(|name| (*name, false)))
        .filter(|(name, _)| !is_docker() || !matches!(*name, "login" | "switch_account"))
        .collect()
}

pub fn test_tool_read_only_annotations() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .send_request("tools/list", json!({}), TIMEOUT)
        .expect("tools/list should succeed");
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools should be an array");
    let expected = expected_annotations();

    assert_eq!(
        tools.len(),
        expected.len(),
        "unexpected advertised tool set"
    );
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name should be a string");
        let expected_hint = expected
            .get(name)
            .unwrap_or_else(|| panic!("unclassified tool {name}"));
        assert_eq!(
            tool["annotations"]["readOnlyHint"].as_bool(),
            Some(*expected_hint),
            "wrong readOnlyHint for {name}"
        );
    }
}

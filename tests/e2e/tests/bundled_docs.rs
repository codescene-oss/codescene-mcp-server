//! Bundled documentation integration tests.
//!
//! Tests that the MCP server correctly serves embedded documentation
//! via the `explain_code_health` and `explain_code_health_productivity`
//! tools without file-not-found errors.

use super::*;

const TIMEOUT: Duration = Duration::from_secs(30);
const MIN_CONTENT_LENGTH: usize = 100;
const MIN_MATCHING_TERMS: usize = 2;

const CODE_HEALTH_TERMS: &[&str] = &["code health", "maintainability", "code", "quality"];
const PRODUCTIVITY_TERMS: &[&str] = &["productivity", "defect", "business", "code health"];
const ERROR_PATTERNS: &[&str] = &[
    "no such file or directory",
    "filenotfounderror",
    "not found",
];

fn call_doc_tool(client: &mut MCPClient, tool_name: &str) -> String {
    let response = client
        .call_tool(tool_name, json!({}), TIMEOUT)
        .unwrap_or_else(|e| panic!("{tool_name} should succeed: {e}"));
    extract_result_text(&response)
}

fn assert_doc_content(result_text: &str, expected_terms: &[&str], min_terms: usize) {
    assert!(
        result_text.len() > MIN_CONTENT_LENGTH,
        "Expected > {MIN_CONTENT_LENGTH} chars, got {}",
        result_text.len()
    );

    let lower = result_text.to_lowercase();
    let found = expected_terms
        .iter()
        .filter(|term| lower.contains(*term))
        .count();

    assert!(
        found >= min_terms,
        "Expected >= {min_terms} matching terms, found {found}"
    );
}

fn setup_doc_client(command: &[String], env: &[(String, String)], repo_dir: &Path) -> MCPClient {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    client
}

pub fn test_explain_code_health() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = setup_doc_client(&command, &env, &repo_dir);
    let result = call_doc_tool(&mut client, "explain_code_health");
    assert_doc_content(&result, CODE_HEALTH_TERMS, MIN_MATCHING_TERMS);
}

pub fn test_explain_code_health_productivity() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = setup_doc_client(&command, &env, &repo_dir);
    let result = call_doc_tool(&mut client, "explain_code_health_productivity");
    assert_doc_content(&result, PRODUCTIVITY_TERMS, MIN_MATCHING_TERMS);
}

pub fn test_no_doc_file_errors() {
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = setup_doc_client(&command, &env, &repo_dir);

    let tools = ["explain_code_health", "explain_code_health_productivity"];

    for tool_name in tools {
        let result = call_doc_tool(&mut client, tool_name);
        let lower = result.to_lowercase();

        for pattern in ERROR_PATTERNS {
            assert!(
                !lower.contains(pattern),
                "{tool_name} must not contain '{pattern}': {result}"
            );
        }

        assert!(
            !lower.contains("traceback"),
            "{tool_name} must not contain traceback: {result}"
        );
    }
}

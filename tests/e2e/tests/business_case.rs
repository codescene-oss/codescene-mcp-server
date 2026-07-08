//! Business case integration tests.
//!
//! Tests that the MCP server correctly generates refactoring business case data
//! via the `code_health_refactoring_business_case` tool.
//!
//! Validates:
//! - The tool returns meaningful content for complex files
//! - Responses contain expected business case metrics
//! - Regression coefficient files (defects.json, time.json) are properly
//!   embedded and accessible at runtime (no "os error 2" failures)

use super::*;

const TOOL_NAME: &str = "code_health_refactoring_business_case";
const TEST_FILE: &str = "src/services/order_processor.py";
const TIMEOUT: Duration = Duration::from_secs(60);

const BUSINESS_CASE_TERMS: &[&str] = &[
    "defect",
    "development",
    "optimistic",
    "pessimistic",
    "scenario",
];
const ERROR_PATTERNS: &[&str] = &["no such file or directory", "os error 2", "traceback"];

fn call_business_case(client: &mut MCPClient, repo_dir: &Path) -> String {
    let test_file = repo_dir.join(TEST_FILE);
    let response = client
        .call_tool(
            TOOL_NAME,
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Business case tool call should succeed");
    extract_result_text(&response)
}

fn setup_and_call(command: &[String], env: &[(String, String)], repo_dir: &Path) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    call_business_case(&mut client, repo_dir)
}

pub fn test_business_case_basic_response() {
    let (command, env, repo_dir, _tmp) = setup();
    let result_text = setup_and_call(&command, &env, &repo_dir);

    assert!(
        !result_text.is_empty(),
        "Business case should return content"
    );
}

pub fn test_business_case_contains_metrics() {
    let (command, env, repo_dir, _tmp) = setup();
    let result_text = setup_and_call(&command, &env, &repo_dir);
    let lower = result_text.to_lowercase();

    let terms_found = BUSINESS_CASE_TERMS
        .iter()
        .filter(|term| lower.contains(*term))
        .count();

    assert!(
        terms_found >= 2,
        "Expected at least 2 business case terms, found {terms_found}"
    );
}

pub fn test_business_case_no_file_errors() {
    let (command, env, repo_dir, _tmp) = setup();
    let result_text = setup_and_call(&command, &env, &repo_dir);
    let lower = result_text.to_lowercase();

    for pattern in ERROR_PATTERNS {
        assert!(
            !lower.contains(pattern),
            "Response must not contain error pattern '{pattern}': {result_text}"
        );
    }
}

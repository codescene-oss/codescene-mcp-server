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
//! - An optional, user-selected `target_code_health` overrides the default
//!   incremental target, so users can make the case for e.g. a perfect 10.0
//! - Invalid targets (out of range, or not above the file's current Code
//!   Health) are reported instead of producing a business case

use super::*;
use serde_json::Value;

const TOOL_NAME: &str = "code_health_refactoring_business_case";
const TEST_FILE: &str = "src/services/order_processor.py";
const TIMEOUT: Duration = Duration::from_secs(60);

const OPTIMAL_TARGET: f64 = 10.0;
const OPTIMAL_SCENARIO: &str = "optimal";
const OUT_OF_RANGE_TARGET: f64 = 11.0;
const OUT_OF_RANGE_MESSAGE: &str = "must be between 1 and 10";
// Every file scores at least 1.0, so this target is never above the current score.
const LOWEST_TARGET: f64 = 1.0;
const NOT_ABOVE_CURRENT_MESSAGE: &str = "already meets the target";

const BUSINESS_CASE_TERMS: &[&str] = &[
    "defect",
    "development",
    "optimistic",
    "pessimistic",
    "scenario",
];
const ERROR_PATTERNS: &[&str] = &["no such file or directory", "os error 2", "traceback"];

fn call_business_case(client: &mut MCPClient, arguments: Value) -> String {
    let response = client
        .call_tool(TOOL_NAME, arguments, TIMEOUT)
        .expect("Business case tool call should succeed");
    extract_result_text(&response)
}

fn file_arguments(repo_dir: &Path) -> Value {
    json!({"file_path": repo_dir.join(TEST_FILE).to_string_lossy()})
}

fn setup_and_call_with_target(target: f64) -> String {
    let (command, env, repo_dir, _tmp) = setup();
    let mut arguments = file_arguments(&repo_dir);
    arguments["target_code_health"] = json!(target);
    setup_and_call_with(&command, &env, &repo_dir, arguments)
}

fn setup_and_call_with(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
    arguments: Value,
) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    call_business_case(&mut client, arguments)
}

fn setup_and_call(command: &[String], env: &[(String, String)], repo_dir: &Path) -> String {
    setup_and_call_with(command, env, repo_dir, file_arguments(repo_dir))
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

pub fn test_business_case_user_selected_target() {
    let result_text = setup_and_call_with_target(OPTIMAL_TARGET);

    let case: Value = serde_json::from_str(&result_text)
        .unwrap_or_else(|e| panic!("Expected business case JSON ({e}): {result_text}"));
    assert_eq!(case["target_score"], json!(OPTIMAL_TARGET), "{result_text}");
    assert_eq!(case["scenario"], json!(OPTIMAL_SCENARIO), "{result_text}");
    assert_optimistic_beats_pessimistic(&case, &result_text);
}

fn assert_optimistic_beats_pessimistic(case: &Value, result_text: &str) {
    for metric in ["defect_reduction_percent", "time_reduction_percent"] {
        let optimistic = case["optimistic_outcome"][metric].as_f64().unwrap();
        let pessimistic = case["pessimistic_outcome"][metric].as_f64().unwrap();
        assert!(
            pessimistic > 0.0 && optimistic >= pessimistic,
            "Expected positive reductions with optimistic >= pessimistic for {metric}: {result_text}"
        );
    }
}

pub fn test_business_case_rejects_out_of_range_target() {
    let result_text = setup_and_call_with_target(OUT_OF_RANGE_TARGET);

    assert!(
        result_text.contains(OUT_OF_RANGE_MESSAGE),
        "Expected out-of-range rejection: {result_text}"
    );
}

pub fn test_business_case_target_not_above_current() {
    let result_text = setup_and_call_with_target(LOWEST_TARGET);

    assert!(
        result_text.contains(NOT_ABOVE_CURRENT_MESSAGE),
        "Expected not-above-current explanation: {result_text}"
    );
}

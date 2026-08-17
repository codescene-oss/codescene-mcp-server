//! Pre-commit safeguard integration tests.
//!
//! Tests that the MCP server correctly distinguishes between empty safeguard
//! results caused by clean staged changes and empty safeguard results caused
//! by the absence of any applicable staged changes.
//!
//! Validates:
//! - Clean staged changes report `metadata.status = no-issues-found`
//! - Repositories with no staged changes report `metadata.status = no-files-modified`

use super::*;
use std::process::Command;

const TOOL_NAME: &str = "pre_commit_code_health_safeguard";
const TIMEOUT: Duration = Duration::from_secs(60);
const TEST_FILE: &str = "src/utils/calculator.py";

const CLEAN_ADDITION: &str = r#"

def calculate_median(items: list[float]) -> float:
    """Calculate the median of all items."""
    if not items:
        return 0.0
    sorted_items = sorted(items)
    mid = len(sorted_items) // 2
    if len(sorted_items) % 2 == 0:
        return (sorted_items[mid - 1] + sorted_items[mid]) / 2
    return sorted_items[mid]
"#;

fn git(repo_dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .expect("git command should execute");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn parse_quality_gates(result_text: &str) -> Option<String> {
    let data: serde_json::Value = serde_json::from_str(result_text).ok()?;
    data.get("quality_gates")?.as_str().map(String::from)
}

fn parse_metadata_status(result_text: &str) -> Option<String> {
    let data: serde_json::Value = serde_json::from_str(result_text).ok()?;
    data.get("metadata")?
        .get("status")?
        .as_str()
        .map(String::from)
}

fn parse_result_count(result_text: &str) -> Option<usize> {
    let data: serde_json::Value = serde_json::from_str(result_text).ok()?;
    data.get("results")?.as_array().map(Vec::len)
}

fn parse_metadata_count(result_text: &str, key: &str) -> Option<u64> {
    let data: serde_json::Value = serde_json::from_str(result_text).ok()?;
    data.get("metadata")?.get(key)?.as_u64()
}

fn run_pre_commit_safeguard(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
) -> (String, Option<String>) {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            TOOL_NAME,
            json!({"git_repository_path": repo_dir.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("pre_commit_code_health_safeguard tool call should succeed");

    let result_text = extract_result_text(&response);
    let quality_gates = parse_quality_gates(&result_text);
    (result_text, quality_gates)
}

fn stage_clean_change(repo_dir: &Path) {
    let test_file = repo_dir.join(TEST_FILE);
    let original = std::fs::read_to_string(&test_file).expect("Read original file");
    std::fs::write(&test_file, format!("{original}{CLEAN_ADDITION}")).expect("Write clean change");
    git(repo_dir, &["add", TEST_FILE]);
}

fn assert_empty_results_passed_with_status(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
    expected_status: &str,
) {
    let (result_text, quality_gates) = run_pre_commit_safeguard(command, env, repo_dir);

    assert!(!result_text.is_empty(), "Tool should return content");
    assert_eq!(quality_gates.as_deref(), Some("passed"));
    assert_eq!(parse_result_count(&result_text), Some(0));
    assert_eq!(
        parse_metadata_status(&result_text).as_deref(),
        Some(expected_status),
        "Expected empty results to expose metadata.status={expected_status}: {result_text}"
    );
}

fn assert_empty_results_passed_without_checked_files(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
) {
    let (result_text, quality_gates) = run_pre_commit_safeguard(command, env, repo_dir);

    assert!(!result_text.is_empty(), "Tool should return content");
    assert_eq!(quality_gates.as_deref(), Some("passed"));
    assert_eq!(parse_result_count(&result_text), Some(0));
    assert_eq!(
        parse_metadata_count(&result_text, "checked-file-count"),
        Some(0)
    );
    assert_eq!(
        parse_metadata_count(&result_text, "code-health-eligible-file-count"),
        Some(0)
    );

    let status = parse_metadata_status(&result_text);
    assert!(
        matches!(
            status.as_deref(),
            Some("no-files-modified") | Some("no-applicable-changes")
        ),
        "Expected empty results with no checked files to expose metadata.status=no-files-modified or no-applicable-changes: {result_text}"
    );
}

pub fn test_reports_no_issues_found_for_clean_staged_changes() {
    let (command, env, repo_dir, _tmp) = setup();
    stage_clean_change(&repo_dir);
    assert_empty_results_passed_with_status(&command, &env, &repo_dir, "no-issues-found");
}

pub fn test_reports_no_files_modified_for_empty_staging_area() {
    let (command, env, repo_dir, _tmp) = setup();
    assert_empty_results_passed_without_checked_files(&command, &env, &repo_dir);
}
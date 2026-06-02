//! Integration tests for the `analyze_change_set` MCP tool.
//!
//! Tests that branch-level Code Health analysis correctly:
//! - Passes when no code health decline exists on the current branch vs base_ref
//! - Fails when a commit on the current branch introduces a code health decline
//! - Fails when a new file on the branch introduces code health issues
//! - Passes when a new file on the branch has clean code health

use super::*;
use std::process::Command;

const TOOL_NAME: &str = "analyze_change_set";
const BASE_REF: &str = "master";
const TIMEOUT: Duration = Duration::from_secs(60);

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

const DEGRADING_ADDITION: &str = r#"

def validate_order(order, customer, inventory, config):
    """Validate an order with complex business rules."""
    if (order is not None and customer is not None and inventory is not None
            and config is not None and order.get("items") and customer.get("id")
            and inventory.get("stock") and config.get("enabled")
            and order.get("total") > 0 and customer.get("active")
            and not customer.get("banned") and config.get("allow_orders")):
        return True
    if (order is not None and order.get("priority") and customer is not None
            and customer.get("vip") and inventory is not None
            and inventory.get("reserved") and config is not None
            and config.get("vip_enabled") and order.get("total") > 100
            and not order.get("flagged") and customer.get("verified")
            and config.get("allow_vip")):
        return True
    return False
"#;

const DEGRADING_NEW_FILE: &str = r#""""
Validation module with complex business rules.
"""


def validate_order(order, customer, inventory, config):
    """Validate an order with complex business rules."""
    if (order is not None and customer is not None and inventory is not None
            and config is not None and order.get("items") and customer.get("id")
            and inventory.get("stock") and config.get("enabled")
            and order.get("total") > 0 and customer.get("active")
            and not customer.get("banned") and config.get("allow_orders")):
        return True
    if (order is not None and order.get("priority") and customer is not None
            and customer.get("vip") and inventory is not None
            and inventory.get("reserved") and config is not None
            and config.get("vip_enabled") and order.get("total") > 100
            and not order.get("flagged") and customer.get("verified")
            and config.get("allow_vip")):
        return True
    return False
"#;

const CLEAN_NEW_FILE: &str = r#""""
Simple statistics utility module.
"""


def calculate_median(items: list[float]) -> float:
    """Calculate the median of all items."""
    if not items:
        return 0.0
    sorted_items = sorted(items)
    mid = len(sorted_items) // 2
    if len(sorted_items) % 2 == 0:
        return (sorted_items[mid - 1] + sorted_items[mid]) / 2
    return sorted_items[mid]


def calculate_mode(items: list[float]) -> float:
    """Calculate the mode of all items."""
    if not items:
        return 0.0
    counts: dict[float, int] = {}
    for item in items:
        counts[item] = counts.get(item, 0) + 1
    return max(counts, key=counts.get)
"#;

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

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

fn create_feature_branch_with_file_change(repo_dir: &Path, file_path: &str, additional_code: &str) {
    git(repo_dir, &["checkout", "-b", "feature"]);

    let full_path = repo_dir.join(file_path);
    let original = std::fs::read_to_string(&full_path).expect("Read original file");
    std::fs::write(&full_path, format!("{original}{additional_code}")).expect("Write modified file");

    git(repo_dir, &["add", "."]);
    git(repo_dir, &["commit", "-m", "Feature branch change"]);
}

fn create_feature_branch_with_new_file(repo_dir: &Path, file_path: &str, content: &str) {
    git(repo_dir, &["checkout", "-b", "feature"]);

    let full_path = repo_dir.join(file_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("Create parent directories");
    }
    std::fs::write(&full_path, content).expect("Write new file");

    git(repo_dir, &["add", "."]);
    git(repo_dir, &["commit", "-m", "Add new file on feature branch"]);
}

// ---------------------------------------------------------------------------
// Analysis helpers
// ---------------------------------------------------------------------------

fn parse_quality_gates(result_text: &str) -> Option<String> {
    let data: serde_json::Value = serde_json::from_str(result_text).ok()?;
    data.get("quality_gates")?.as_str().map(String::from)
}

fn run_change_set_analysis(
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
            json!({
                "base_ref": BASE_REF,
                "git_repository_path": repo_dir.to_string_lossy()
            }),
            TIMEOUT,
        )
        .expect("analyze_change_set tool call should succeed");

    let result_text = extract_result_text(&response);
    let quality_gates = parse_quality_gates(&result_text);
    (result_text, quality_gates)
}

// ---------------------------------------------------------------------------
// Local setup — each test gets its own temp dir and git repo
// ---------------------------------------------------------------------------

fn local_setup() -> (Vec<String>, Vec<(String, String)>, std::path::PathBuf, tempfile::TempDir) {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_changeset_").expect("Failed to create temp dir");
    let sample_files = get_sample_files();
    let repo_dir =
        create_git_repo(temp_dir.path(), &sample_files).expect("Failed to create git repo");

    let base = base_env();
    let env_map = backend.get_env(&base, &repo_dir);
    let env_vec: Vec<(String, String)> = env_map.into_iter().collect();
    let command = backend.get_command(&repo_dir);

    (command, env_vec, repo_dir, temp_dir)
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

fn assert_quality_gates_passed(command: &[String], env: &[(String, String)], repo_dir: &Path) {
    let (result_text, quality_gates) = run_change_set_analysis(command, env, repo_dir);

    assert!(!result_text.is_empty(), "Tool should return content");
    assert_eq!(
        quality_gates.as_deref(),
        Some("passed"),
        "Quality gates should pass, got: {quality_gates:?}"
    );
}

fn assert_quality_gates_failed(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
    expected_file: &str,
) {
    let (result_text, quality_gates) = run_change_set_analysis(command, env, repo_dir);

    assert_eq!(
        quality_gates.as_deref(),
        Some("failed"),
        "Quality gates should fail, got: {quality_gates:?}"
    );
    assert!(
        result_text.contains(expected_file),
        "Findings should reference {expected_file}"
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

pub fn test_passes_on_clean_branch() {
    let (command, env, repo_dir, _tmp) = local_setup();
    create_feature_branch_with_file_change(&repo_dir, "src/utils/calculator.py", CLEAN_ADDITION);
    assert_quality_gates_passed(&command, &env, &repo_dir);
}

pub fn test_fails_on_degraded_branch() {
    let (command, env, repo_dir, _tmp) = local_setup();
    create_feature_branch_with_file_change(&repo_dir, "src/utils/calculator.py", DEGRADING_ADDITION);
    assert_quality_gates_failed(&command, &env, &repo_dir, "calculator.py");
}

pub fn test_fails_on_new_file_with_degraded_health() {
    let (command, env, repo_dir, _tmp) = local_setup();
    create_feature_branch_with_new_file(&repo_dir, "src/validation/validator.py", DEGRADING_NEW_FILE);
    assert_quality_gates_failed(&command, &env, &repo_dir, "validator.py");
}

pub fn test_passes_on_new_file_with_clean_health() {
    let (command, env, repo_dir, _tmp) = local_setup();
    create_feature_branch_with_new_file(&repo_dir, "src/stats/statistics.py", CLEAN_NEW_FILE);
    assert_quality_gates_passed(&command, &env, &repo_dir);
}

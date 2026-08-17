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
const CALCULATOR_PATH: &str = "src/utils/calculator.py";
const VALIDATOR_PATH: &str = "src/validation/validator.py";
const STATISTICS_PATH: &str = "src/stats/statistics.py";

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

enum ExistingFileChange {
    CleanCalculator,
    DegradingCalculator,
}

impl ExistingFileChange {
    fn file_path(&self) -> &'static str {
        CALCULATOR_PATH
    }

    fn additional_code(&self) -> &'static str {
        match self {
            Self::CleanCalculator => CLEAN_ADDITION,
            Self::DegradingCalculator => DEGRADING_ADDITION,
        }
    }
}

enum NewFileScenario {
    DegradingValidation,
    CleanStatistics,
}

impl NewFileScenario {
    fn file_path(&self) -> &'static str {
        match self {
            Self::DegradingValidation => VALIDATOR_PATH,
            Self::CleanStatistics => STATISTICS_PATH,
        }
    }

    fn content(&self) -> &'static str {
        match self {
            Self::DegradingValidation => DEGRADING_NEW_FILE,
            Self::CleanStatistics => CLEAN_NEW_FILE,
        }
    }
}

enum ExpectedFile {
    Calculator,
    Validator,
}

impl ExpectedFile {
    fn file_name(&self) -> &'static str {
        match self {
            Self::Calculator => "calculator.py",
            Self::Validator => "validator.py",
        }
    }
}

enum MetadataStatus {
    NoIssuesFound,
    NoFilesModified,
}

impl MetadataStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::NoIssuesFound => "no-issues-found",
            Self::NoFilesModified => "no-files-modified",
        }
    }
}

struct TestContext {
    command: Vec<String>,
    env: Vec<(String, String)>,
    repo_dir: std::path::PathBuf,
    _temp_dir: tempfile::TempDir,
}

struct AnalysisOutput {
    result_text: String,
    data: serde_json::Value,
}

impl AnalysisOutput {
    fn quality_gates(&self) -> Option<&str> {
        self.data.get("quality_gates")?.as_str()
    }

    fn metadata_status(&self) -> Option<&str> {
        self.data.get("metadata")?.get("status")?.as_str()
    }

    fn result_count(&self) -> Option<usize> {
        self.data.get("results")?.as_array().map(Vec::len)
    }
}

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

fn create_feature_branch_with_file_change(repo_dir: &Path, change: ExistingFileChange) {
    git(repo_dir, &["checkout", "-b", "feature"]);

    let full_path = repo_dir.join(change.file_path());
    let original = std::fs::read_to_string(&full_path).expect("Read original file");
    std::fs::write(&full_path, format!("{original}{}", change.additional_code()))
        .expect("Write modified file");

    git(repo_dir, &["add", "."]);
    git(repo_dir, &["commit", "-m", "Feature branch change"]);
}

fn create_feature_branch_with_new_file(repo_dir: &Path, scenario: NewFileScenario) {
    git(repo_dir, &["checkout", "-b", "feature"]);

    let full_path = repo_dir.join(scenario.file_path());
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("Create parent directories");
    }
    std::fs::write(&full_path, scenario.content()).expect("Write new file");

    git(repo_dir, &["add", "."]);
    git(
        repo_dir,
        &["commit", "-m", "Add new file on feature branch"],
    );
}

fn create_feature_branch_without_changes(repo_dir: &Path) {
    git(repo_dir, &["checkout", "-b", "feature"]);
}

// ---------------------------------------------------------------------------
// Analysis helpers
// ---------------------------------------------------------------------------

fn run_change_set_analysis(context: &TestContext) -> AnalysisOutput {
    let mut client = make_client(&context.command, &context.env, &context.repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            TOOL_NAME,
            json!({
                "base_ref": BASE_REF,
                "git_repository_path": context.repo_dir.to_string_lossy()
            }),
            TIMEOUT,
        )
        .expect("analyze_change_set tool call should succeed");

    let result_text = extract_result_text(&response);
    let data = serde_json::from_str(&result_text).expect("Result should be valid JSON");
    AnalysisOutput { result_text, data }
}

// ---------------------------------------------------------------------------
// Local setup — each test gets its own temp dir and git repo
// ---------------------------------------------------------------------------

fn local_setup() -> TestContext {
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

    TestContext {
        command,
        env: env_vec,
        repo_dir,
        _temp_dir: temp_dir,
    }
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

fn assert_quality_gates_passed(context: &TestContext) {
    let output = run_change_set_analysis(context);

    assert!(!output.result_text.is_empty(), "Tool should return content");
    assert_eq!(
        output.quality_gates(),
        Some("passed"),
        "Quality gates should pass, got: {:?}",
        output.quality_gates()
    );
}

fn assert_quality_gates_failed(context: &TestContext, expected_file: ExpectedFile) {
    let output = run_change_set_analysis(context);

    assert_eq!(
        output.quality_gates(),
        Some("failed"),
        "Quality gates should fail, got: {:?}",
        output.quality_gates()
    );
    assert!(
        output.result_text.contains(expected_file.file_name()),
        "Findings should reference {}",
        expected_file.file_name()
    );
}

fn assert_empty_results_passed_with_status(context: &TestContext, expected_status: MetadataStatus) {
    let output = run_change_set_analysis(context);

    assert!(!output.result_text.is_empty(), "Tool should return content");
    assert_eq!(output.quality_gates(), Some("passed"));
    assert_eq!(output.result_count(), Some(0));
    assert_eq!(
        output.metadata_status(),
        Some(expected_status.as_str()),
        "Expected empty results to expose metadata.status={}: {}",
        expected_status.as_str(),
        output.result_text
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

pub fn test_passes_on_clean_branch() {
    let context = local_setup();
    create_feature_branch_with_file_change(&context.repo_dir, ExistingFileChange::CleanCalculator);
    assert_quality_gates_passed(&context);
}

pub fn test_fails_on_degraded_branch() {
    let context = local_setup();
    create_feature_branch_with_file_change(
        &context.repo_dir,
        ExistingFileChange::DegradingCalculator,
    );
    assert_quality_gates_failed(&context, ExpectedFile::Calculator);
}

pub fn test_fails_on_new_file_with_degraded_health() {
    let context = local_setup();
    create_feature_branch_with_new_file(&context.repo_dir, NewFileScenario::DegradingValidation);
    assert_quality_gates_failed(&context, ExpectedFile::Validator);
}

pub fn test_passes_on_new_file_with_clean_health() {
    let context = local_setup();
    create_feature_branch_with_new_file(&context.repo_dir, NewFileScenario::CleanStatistics);
    assert_quality_gates_passed(&context);
}

pub fn test_reports_no_issues_found_for_clean_change_set() {
    let context = local_setup();
    create_feature_branch_with_file_change(&context.repo_dir, ExistingFileChange::CleanCalculator);
    assert_empty_results_passed_with_status(&context, MetadataStatus::NoIssuesFound);
}

pub fn test_reports_no_files_modified_for_empty_change_set() {
    let context = local_setup();
    create_feature_branch_without_changes(&context.repo_dir);
    assert_empty_results_passed_with_status(&context, MetadataStatus::NoFilesModified);
}

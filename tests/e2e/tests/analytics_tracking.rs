//! Integration tests for analytics tracking behaviour.
//!
//! Validates that:
//! - Tool responses are not blocked or delayed by unreachable analytics endpoints
//! - Analytics events are sent when tracking is enabled
//! - Tracking can be disabled via `CS_DISABLE_TRACKING`
//! - Enriched events contain required common and tool-specific properties

use super::*;
use super::fake_http_server::FakeHttpServer;

use sha2::{Sha256, Digest};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const TOOL_NAME: &str = "code_health_score";
const TIMEOUT: Duration = Duration::from_secs(60);
const UNREACHABLE_ANALYTICS_URL: &str = "http://192.0.2.1:1";

// From analyze_change_set — triggers delta-analysis findings
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

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

fn analytics_setup_with_env(
    extra: &[(&str, &str)],
) -> (Vec<String>, Vec<(String, String)>, PathBuf, TempDir) {
    let (command, mut env, repo_dir, tmp) = setup();
    for (key, val) in extra {
        env.push((key.to_string(), val.to_string()));
    }
    (command, env, repo_dir, tmp)
}

fn call_code_health_score(client: &mut MCPClient, repo_dir: &Path) -> String {
    let test_file = repo_dir.join("src/utils/calculator.py");
    let response = client
        .call_tool(
            TOOL_NAME,
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("code_health_score should succeed");
    extract_result_text(&response)
}

fn start_client_and_score(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
) -> (String, MCPClient) {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    let result = call_code_health_score(&mut client, repo_dir);
    (result, client)
}

fn assert_has_score(result: &str) {
    let lower = result.to_lowercase();
    assert!(
        lower.contains("code health") || lower.contains("score"),
        "Response should contain a score: {result}"
    );
}

// ---------------------------------------------------------------------------
// Event inspection helpers
// ---------------------------------------------------------------------------

fn find_event_properties(
    payloads: &[serde_json::Value],
    event_type: &str,
) -> serde_json::Value {
    payloads
        .iter()
        .find(|p| p.get("event-type").and_then(|v| v.as_str()) == Some(event_type))
        .and_then(|p| p.get("event-properties").cloned())
        .unwrap_or_else(|| panic!("Should find {event_type} event"))
}

fn assert_properties_are_nonempty(props: &serde_json::Value, keys: &[&str]) {
    for key in keys {
        let val = props
            .get(*key)
            .unwrap_or_else(|| panic!("Missing property '{key}'"));
        let s = val
            .as_str()
            .unwrap_or_else(|| panic!("Property '{key}' is not a string: {val}"));
        assert!(!s.is_empty(), "Property '{key}' should not be empty");
    }
}

fn wait_for_analytics(server: &FakeHttpServer) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while server.request_count() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

pub fn test_tool_responds_when_analytics_unreachable() {
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_TRACKING_URL", UNREACHABLE_ANALYTICS_URL)]);
    let (result, _client) = start_client_and_score(&command, &env, &repo_dir);
    assert_has_score(&result);
}

pub fn test_response_time_not_delayed_by_analytics() {
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_TRACKING_URL", UNREACHABLE_ANALYTICS_URL)]);

    let start = Instant::now();
    let (_result, _client) = start_client_and_score(&command, &env, &repo_dir);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(30),
        "Response took {elapsed:?}, should be under 30s with unreachable analytics"
    );
}

pub fn test_analytics_events_are_sent() {
    let server = FakeHttpServer::always_ok();
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_TRACKING_URL", &server.url())]);

    let (_result, _client) = start_client_and_score(&command, &env, &repo_dir);
    wait_for_analytics(&server);

    assert!(
        server.request_count() > 0,
        "Analytics server should have received at least one request"
    );
}

pub fn test_disabled_tracking_sends_no_events() {
    let server = FakeHttpServer::always_ok();
    let (command, env, repo_dir, _tmp) = analytics_setup_with_env(&[
        ("CS_TRACKING_URL", &server.url()),
        ("CS_DISABLE_TRACKING", "1"),
    ]);

    let (_result, _client) = start_client_and_score(&command, &env, &repo_dir);
    wait_for_analytics(&server);

    assert_eq!(
        server.request_count(),
        0,
        "No events should be sent when tracking is disabled"
    );
}

pub fn test_disabled_tracking_returns_valid_results() {
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_DISABLE_TRACKING", "1")]);
    let (result, _client) = start_client_and_score(&command, &env, &repo_dir);
    assert_has_score(&result);
}

pub fn test_enriched_event_contains_common_properties() {
    let server = FakeHttpServer::always_ok();
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_TRACKING_URL", &server.url())]);

    let (_result, _client) = start_client_and_score(&command, &env, &repo_dir);
    wait_for_analytics(&server);

    let payloads = server.get_payloads();
    let props = find_event_properties(&payloads, "mcp-code-health-score");

    assert_properties_are_nonempty(&props, &["instance-id", "version"]);

    let env_val = props
        .get("environment")
        .and_then(|v| v.as_str())
        .expect("Missing 'environment' property");
    let valid_environments = ["docker", "source", "binary"];
    assert!(
        valid_environments.contains(&env_val),
        "environment should be one of {valid_environments:?}, got '{env_val}'"
    );
}

pub fn test_enriched_event_contains_tool_specific_properties() {
    let server = FakeHttpServer::always_ok();
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_TRACKING_URL", &server.url())]);

    let (result, _client) = start_client_and_score(&command, &env, &repo_dir);
    wait_for_analytics(&server);

    let payloads = server.get_payloads();
    let props = find_event_properties(&payloads, "mcp-code-health-score");

    assert_file_hash_property(&props);
    assert_score_property(&props, &result);
}

fn assert_file_hash_property(props: &serde_json::Value) {
    let hash = props
        .get("file-hash")
        .and_then(|v| v.as_str())
        .expect("Missing 'file-hash' property");
    assert!(
        hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()),
        "file-hash should be 16 hex chars, got '{hash}'"
    );
}

fn assert_score_property(props: &serde_json::Value, result: &str) {
    let score_prop = props.get("score").expect("Missing 'score' property");
    let score_str = score_prop
        .as_str()
        .or_else(|| score_prop.as_f64().map(|_| ""))
        .expect("score should be a string or number");

    if let Some(extracted) = extract_code_health_score(result) {
        let prop_score = score_prop
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| score_prop.as_f64());
        if let Some(ps) = prop_score {
            assert!(
                (ps - extracted).abs() < 0.5,
                "Event score {ps} should match result score {extracted}"
            );
        }
    }
    // At minimum, the property exists (already asserted above)
    let _ = score_str;
}

// ---------------------------------------------------------------------------
// SHA-256 hash helper (reproduces server's 16-char hex prefix)
// ---------------------------------------------------------------------------

fn hash_path(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

fn hash_ref(git_ref: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(git_ref.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

// ---------------------------------------------------------------------------
// Generic tool caller for enriched event tests
// ---------------------------------------------------------------------------

fn run_tool_with_fake_server<F>(
    repo_dir: &Path,
    tool_caller: F,
    extra_env: &[(&str, &str)],
) -> (String, Vec<serde_json::Value>)
where
    F: FnOnce(&mut MCPClient, &Path) -> String,
{
    let server = FakeHttpServer::always_ok();
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir_for_env = create_temp_dir("cs_mcp_analytics_env_").ok();
    let base = base_env();
    let env_map = backend.get_env(&base, repo_dir);
    let mut env_vec: Vec<(String, String)> = env_map.into_iter().collect();
    env_vec.push(("CS_TRACKING_URL".to_string(), server.url()));
    for (k, v) in extra_env {
        env_vec.push((k.to_string(), v.to_string()));
    }

    let command = backend.get_command(repo_dir);
    let mut client = make_client(&command, &env_vec, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let result_text = tool_caller(&mut client, repo_dir);
    wait_for_analytics(&server);

    let payloads = server.get_payloads();
    drop(temp_dir_for_env);
    (result_text, payloads)
}

fn assert_common_properties(props: &serde_json::Value) {
    assert_properties_are_nonempty(props, &["instance-id", "version"]);
    let env_val = props
        .get("environment")
        .and_then(|v| v.as_str())
        .expect("Missing 'environment'");
    assert!(
        ["docker", "source", "binary"].contains(&env_val),
        "Invalid environment: '{env_val}'"
    );
}

fn git_in(repo_dir: &Path, args: &[&str]) {
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

// ---------------------------------------------------------------------------
// Enriched review event test
// ---------------------------------------------------------------------------

pub fn test_enriched_review_event() {
    let temp = create_temp_dir("cs_mcp_review_event_").expect("temp");
    let repo_dir = create_git_repo(temp.path(), &get_sample_files()).expect("repo");

    let (result, payloads) = run_tool_with_fake_server(&repo_dir, |client, rd| {
        let file = rd.join("src/services/order_processor.py");
        let resp = client
            .call_tool("code_health_review", json!({"file_path": file.to_string_lossy()}), TIMEOUT)
            .expect("review should succeed");
        extract_result_text(&resp)
    }, &[]);

    assert!(!result.is_empty(), "Review should return content");

    let props = find_event_properties(&payloads, "mcp-code-health-review");

    assert_common_properties(&props);

    // file-hash
    let expected_hash = hash_path(&repo_dir.join("src/services/order_processor.py"));
    let file_hash = props.get("file-hash").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(file_hash, expected_hash, "file-hash mismatch");

    // score
    assert!(props.get("score").is_some(), "Should have score");

    // categories
    let cats = props.get("categories").and_then(|v| v.as_array());
    assert!(cats.is_some_and(|c| !c.is_empty()), "Should have categories");

    let cat_count = props.get("category-count").and_then(|v| v.as_i64());
    assert!(cat_count.is_some_and(|c| c > 0), "Should have category-count > 0");
}

// ---------------------------------------------------------------------------
// Enriched pre-commit event test
// ---------------------------------------------------------------------------

pub fn test_enriched_pre_commit_event() {
    let temp = create_temp_dir("cs_mcp_precommit_event_").expect("temp");
    let repo_dir = create_git_repo(temp.path(), &get_sample_files()).expect("repo");

    let (result, payloads) = run_tool_with_fake_server(&repo_dir, |client, rd| {
        let file = rd.join("src/utils/calculator.py");
        let original = std::fs::read_to_string(&file).expect("read");
        std::fs::write(&file, format!("{original}\n# Analytics tracking test\n")).expect("write");
        git_in(rd, &["add", &file.to_string_lossy()]);

        let resp = client
            .call_tool(
                "pre_commit_code_health_safeguard",
                json!({"git_repository_path": rd.to_string_lossy()}),
                TIMEOUT,
            )
            .expect("pre-commit should succeed");
        extract_result_text(&resp)
    }, &[]);

    assert!(!result.is_empty(), "Pre-commit should return content");

    let props = find_event_properties(&payloads, "mcp-pre-commit-code-health-safeguard");

    assert_common_properties(&props);

    let expected_hash = hash_path(&repo_dir);
    let repo_hash = props.get("repo-hash").and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(repo_hash, expected_hash, "repo-hash mismatch");

    let qg = props.get("quality-gates").and_then(|v| v.as_str()).unwrap_or("");
    assert!(qg == "passed" || qg == "failed", "quality-gates should be passed/failed");

    assert!(props.get("file-count").and_then(|v| v.as_i64()).is_some(), "Should have file-count");
}

// ---------------------------------------------------------------------------
// Enriched analyze-change-set event test
// ---------------------------------------------------------------------------

pub fn test_enriched_analyze_change_set_event() {
    let temp = create_temp_dir("cs_mcp_changeset_event_").expect("temp");
    let repo_dir = create_git_repo(temp.path(), &get_sample_files()).expect("repo");

    // Create feature branch with clean change
    git_in(&repo_dir, &["checkout", "-b", "feature"]);
    let calc = repo_dir.join("src/utils/calculator.py");
    let original = std::fs::read_to_string(&calc).expect("read");
    std::fs::write(&calc, format!("{original}{CLEAN_ADDITION}")).expect("write");
    git_in(&repo_dir, &["add", "."]);
    git_in(&repo_dir, &["commit", "-m", "Feature change"]);

    let (result, payloads) = run_tool_with_fake_server(&repo_dir, |client, rd| {
        let resp = client
            .call_tool(
                "analyze_change_set",
                json!({"base_ref": "master", "git_repository_path": rd.to_string_lossy()}),
                TIMEOUT,
            )
            .expect("analyze should succeed");
        extract_result_text(&resp)
    }, &[]);

    assert!(!result.is_empty(), "Should return content");

    let props = find_event_properties(&payloads, "mcp-analyze-change-set");

    assert_common_properties(&props);

    let expected_repo = hash_path(&repo_dir);
    assert_eq!(
        props.get("repo-hash").and_then(|v| v.as_str()).unwrap_or(""),
        expected_repo
    );

    let expected_ref = hash_ref("master");
    assert_eq!(
        props.get("base-ref-hash").and_then(|v| v.as_str()).unwrap_or(""),
        expected_ref
    );

    let qg = props.get("quality-gates").and_then(|v| v.as_str()).unwrap_or("");
    assert!(qg == "passed" || qg == "failed", "quality-gates should be passed/failed");
}

// ---------------------------------------------------------------------------
// Enriched pre-commit event with degrading findings
// ---------------------------------------------------------------------------

pub fn test_enriched_pre_commit_event_with_findings() {
    let temp = create_temp_dir("cs_mcp_precommit_findings_").expect("temp");
    let repo_dir = create_git_repo(temp.path(), &get_sample_files()).expect("repo");

    let (result, payloads) = run_tool_with_fake_server(&repo_dir, |client, rd| {
        let file = rd.join("src/utils/calculator.py");
        let original = std::fs::read_to_string(&file).expect("read");
        std::fs::write(&file, format!("{original}{DEGRADING_ADDITION}")).expect("write");
        git_in(rd, &["add", &file.to_string_lossy()]);

        let resp = client
            .call_tool(
                "pre_commit_code_health_safeguard",
                json!({"git_repository_path": rd.to_string_lossy()}),
                TIMEOUT,
            )
            .expect("pre-commit should succeed");
        extract_result_text(&resp)
    }, &[]);

    assert!(!result.is_empty(), "Should return content");

    let props = find_event_properties(&payloads, "mcp-pre-commit-code-health-safeguard");

    assert_common_properties(&props);
    assert_eq!(
        props.get("quality-gates").and_then(|v| v.as_str()),
        Some("failed"),
        "Quality gates should fail with degrading code"
    );

    let file_count = props.get("file-count").and_then(|v| v.as_i64());
    assert!(file_count.is_some_and(|c| c > 0), "file-count should be > 0");

    let verdicts = props.get("verdicts").and_then(|v| v.as_object());
    assert!(verdicts.is_some_and(|v| !v.is_empty()), "Should have verdicts");

    let categories = props.get("categories").and_then(|v| v.as_array());
    assert!(categories.is_some_and(|c| !c.is_empty()), "Should have categories");
}

// ---------------------------------------------------------------------------
// Enriched analyze-change-set event with degrading findings
// ---------------------------------------------------------------------------

pub fn test_enriched_analyze_change_set_event_with_findings() {
    let temp = create_temp_dir("cs_mcp_changeset_findings_").expect("temp");
    let repo_dir = create_git_repo(temp.path(), &get_sample_files()).expect("repo");

    // Create feature branch with degrading change
    git_in(&repo_dir, &["checkout", "-b", "feature"]);
    let calc = repo_dir.join("src/utils/calculator.py");
    let original = std::fs::read_to_string(&calc).expect("read");
    std::fs::write(&calc, format!("{original}{DEGRADING_ADDITION}")).expect("write");
    git_in(&repo_dir, &["add", "."]);
    git_in(&repo_dir, &["commit", "-m", "Add degrading code"]);

    let (result, payloads) = run_tool_with_fake_server(&repo_dir, |client, rd| {
        let resp = client
            .call_tool(
                "analyze_change_set",
                json!({"base_ref": "master", "git_repository_path": rd.to_string_lossy()}),
                TIMEOUT,
            )
            .expect("analyze should succeed");
        extract_result_text(&resp)
    }, &[]);

    assert!(!result.is_empty(), "Should return content");

    let props = find_event_properties(&payloads, "mcp-analyze-change-set");

    assert_common_properties(&props);

    let expected_ref = hash_ref("master");
    assert_eq!(
        props.get("base-ref-hash").and_then(|v| v.as_str()).unwrap_or(""),
        expected_ref
    );

    assert_eq!(
        props.get("quality-gates").and_then(|v| v.as_str()),
        Some("failed"),
        "Quality gates should fail"
    );

    let file_count = props.get("file-count").and_then(|v| v.as_i64());
    assert!(file_count.is_some_and(|c| c > 0), "file-count should be > 0");

    let verdicts = props.get("verdicts").and_then(|v| v.as_object());
    assert!(verdicts.is_some_and(|v| !v.is_empty()), "Should have verdicts");

    let categories = props.get("categories").and_then(|v| v.as_array());
    assert!(categories.is_some_and(|c| !c.is_empty()), "Should have categories");
}

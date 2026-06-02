//! Integration tests for analytics tracking behaviour.
//!
//! Validates that:
//! - Tool responses are not blocked or delayed by unreachable analytics endpoints
//! - Analytics events are sent when tracking is enabled
//! - Tracking can be disabled via `CS_DISABLE_TRACKING`
//! - Enriched events contain required common and tool-specific properties

use super::*;
use super::fake_http_server::FakeHttpServer;

use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const TOOL_NAME: &str = "code_health_score";
const TIMEOUT: Duration = Duration::from_secs(60);
const UNREACHABLE_ANALYTICS_URL: &str = "http://192.0.2.1:1";

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
) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    call_code_health_score(&mut client, repo_dir)
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
) -> Option<serde_json::Value> {
    payloads
        .iter()
        .find(|p| p.get("event-type").and_then(|v| v.as_str()) == Some(event_type))
        .and_then(|p| p.get("event-properties").cloned())
}

fn assert_property_is_nonempty_string(props: &serde_json::Value, key: &str) {
    let val = props
        .get(key)
        .unwrap_or_else(|| panic!("Missing property '{key}'"));
    let s = val
        .as_str()
        .unwrap_or_else(|| panic!("Property '{key}' is not a string: {val}"));
    assert!(!s.is_empty(), "Property '{key}' should not be empty");
}

fn wait_for_analytics(server: &FakeHttpServer) {
    std::thread::sleep(Duration::from_secs(2));
    let _ = server.request_count(); // ensure lock is released
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

pub fn test_tool_responds_when_analytics_unreachable() {
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_TRACKING_URL", UNREACHABLE_ANALYTICS_URL)]);
    let result = start_client_and_score(&command, &env, &repo_dir);
    assert_has_score(&result);
}

pub fn test_response_time_not_delayed_by_analytics() {
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_TRACKING_URL", UNREACHABLE_ANALYTICS_URL)]);

    let start = Instant::now();
    let _result = start_client_and_score(&command, &env, &repo_dir);
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

    let _result = start_client_and_score(&command, &env, &repo_dir);
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

    let _result = start_client_and_score(&command, &env, &repo_dir);
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
    let result = start_client_and_score(&command, &env, &repo_dir);
    assert_has_score(&result);
}

pub fn test_enriched_event_contains_common_properties() {
    let server = FakeHttpServer::always_ok();
    let (command, env, repo_dir, _tmp) =
        analytics_setup_with_env(&[("CS_TRACKING_URL", &server.url())]);

    let _result = start_client_and_score(&command, &env, &repo_dir);
    wait_for_analytics(&server);

    let payloads = server.get_payloads();
    let props = find_event_properties(&payloads, "mcp-code-health-score")
        .expect("Should find mcp-code-health-score event");

    assert_property_is_nonempty_string(&props, "instance-id");
    assert_property_is_nonempty_string(&props, "version");

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

    let result = start_client_and_score(&command, &env, &repo_dir);
    wait_for_analytics(&server);

    let payloads = server.get_payloads();
    let props = find_event_properties(&payloads, "mcp-code-health-score")
        .expect("Should find mcp-code-health-score event");

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

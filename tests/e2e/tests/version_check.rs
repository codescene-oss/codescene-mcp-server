//! Version check integration tests.
//!
//! Validates that MCP tool calls work correctly both when the GitHub
//! version check endpoint is unreachable and when a local fake server
//! reports a newer version available.

use super::*;
use super::fake_http_server::FakeHttpServer;

use std::time::{Duration, Instant};

/// RFC 5737 non-routable address — guaranteed to be unreachable.
const UNREACHABLE_URL: &str = "http://192.0.2.1:1/fake";

/// Fake version that will always differ from the server's real version.
const FAKE_LATEST_VERSION: &str = "MCP-99.99.99";

/// Build a version-check-specific environment.
///
/// Removes `CS_DISABLE_VERSION_CHECK` from the base env and sets
/// `CS_VERSION_CHECK_URL` to the given `url`.
fn version_check_setup_with_url(url: &str) -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let (command, env, repo_dir, tmp) = setup();
    let mut env: Vec<(String, String)> = env
        .into_iter()
        .filter(|(k, _)| k != "CS_DISABLE_VERSION_CHECK")
        .collect();
    env.push(("CS_VERSION_CHECK_URL".to_string(), url.to_string()));
    (command, env, repo_dir, tmp)
}

/// Convenience wrapper that points `CS_VERSION_CHECK_URL` at an
/// unreachable address (RFC 5737).
fn version_check_setup() -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    version_check_setup_with_url(UNREACHABLE_URL)
}

/// Start a [`FakeHttpServer`] that mimics the GitHub releases/latest endpoint.
fn start_fake_version_server() -> FakeHttpServer {
    FakeHttpServer::start(|_req| {
        let body = format!(r#"{{"tag_name":"{}"}}"#, FAKE_LATEST_VERSION);
        (200, body)
    })
}

/// Set up a fake version server with `CS_DISABLE_VERSION_CHECK=1`.
///
/// Returns the server, a started+initialized MCP client, the test file
/// path, and the temp dir guard.
fn disabled_check_setup() -> (FakeHttpServer, MCPClient, std::path::PathBuf, tempfile::TempDir) {
    let server = start_fake_version_server();
    let url = format!("http://{}:{}/releases/latest", fake_server_url_host(), server.port());

    let (command, mut env, repo_dir, tmp) = version_check_setup_with_url(&url);
    env.push(("CS_DISABLE_VERSION_CHECK".to_string(), "1".to_string()));

    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = repo_dir.join("src/utils/calculator.py");
    (server, client, test_file, tmp)
}

/// Call `code_health_score` on `test_file` and return the result text.
fn call_code_health_score(client: &mut MCPClient, test_file: &Path) -> String {
    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": test_file.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");
    extract_result_text(&response)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Start an MCP client from a version-check setup and return it with
/// the test file path and temp dir guard.
fn start_unreachable_client() -> (MCPClient, std::path::PathBuf, tempfile::TempDir) {
    let (command, env, repo_dir, tmp) = version_check_setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    let test_file = repo_dir.join("src/utils/calculator.py");
    (client, test_file, tmp)
}

pub fn test_tool_responds_when_github_unreachable() {
    let (mut client, test_file, _tmp) = start_unreachable_client();

    let result_text = call_code_health_score(&mut client, &test_file);
    assert!(!result_text.is_empty(), "Tool should return content");

    let score = extract_code_health_score(&result_text);
    assert!(score.is_some(), "Response should contain a valid score: {result_text}");
}

pub fn test_no_version_update_noise() {
    let (mut client, test_file, _tmp) = start_unreachable_client();

    for i in 1..=2 {
        let result_text = call_code_health_score(&mut client, &test_file);
        assert!(
            !result_text.contains("VERSION UPDATE AVAILABLE"),
            "Call {i}: unexpected VERSION UPDATE AVAILABLE banner",
        );
    }
}

pub fn test_response_time_acceptable() {
    let (mut client, test_file, _tmp) = start_unreachable_client();

    let start = Instant::now();
    let result_text = call_code_health_score(&mut client, &test_file);
    let elapsed = start.elapsed();

    assert!(!result_text.is_empty(), "Tool should return content");
    assert!(
        elapsed < Duration::from_secs(30),
        "Response took {elapsed:?} — should be under 30s",
    );
}

pub fn test_version_info_appears_after_background_fetch() {
    let server = start_fake_version_server();
    let url = format!("http://{}:{}/releases/latest", fake_server_url_host(), server.port());

    let (command, env, repo_dir, _tmp) = version_check_setup_with_url(&url);
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = repo_dir.join("src/utils/calculator.py");

    // Call 1 — triggers background fetch; version banner not expected yet.
    let result_text = call_code_health_score(&mut client, &test_file);
    let score = extract_code_health_score(&result_text);
    assert!(score.is_some(), "Call 1 should return a valid score: {result_text}");

    // Wait for the fast local fetch to complete in the background.
    std::thread::sleep(Duration::from_secs(3));

    // Calls 2..5 — cached result should now include the version banner.
    let mut version_appeared = false;
    for _ in 2..=5 {
        let result_text = call_code_health_score(&mut client, &test_file);
        if result_text.contains("VERSION UPDATE AVAILABLE") {
            assert!(
                result_text.contains(FAKE_LATEST_VERSION),
                "Banner should mention {FAKE_LATEST_VERSION}",
            );
            version_appeared = true;
            break;
        }
    }

    assert!(version_appeared, "VERSION UPDATE AVAILABLE should appear on a subsequent call");
}

pub fn test_disabled_version_check_no_banner() {
    let (_server, mut client, test_file, _tmp) = disabled_check_setup();

    for i in 1..=3 {
        let result_text = call_code_health_score(&mut client, &test_file);
        assert!(
            !result_text.contains("VERSION UPDATE AVAILABLE"),
            "Call {i}: VERSION UPDATE AVAILABLE should be suppressed",
        );
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Count requests that originate from the MCP server's version checker.
///
/// The version checker sends `User-Agent: cs-mcp` (see
/// `version_checker::fetch_latest_version`). Filtering on this UA isolates
/// the MCP's own version-check traffic from unrelated localhost connections
/// — e.g. security/EDR agents or port monitors that probe newly-opened
/// listening ports with their own clients (`Go-http-client`, etc.).
fn mcp_version_check_request_count(server: &FakeHttpServer) -> usize {
    server
        .get_requests()
        .iter()
        .filter(|req| {
            req.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("user-agent") && v == "cs-mcp")
        })
        .count()
}

pub fn test_disabled_version_check_no_network_traffic() {
    let (server, mut client, test_file, _tmp) = disabled_check_setup();

    for _ in 1..=3 {
        call_code_health_score(&mut client, &test_file);
    }

    // Allow any stray background requests to arrive.
    std::thread::sleep(Duration::from_secs(2));

    assert_eq!(
        mcp_version_check_request_count(&server),
        0,
        "MCP server should not call the version endpoint when disabled",
    );
}

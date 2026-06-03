//! Error logging and telemetry redaction integration tests.
//!
//! Verifies that the MCP server:
//! 1. Sends only safe error kind labels (not raw stderr) to telemetry
//! 2. Reports `license_check_failed` for invalid tokens without leaking them
//! 3. Logs full error details to a file when file logging is enabled
//! 4. Respects `CS_LOG_RETENTION_DAYS=0` to disable file logging
//! 5. Includes file extension detail for unsupported file types

use super::*;
use super::fake_https_server::FakeHttpsServer;

const SAFE_ERROR_KINDS: &[&str] = &[
    "non_zero_exit",
    "not_found",
    "io",
    "invalid_input",
    "license_check_failed",
    "http",
    "transport",
    "status",
    "api_error",
    "file_not_found",
    "unsupported_file_type",
    "not_a_git_repo",
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn trigger_error_with_fake_server(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
    config_dir: &Path,
    extra_env: &[(&str, &str)],
    file_path: Option<&Path>,
) -> (String, Vec<serde_json::Value>) {
    let cert_dir = create_temp_dir("cs_mcp_certs_err_").expect("cert dir");
    let server = FakeHttpsServer::always_ok(cert_dir.path());

    let target = file_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| repo_dir.join("does_not_exist_xyz.py"));

    let env: Vec<(String, String)> = env
        .iter()
        .cloned()
        .chain(std::iter::once((
            "CS_TRACKING_URL".to_string(),
            server.url(),
        )))
        .chain(std::iter::once((
            "REQUESTS_CA_BUNDLE".to_string(),
            server.certs.ca_cert_path.to_string_lossy().to_string(),
        )))
        .chain(std::iter::once((
            "CS_CONFIG_DIR".to_string(),
            docker_config_dir(config_dir, repo_dir),
        )))
        .chain(
            extra_env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        )
        .collect();

    let mut client = make_client(command, &env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": target.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);

    // Docker containers may need extra time for telemetry delivery
    let wait = if super::is_docker() { 5 } else { 2 };
    std::thread::sleep(Duration::from_secs(wait));

    let payloads = server.get_payloads();
    server.shutdown();

    (result_text, payloads)
}

fn extract_error_payloads(payloads: &[serde_json::Value]) -> Vec<serde_json::Value> {
    payloads
        .iter()
        .filter(|p| {
            p.get("event-type")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("error"))
        })
        .cloned()
        .collect()
}

fn get_error_kind(payload: &serde_json::Value) -> String {
    payload
        .get("event-properties")
        .and_then(|p| p.get("error"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn get_error_detail(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("event-properties")
        .and_then(|p| p.get("detail"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

pub fn test_error_telemetry_sends_only_kind() {
    let (command, env, repo_dir, _tmp) = setup();
    let config_dir = repo_dir.join(".config_telemetry");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let (result_text, payloads) =
        trigger_error_with_fake_server(&command, &env, &repo_dir, &config_dir, &[], None);

    assert!(!result_text.is_empty(), "Tool should return error response");

    let error_payloads = extract_error_payloads(&payloads);
    assert!(
        !error_payloads.is_empty(),
        "Should have at least one error tracking event"
    );

    for payload in &error_payloads {
        let error_value = get_error_kind(payload);
        assert!(
            SAFE_ERROR_KINDS.contains(&error_value.as_str()),
            "Error kind '{error_value}' is not in SAFE_ERROR_KINDS"
        );
        assert!(
            !error_value.contains('/') && !error_value.contains('\\'),
            "Error kind should not contain path separators: '{error_value}'"
        );
        assert!(
            !error_value.contains("exited with code"),
            "No raw stderr should leak into telemetry: '{error_value}'"
        );
    }
}

pub fn test_error_telemetry_invalid_token() {
    let (command, env, repo_dir, _tmp) = setup();
    let config_dir = repo_dir.join(".config_invalid_token");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let existing_file = repo_dir.join("src/utils/calculator.py");
    let (result_text, payloads) = trigger_error_with_fake_server(
        &command,
        &env,
        &repo_dir,
        &config_dir,
        &[("CS_ACCESS_TOKEN", "invalid-garbage-token-xyz")],
        Some(&existing_file),
    );

    assert!(!result_text.is_empty(), "Tool should return error response");

    let error_payloads = extract_error_payloads(&payloads);
    assert!(
        !error_payloads.is_empty(),
        "Should have at least one error tracking event"
    );

    let error_value = get_error_kind(&error_payloads[0]);
    assert_eq!(
        error_value, "license_check_failed",
        "Error kind should be license_check_failed, got: '{error_value}'"
    );
    assert!(
        !error_value.contains("invalid-garbage-token-xyz"),
        "Token value must not leak into telemetry"
    );
}

pub fn test_error_logged_to_file() {
    let (command, env, repo_dir, _tmp) = setup();
    let config_dir = repo_dir.join(".config_logging");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let (result_text, _payloads) = trigger_error_with_fake_server(
        &command,
        &env,
        &repo_dir,
        &config_dir,
        &[("CS_LOG_RETENTION_DAYS", "7")],
        None,
    );

    assert!(!result_text.is_empty(), "Tool should return error response");

    let log_dir = config_dir.join("logs");
    assert!(
        log_dir.exists() && log_dir.is_dir(),
        "Log directory should be created at {log_dir:?}"
    );

    let log_files: Vec<_> = std::fs::read_dir(&log_dir)
        .expect("read log dir")
        .filter_map(|e| e.ok())
        .collect();
    assert!(
        !log_files.is_empty(),
        "Log directory should contain at least one file"
    );

    let log_content: String = log_files
        .iter()
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .collect();
    assert!(
        log_content.to_lowercase().contains("error"),
        "Log files should contain error details"
    );

    let detail_markers = [
        "does_not_exist_xyz",
        "no such file",
        "not a supported",
        "non_zero_exit",
        "invalid_input",
        "file_not_found",
    ];
    let has_detail = detail_markers
        .iter()
        .any(|m| log_content.to_lowercase().contains(m));
    assert!(has_detail, "Log should contain error detail markers");
}

pub fn test_file_logging_disabled_when_zero() {
    let (command, env, repo_dir, _tmp) = setup();
    let config_dir = repo_dir.join(".config_no_logging");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let (result_text, _payloads) = trigger_error_with_fake_server(
        &command,
        &env,
        &repo_dir,
        &config_dir,
        &[("CS_LOG_RETENTION_DAYS", "0")],
        None,
    );

    assert!(!result_text.is_empty(), "Tool should return error response");

    let log_dir = config_dir.join("logs");
    assert!(
        !log_dir.exists(),
        "Log directory should NOT exist when retention is 0, but found {log_dir:?}"
    );
}

pub fn test_unsupported_file_type_detail_in_telemetry() {
    let (command, env, repo_dir, _tmp) = setup();
    let config_dir = repo_dir.join(".config_unsupported");
    std::fs::create_dir_all(&config_dir).expect("create config dir");

    let unsupported_file = repo_dir.join("readme.txt");
    std::fs::write(&unsupported_file, "just a text file").expect("write readme.txt");

    let (result_text, payloads) = trigger_error_with_fake_server(
        &command,
        &env,
        &repo_dir,
        &config_dir,
        &[],
        Some(&unsupported_file),
    );

    assert!(!result_text.is_empty(), "Tool should return error response");

    let error_payloads = extract_error_payloads(&payloads);
    assert!(
        !error_payloads.is_empty(),
        "Should have at least one error tracking event"
    );

    let error_value = get_error_kind(&error_payloads[0]);
    assert_eq!(
        error_value, "unsupported_file_type",
        "Error kind should be unsupported_file_type, got: '{error_value}'"
    );

    let detail = get_error_detail(&error_payloads[0]);
    assert_eq!(
        detail.as_deref(),
        Some(".txt"),
        "Detail should be '.txt', got: {detail:?}"
    );
}

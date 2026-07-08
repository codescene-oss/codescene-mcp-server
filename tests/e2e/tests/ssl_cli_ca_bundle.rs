//! SSL CLI CA bundle end-to-end tests.
//!
//! Validates the full MCP -> real embedded CLI -> HTTPS license endpoint
//! path when a custom CA certificate is required:
//!
//! - When `REQUESTS_CA_BUNDLE` points to the CA cert, the CLI connects
//!   through TLS and the `verify_installation` CLI connectivity check passes.
//! - Without the CA bundle, the TLS handshake fails and the CLI
//!   connectivity check reports a failure.
//!
//! Unlike `ssl_cli_truststore.rs` (which uses a fake CLI to verify JVM args
//! are injected), this test exercises the **real embedded CLI binary** making
//! an actual HTTPS connection through a self-signed CA certificate.

use super::fake_https_server::FakeHttpsServer;
use super::*;

const VERIFY_TIMEOUT: Duration = Duration::from_secs(120);

/// Start a fake HTTPS server that responds to both the tool-license
/// endpoint (used by the CLI for license verification) and the review
/// endpoint, and build the MCP environment pointing at it.
fn cli_ca_setup() -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    FakeHttpsServer,
    tempfile::TempDir,
) {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_ssl_cli_ca_").expect("create temp dir");
    let sample_files = get_sample_files();
    let repo_dir = create_git_repo(temp_dir.path(), &sample_files).expect("create git repo");

    let server = FakeHttpsServer::start(temp_dir.path(), |req| {
        let path = &req.path;
        if path.contains("/api/v2/tool-license/cli") {
            return (200, r#"{"valid":true}"#.to_string());
        }
        if path.contains("/api/v2/projects") {
            if path.contains("page=1") || !path.contains("page=") {
                return (200, r#"[{"id":1,"name":"Test Project"}]"#.to_string());
            }
            return (200, "[]".to_string());
        }
        (200, "{}".to_string())
    });

    let base = base_env();
    let env_map = backend.get_env(&base, &repo_dir);
    let env: Vec<(String, String)> = env_map
        .into_iter()
        .chain([
            ("CS_ACCESS_TOKEN".to_string(), "test-token".to_string()),
            ("CS_DISABLE_VERSION_CHECK".to_string(), "1".to_string()),
            ("CS_DISABLE_TRACKING".to_string(), "1".to_string()),
            ("CS_ONPREM_URL".to_string(), server.url()),
            (
                "REQUESTS_CA_BUNDLE".to_string(),
                server.certs.ca_cert_path.to_string_lossy().to_string(),
            ),
        ])
        .collect();

    let command = backend.get_command(&repo_dir);
    (command, env, repo_dir, server, temp_dir)
}

fn call_verify_installation(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "verify_installation",
            json!({"git_repository_path": repo_dir.to_string_lossy()}),
            VERIFY_TIMEOUT,
        )
        .expect("verify_installation call should succeed");

    extract_result_text(&response)
}

/// With `REQUESTS_CA_BUNDLE` pointing to the CA cert that signed the
/// server certificate, the real CLI should connect successfully through
/// TLS and the CLI connectivity check should pass.
pub fn test_cli_connectivity_passes_with_ca_bundle() {
    if is_docker() {
        return skip_if_docker("HTTPS server on host unreachable from container");
    }
    let (command, env, repo_dir, _server, _tmp) = cli_ca_setup();

    let result = call_verify_installation(&command, &env, &repo_dir);
    let lower = result.to_lowercase();

    assert!(
        lower.contains("[pass] cli connectivity"),
        "CLI connectivity check should pass with CA bundle set, got: {result}"
    );
}

/// Without `REQUESTS_CA_BUNDLE`, the CLI cannot trust the self-signed
/// server certificate. The TLS handshake fails and `verify_installation`
/// should report a CLI connectivity failure.
pub fn test_cli_connectivity_fails_without_ca_bundle() {
    if is_docker() {
        return skip_if_docker("HTTPS server on host unreachable from container");
    }
    let (command, env, repo_dir, _server, _tmp) = cli_ca_setup();

    let env_without_ca: Vec<(String, String)> = env
        .into_iter()
        .filter(|(k, _)| k != "REQUESTS_CA_BUNDLE" && k != "SSL_CERT_FILE" && k != "CURL_CA_BUNDLE")
        .collect();

    let result = call_verify_installation(&command, &env_without_ca, &repo_dir);
    let lower = result.to_lowercase();

    assert!(
        lower.contains("[fail] cli connectivity")
            || lower.contains("ssl")
            || lower.contains("tls")
            || lower.contains("certificate")
            || lower.contains("handshake"),
        "CLI connectivity should fail without CA bundle, got: {result}"
    );
    assert!(
        !lower.contains("[pass] cli connectivity"),
        "CLI connectivity should NOT pass without CA bundle, got: {result}"
    );
}

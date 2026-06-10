//! SSL API CA bundle integration tests.
//!
//! Verifies that the MCP server's reqwest HTTP client respects
//! `REQUESTS_CA_BUNDLE` when making API calls (e.g. `select_project`).
//!
//! Spins up a local HTTPS server with a self-signed CA certificate,
//! sets `CS_ONPREM_URL` to point at it, and confirms:
//! 1. `select_project` succeeds when `REQUESTS_CA_BUNDLE` is set.
//! 2. `select_project` fails without the CA bundle.

use super::*;
use super::fake_https_server::FakeHttpsServer;

fn call_select_project(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool("select_project", json!({}), Duration::from_secs(30))
        .expect("select_project call should succeed");

    extract_result_text(&response)
}

fn ssl_api_setup() -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    FakeHttpsServer,
    tempfile::TempDir,
) {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_ssl_api_").expect("create temp dir");
    let sample_files = get_sample_files();
    let repo_dir = create_git_repo(temp_dir.path(), &sample_files).expect("create git repo");

    let server = FakeHttpsServer::start_projects_api(temp_dir.path());

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

/// When `REQUESTS_CA_BUNDLE` points to the CA cert, `select_project` should
/// succeed and return the fake project data.
pub fn test_api_uses_ca_bundle() {
    if is_docker() { return skip_if_docker("HTTPS server on host unreachable from container"); }
    let (command, env, repo_dir, _server, _tmp) = ssl_api_setup();

    let result = call_select_project(&command, &env, &repo_dir);
    let lower = result.to_lowercase();

    assert!(
        lower.contains("test project"),
        "Should return project data when CA bundle is set, got: {result}"
    );
    assert!(
        !lower.contains("error"),
        "Should not contain errors when CA bundle is set, got: {result}"
    );
}

/// Without `REQUESTS_CA_BUNDLE`, the TLS handshake should fail because the
/// server uses a self-signed certificate unknown to the system trust store.
pub fn test_api_fails_without_ca_bundle() {
    if is_docker() { return skip_if_docker("HTTPS server on host unreachable from container"); }
    let (command, env, repo_dir, _server, _tmp) = ssl_api_setup();

    let env_without_ca: Vec<(String, String)> = env
        .into_iter()
        .filter(|(k, _)| {
            k != "REQUESTS_CA_BUNDLE" && k != "SSL_CERT_FILE" && k != "CURL_CA_BUNDLE"
        })
        .collect();

    let result = call_select_project(&command, &env_without_ca, &repo_dir);
    let lower = result.to_lowercase();

    assert!(
        lower.contains("error"),
        "Should fail without CA bundle, got: {result}"
    );
    assert!(
        !lower.contains("test project"),
        "Should not return project data without CA bundle, got: {result}"
    );
}

/// When `REQUESTS_CA_BUNDLE` points to a non-existent file, the behaviour
/// should be the same as when the variable is unset: the TLS handshake fails
/// because the self-signed server certificate is unknown. This catches a
/// common Windows misconfiguration where backslashes in JSON config are not
/// properly escaped, causing the path to be mangled.
pub fn test_api_fails_with_invalid_ca_bundle_path() {
    if is_docker() { return skip_if_docker("HTTPS server on host unreachable from container"); }
    let (command, env, repo_dir, _server, _tmp) = ssl_api_setup();

    let env_bad_ca: Vec<(String, String)> = env
        .into_iter()
        .filter(|(k, _)| {
            k != "REQUESTS_CA_BUNDLE" && k != "SSL_CERT_FILE" && k != "CURL_CA_BUNDLE"
        })
        .chain(std::iter::once((
            "REQUESTS_CA_BUNDLE".to_string(),
            "/nonexistent/path/to/ca-bundle.pem".to_string(),
        )))
        .collect();

    let result = call_select_project(&command, &env_bad_ca, &repo_dir);
    let lower = result.to_lowercase();

    assert!(
        lower.contains("error"),
        "Should fail when CA bundle path is invalid, got: {result}"
    );
    assert!(
        !lower.contains("test project"),
        "Should not return project data when CA bundle path is invalid, got: {result}"
    );
}

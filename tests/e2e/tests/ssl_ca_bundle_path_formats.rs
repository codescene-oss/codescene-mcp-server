//! SSL CA bundle path format integration tests.
//!
//! Validates that `REQUESTS_CA_BUNDLE` works with different path formats
//! on the current platform, specifically targeting Windows where paths
//! can use backslashes (`C:\...`) or forward slashes (`C:/...`).
//!
//! Spins up a local HTTPS server with a self-signed CA certificate and
//! confirms that `select_project` succeeds when `REQUESTS_CA_BUNDLE`
//! uses each path format, and fails without the CA bundle (proving TLS
//! verification is actually enforced).

use super::fake_https_server::FakeHttpsServer;
use super::*;

const TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn call_select_project(command: &[String], env: &[(String, String)], repo_dir: &Path) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool("select_project", json!({}), TIMEOUT)
        .expect("select_project call should succeed");

    extract_result_text(&response)
}

struct TestSetup {
    command: Vec<String>,
    env: Vec<(String, String)>,
    repo_dir: std::path::PathBuf,
    ca_cert_path: std::path::PathBuf,
    _server: FakeHttpsServer,
    _tmp: tempfile::TempDir,
}

fn path_setup() -> TestSetup {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_ssl_paths_").expect("create temp dir");
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
        ])
        .collect();

    let ca_cert_path = server.certs.ca_cert_path.clone();
    let command = backend.get_command(&repo_dir);

    TestSetup {
        command,
        env,
        repo_dir,
        ca_cert_path,
        _server: server,
        _tmp: temp_dir,
    }
}

fn env_with_ca_bundle(base: &[(String, String)], ca_path: &str) -> Vec<(String, String)> {
    base.iter()
        .filter(|(k, _)| k != "REQUESTS_CA_BUNDLE" && k != "SSL_CERT_FILE" && k != "CURL_CA_BUNDLE")
        .cloned()
        .chain(std::iter::once((
            "REQUESTS_CA_BUNDLE".to_string(),
            ca_path.to_string(),
        )))
        .collect()
}

fn strip_windows_prefix(path: &str) -> String {
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

fn assert_tls_outcome(result: &str, expect_success: bool, context: &str) {
    let lower = result.to_lowercase();
    let has_project = lower.contains("test project");
    let has_error = lower.contains("error");

    if expect_success {
        assert!(
            has_project,
            "{context}: Should return project data, got: {result}"
        );
        assert!(
            !has_error,
            "{context}: Should not contain errors, got: {result}"
        );
    } else {
        assert!(has_error, "{context}: Should fail, got: {result}");
        assert!(
            !has_project,
            "{context}: Should not return project data, got: {result}"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests — environment variable path formats
// ---------------------------------------------------------------------------

/// Baseline: without `REQUESTS_CA_BUNDLE`, the self-signed server is rejected.
/// This proves TLS verification is actually enforced in subsequent tests.
pub fn test_baseline_fails_without_ca_bundle() {
    if is_docker() {
        return skip_if_docker("HTTPS server on host unreachable from container");
    }
    let s = path_setup();

    let env: Vec<(String, String)> = s
        .env
        .iter()
        .filter(|(k, _)| k != "REQUESTS_CA_BUNDLE" && k != "SSL_CERT_FILE" && k != "CURL_CA_BUNDLE")
        .cloned()
        .collect();

    let result = call_select_project(&s.command, &env, &s.repo_dir);
    assert_tls_outcome(&result, false, "No CA bundle");
}

/// The canonical path (as returned by the OS) works with `REQUESTS_CA_BUNDLE`.
pub fn test_canonical_path_succeeds() {
    if is_docker() {
        return skip_if_docker("HTTPS server on host unreachable from container");
    }
    let s = path_setup();

    let canonical = s
        .ca_cert_path
        .canonicalize()
        .unwrap_or_else(|_| s.ca_cert_path.clone());
    // On Windows, canonicalize returns \\?\ prefixed paths — strip the prefix
    // to get a regular absolute path as users would configure.
    let ca_str = strip_windows_prefix(&canonical.to_string_lossy());

    let env = env_with_ca_bundle(&s.env, &ca_str);
    let result = call_select_project(&s.command, &env, &s.repo_dir);
    assert_tls_outcome(&result, true, &format!("Canonical path: {ca_str}"));
}

/// Forward-slash paths (`C:/Users/...` on Windows) must work. This is the
/// format many MCP client configs use because JSON requires escaped
/// backslashes but forward slashes work without escaping.
pub fn test_forward_slash_path_succeeds() {
    if is_docker() {
        return skip_if_docker("HTTPS server on host unreachable from container");
    }
    let s = path_setup();

    let native = s.ca_cert_path.to_string_lossy().to_string();
    let forward_slash = native.replace('\\', "/");

    let env = env_with_ca_bundle(&s.env, &forward_slash);
    let result = call_select_project(&s.command, &env, &s.repo_dir);
    assert_tls_outcome(
        &result,
        true,
        &format!("Forward-slash path: {forward_slash}"),
    );
}

/// Native Windows backslash paths (`C:\Users\...`) must work. This is
/// what users get when they copy a path from Explorer or use `where`.
#[cfg(windows)]
pub fn test_backslash_path_succeeds() {
    if is_docker() {
        return skip_if_docker("HTTPS server on host unreachable from container");
    }
    let s = path_setup();

    let native = s.ca_cert_path.to_string_lossy().to_string();
    let backslash = native.replace('/', "\\");

    let env = env_with_ca_bundle(&s.env, &backslash);
    let result = call_select_project(&s.command, &env, &s.repo_dir);
    assert_tls_outcome(&result, true, &format!("Backslash path: {backslash}"));
}

/// A nonexistent CA bundle path must cause TLS failure — the server must
/// NOT silently fall through to system roots when the user explicitly
/// configured a path that doesn't exist.
pub fn test_nonexistent_ca_bundle_path_fails() {
    if is_docker() {
        return skip_if_docker("HTTPS server on host unreachable from container");
    }
    let s = path_setup();

    let bad_path = if cfg!(windows) {
        r"C:\nonexistent\path\to\cert.pem".to_string()
    } else {
        "/nonexistent/path/to/cert.pem".to_string()
    };

    let env = env_with_ca_bundle(&s.env, &bad_path);
    let result = call_select_project(&s.command, &env, &s.repo_dir);
    assert_tls_outcome(&result, false, &format!("Nonexistent path: {bad_path}"));
}

// ---------------------------------------------------------------------------
// Tests — set_config tool flow
// ---------------------------------------------------------------------------

/// Setting `ca_bundle` via `set_config` with a forward-slash Windows path
/// must apply immediately and allow TLS connections to the self-signed
/// HTTPS server. This exercises the full config pipeline: JSON-RPC →
/// `set_config` → `std::env::set_var("REQUESTS_CA_BUNDLE", ...)` →
/// `ca_bundle_path_from_env()` → `build_reqwest_client()`.
pub fn test_set_config_ca_bundle_applies_immediately() {
    if is_docker() {
        return skip_if_docker("HTTPS server on host unreachable from container");
    }
    let s = path_setup();

    // Start without REQUESTS_CA_BUNDLE so the env var is not "client-provided"
    // (otherwise set_config won't override it).
    let env: Vec<(String, String)> = s
        .env
        .iter()
        .filter(|(k, _)| k != "REQUESTS_CA_BUNDLE" && k != "SSL_CERT_FILE" && k != "CURL_CA_BUNDLE")
        .cloned()
        .collect();

    // Use a config dir inside the repo so it works with Docker backends too.
    let config_dir = s.repo_dir.join(".cs_config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    let env: Vec<(String, String)> = env
        .into_iter()
        .chain(std::iter::once((
            "CS_CONFIG_DIR".to_string(),
            docker_config_dir(&config_dir, &s.repo_dir),
        )))
        .collect();

    let mut client = make_client(&s.command, &env, &s.repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    // Use forward-slash path — the format users should prefer in JSON config.
    let ca_str = s.ca_cert_path.to_string_lossy().replace('\\', "/");

    let set_result = client
        .call_tool(
            "set_config",
            json!({"key": "ca_bundle", "value": ca_str}),
            TIMEOUT,
        )
        .expect("set_config should succeed");
    let set_text = extract_result_text(&set_result);
    assert!(
        !set_text.to_lowercase().contains("error"),
        "set_config should succeed, got: {set_text}"
    );

    // Now select_project should work because set_config applied the CA bundle.
    let response = client
        .call_tool("select_project", json!({}), TIMEOUT)
        .expect("select_project call should succeed");
    let result = extract_result_text(&response);
    assert_tls_outcome(&result, true, &format!("set_config ca_bundle: {ca_str}"));
}

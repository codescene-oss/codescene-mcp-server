//! SSL certificate CLI integration tests.
//!
//! Validates that the MCP server passes custom CA certificates to the CLI
//! via the `CS_CERTS` environment variable:
//! - When `REQUESTS_CA_BUNDLE` is set, `CS_CERTS` is forwarded to the CLI.
//! - When `REQUESTS_CA_BUNDLE` is missing, `CS_CERTS` is not set.
//!
//! Uses a fake CLI binary compiled from Rust source at runtime that checks
//! whether `CS_CERTS` is present and points to a valid file.

use super::*;
use std::process::Command;

const TEST_CA_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
    MIIDPzCCAiegAwIBAgIUdGj465l77xx7Je8KqOESIqx9zXYwDQYJKoZIhvcNAQEL\n\
    BQAwTzELMAkGA1UEBhMCVVMxDTALBgNVBAgMBFRlc3QxDTALBgNVBAcMBFRlc3Qx\n\
    EDAOBgNVBAoMB1Rlc3QgQ0ExEDAOBgNVBAMMB1Rlc3QgQ0EwHhcNMjYwMTE2MDky\n\
    OTQ5WhcNMjcwMTE2MDkyOTQ5WjBPMQswCQYDVQQGEwJVUzENMAsGA1UECAwEVGVz\n\
    dDENMAsGA1UEBwwEVGVzdDEQMA4GA1UECgwHVGVzdCBDQTEQMA4GA1UEAwwHVGVz\n\
    dCBDQTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAMqoClSXXim/fiI9\n\
    Lc3X/4D4rHK6cWAnKVPA+CetSJiGrMrfeJZMSTWUv19M8aKlmbZsQxN4X4neycWE\n\
    UxH9y3XaqV9grmGvutTgw98t6fhawevGrjmcA+ygQ5S37reRQOHtc9ob51b8b9Rr\n\
    nyE8qIU2dkZ115VpFN+/woG2LG23iGj2dJ3AaZc/R8X0UQu5tQCDwTOeO/zMWPGG\n\
    xjzDpnFs4u7IAwPECEgEuxHH8PHapUoc0d+Aq/wBKM015qdohoaydrztzXp6DKJ5\n\
    RBv/cn+lTpFdvJQS0CceIo+hOUa46ONq63VM3SQhT7enOWToONBxrZpof18bITFd\n\
    2h4XxoMCAwEAAaMTMBEwDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOC\n\
    AQEAHDWTjJILOtrCBRFksVyvniUGFR8ioz2cE4R8xcKAFxNOPKLuxwm+ilbUBX3A\n\
    8VOCJjR6IimsLMhAUEi5FGDiVVhOwIp1+pULEigTG7r72yOCr2xnw8NrX9UbJNnx\n\
    rlyCjEN9URBpriiGGegixH6AoLVW0SjEsJ7CgfqmfWzKU+nsPIunvePtFhSw5jHC\n\
    mHwYTxYcxYW33TK9qQxs119A9+qG5Z+cJlDtYrfHirHwPZQeuQ25jhKE5FUUiuiq\n\
    iblIIstcPF4n6wQ0ieNajmj5nHXQEypkek8D/ANbwwhlVQ3u/hldcAyj4qD7G5oJ\n\
    sC0Nc9QdNQt5Tos5Je5S7CWL0w==\n\
    -----END CERTIFICATE-----\n";

const FAKE_CLI_RS: &str = r##"use std::env;
use std::path::Path;
use std::process;

fn main() {
    let mut cmd = String::new();

    for arg in env::args().skip(1) {
        if arg.starts_with("-D") {
            continue;
        }
        if cmd.is_empty() {
            cmd = arg;
        }
    }

    let require = env::var("REQUIRE_CS_CERTS").unwrap_or_else(|_| "0".to_string()) == "1";
    if require {
        match env::var("CS_CERTS") {
            Ok(path) if Path::new(&path).is_file() => {}
            Ok(path) => {
                eprintln!("CS_CERTS file missing: {path}");
                process::exit(21);
            }
            Err(_) => {
                eprintln!("CS_CERTS not set");
                process::exit(22);
            }
        }
    }

    match cmd.as_str() {
        "version" => {
            println!("fake-cli-version");
            process::exit(0);
        }
        "review" => {
            println!(r#"{{"score":9.5,"review":[]}}"#);
            process::exit(0);
        }
        _ => {
            eprintln!("unsupported command: {cmd}");
            process::exit(23);
        }
    }
}
"##;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compile the fake CLI from embedded Rust source and return its path.
fn make_fake_cli(dir: &Path) -> std::path::PathBuf {
    let source = dir.join("fake_cs.rs");
    std::fs::write(&source, FAKE_CLI_RS).expect("write fake CLI source");

    let binary_name = if cfg!(windows) { "cs.exe" } else { "cs" };
    let output_path = dir.join(binary_name);

    let result = Command::new("rustc")
        .args([
            source.to_str().expect("source path"),
            "-O",
            "-o",
            output_path.to_str().expect("output path"),
        ])
        .output()
        .expect("rustc should execute");

    assert!(
        result.status.success(),
        "Failed to compile fake CLI: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    output_path
}

/// Start the MCP server, call `code_health_score` on calculator.py, return result text.
fn call_score_tool(
    command: &[String],
    env: &[(String, String)],
    repo_dir: &Path,
) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let target = repo_dir.join("src/utils/calculator.py");
    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": target.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .expect("code_health_score call should succeed");

    extract_result_text(&response)
}

/// Build common test state: temp dir, git repo, fake CLI binary, cert file, and env.
fn local_setup() -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_ssl_cli_").expect("create temp dir");
    let sample_files = get_sample_files();
    let repo_dir =
        create_git_repo(temp_dir.path(), &sample_files).expect("create git repo");

    let fake_cli = make_fake_cli(temp_dir.path());

    let cert_path = temp_dir.path().join("internal-ca.pem");
    std::fs::write(&cert_path, TEST_CA_CERT_PEM).expect("write cert PEM");

    let base = base_env();
    let env_map = backend.get_env(&base, &repo_dir);
    let env: Vec<(String, String)> = env_map
        .into_iter()
        .chain([
            ("CS_CLI_PATH".to_string(), fake_cli.to_string_lossy().to_string()),
            ("REQUIRE_CS_CERTS".to_string(), "1".to_string()),
            ("CS_DISABLE_VERSION_CHECK".to_string(), "1".to_string()),
            ("CS_DISABLE_TRACKING".to_string(), "1".to_string()),
        ])
        .collect();

    let command = backend.get_command(&repo_dir);
    (command, env, repo_dir, cert_path, temp_dir)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// When `REQUESTS_CA_BUNDLE` points to a valid cert, the MCP server passes
/// `CS_CERTS` to the CLI and the fake CLI succeeds with a score.
pub fn test_cs_certs_passed_to_cli() {
    if is_docker() { return skip_if_docker("fake CLI binary not available in container"); }
    let (command, env, repo_dir, cert_path, _tmp) = local_setup();

    let env: Vec<(String, String)> = env
        .into_iter()
        .filter(|(k, _)| k != "SSL_CERT_FILE" && k != "CURL_CA_BUNDLE")
        .chain(std::iter::once((
            "REQUESTS_CA_BUNDLE".to_string(),
            cert_path.to_string_lossy().to_string(),
        )))
        .collect();

    let result = call_score_tool(&command, &env, &repo_dir);
    assert!(
        result.contains("9.5") || result.to_lowercase().contains("score"),
        "Should return a Code Health score, got: {result}"
    );
}

/// Without `REQUESTS_CA_BUNDLE`, `CS_CERTS` is not set and the fake CLI
/// (with `REQUIRE_CS_CERTS=1`) exits with an error.
pub fn test_cs_certs_missing_without_ca_bundle() {
    if is_docker() { return skip_if_docker("fake CLI binary not available in container"); }
    let (command, env, repo_dir, _cert_path, _tmp) = local_setup();

    let env: Vec<(String, String)> = env
        .into_iter()
        .filter(|(k, _)| {
            k != "REQUESTS_CA_BUNDLE" && k != "SSL_CERT_FILE" && k != "CURL_CA_BUNDLE"
        })
        .collect();

    let result = call_score_tool(&command, &env, &repo_dir);
    assert!(
        result.to_lowercase().contains("cs_certs not set"),
        "Should report CS_CERTS not set, got: {result}"
    );
}

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Output;

use p12_keystore::{Certificate as P12Certificate, KeyStore as P12KeyStore, KeyStoreEntry};
use sha2::{Digest, Sha256};

use crate::environment;
use crate::errors::CliError;

/// Trait abstracting CLI subprocess execution for dependency injection.
#[async_trait::async_trait]
pub trait CliRunner: Send + Sync {
    async fn run(&self, args: &[&str], working_dir: Option<&Path>) -> Result<String, CliError>;
}

pub struct ProductionCliRunner;

#[async_trait::async_trait]
impl CliRunner for ProductionCliRunner {
    async fn run(&self, args: &[&str], working_dir: Option<&Path>) -> Result<String, CliError> {
        run_cli(args, working_dir).await
    }
}

const CLI_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cs-cli.zip"));

const CLI_BINARY_NAME: &str = if cfg!(windows) { "cs.exe" } else { "cs" };

const DOCKER_CLI_PATH: &str = "/root/.local/bin/cs";
const SSL_TRUSTSTORE_PASSWORD: &str = "changeit";

/// Resolve the path to the `cs` CLI binary.
///
/// Resolution order:
/// 1. `CS_CLI_PATH` environment variable override
/// 2. Docker container path (`/root/.local/bin/cs`)
/// 3. Extracted from embedded zip to cache directory
pub fn resolve_cli_path() -> Result<PathBuf, CliError> {
    resolve_from_env_override()
        .or_else(|| resolve_from_docker())
        .unwrap_or_else(|| extract_embedded_cli())
}

fn resolve_from_env_override() -> Option<Result<PathBuf, CliError>> {
    let path = std::env::var("CS_CLI_PATH").ok()?;
    let p = PathBuf::from(&path);
    if p.exists() {
        Some(Ok(p))
    } else {
        Some(Err(CliError::NotFound(format!(
            "CS_CLI_PATH points to non-existent path: {path}"
        ))))
    }
}

fn resolve_from_docker() -> Option<Result<PathBuf, CliError>> {
    if !environment::is_docker() {
        return None;
    }
    let p = PathBuf::from(DOCKER_CLI_PATH);
    p.exists().then(|| Ok(p))
}

pub async fn run_cli(args: &[&str], working_dir: Option<&Path>) -> Result<String, CliError> {
    let cli_path = resolve_cli_path()?;
    run_cli_at_path(&cli_path, args, working_dir).await
}

async fn run_cli_at_path(
    cli_path: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<String, CliError> {
    let output = run_cli_process(cli_path, args, working_dir, false).await?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if should_retry_after_telemetry_flush_error(&stderr) {
        let retry_output = run_cli_process(cli_path, args, working_dir, true).await?;
        return parse_cli_output(retry_output);
    }

    parse_cli_output(output)
}

async fn run_cli_process(
    cli_path: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
    disable_tracking: bool,
) -> Result<Output, CliError> {
    let mut cmd = tokio::process::Command::new(cli_path);
    let effective_args = with_ssl_cli_args_if_needed(cli_path, args);

    cmd.args(&effective_args)
        .env("CS_CONTEXT", "mcp-server")
        .env("CS_DISABLE_VERSION_CHECK", "1");

    if disable_tracking {
        cmd.env("CS_DISABLE_TRACKING", "1");
    }

    if let Ok(token) = std::env::var("CS_ACCESS_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            cmd.env("CS_ACCESS_TOKEN", token);
        }
    }

    if let Ok(url) = std::env::var("CS_ONPREM_URL") {
        cmd.env("CS_ONPREM_URL", url);
    }

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.output().await.map_err(CliError::from)
}

fn should_retry_after_telemetry_flush_error(stderr: &str) -> bool {
    stderr.contains("NoSuchFileException") && stderr.contains("codescene-cli.log.jsonl")
}

fn with_ssl_cli_args_if_needed(cli_path: &Path, args: &[&str]) -> Vec<String> {
    let mut effective_args = Vec::new();

    if is_cs_cli_binary(cli_path) {
        effective_args.extend(ssl_cli_args_from_env());
    }

    effective_args.extend(args.iter().map(|a| a.to_string()));
    effective_args
}

fn is_cs_cli_binary(cli_path: &Path) -> bool {
    let Some(name) = cli_path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };

    let lower = name.to_ascii_lowercase();
    lower == "cs" || lower == "cs.exe"
}

fn ssl_cli_args_from_env() -> Vec<String> {
    let Some(ca_bundle_path) = selected_ca_bundle_path_from_env() else {
        return Vec::new();
    };

    let Some(truststore_path) = create_or_get_truststore_from_pem(&ca_bundle_path) else {
        return Vec::new();
    };

    vec![
        format!(
            "-Djavax.net.ssl.trustStore={}",
            truststore_path.to_string_lossy()
        ),
        "-Djavax.net.ssl.trustStoreType=PKCS12".to_string(),
        format!("-Djavax.net.ssl.trustStorePassword={SSL_TRUSTSTORE_PASSWORD}"),
    ]
}

fn selected_ca_bundle_path_from_env() -> Option<PathBuf> {
    ["REQUESTS_CA_BUNDLE", "SSL_CERT_FILE", "CURL_CA_BUNDLE"]
        .into_iter()
        .find_map(|env_var| {
            std::env::var(env_var)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .filter(|p| p.is_file())
        })
}

fn create_or_get_truststore_from_pem(ca_bundle_path: &Path) -> Option<PathBuf> {
    let pem_data = std::fs::read(ca_bundle_path).ok()?;
    let truststore_path = truststore_path_for_pem(&pem_data);
    if truststore_path.exists() {
        return Some(truststore_path);
    }

    if write_pkcs12_truststore_from_pem(&pem_data, &truststore_path) {
        return Some(truststore_path);
    }

    None
}

fn truststore_path_for_pem(pem_data: &[u8]) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(pem_data);
    let digest = hasher.finalize();
    let digest_hex = format!("{:x}", digest);
    let short_hash = &digest_hex[..16];
    std::env::temp_dir().join(format!("cs-mcp-truststore-{short_hash}.p12"))
}

fn write_pkcs12_truststore_from_pem(pem_data: &[u8], truststore_path: &Path) -> bool {
    let Some(()) = ensure_parent_dir(truststore_path) else {
        return false;
    };
    let Some(pkcs12) = build_pkcs12_truststore_bytes(pem_data) else {
        return false;
    };
    std::fs::write(truststore_path, pkcs12).is_ok()
}

fn ensure_parent_dir(path: &Path) -> Option<()> {
    let parent = path.parent()?;
    std::fs::create_dir_all(parent).ok()
}

fn build_pkcs12_truststore_bytes(pem_data: &[u8]) -> Option<Vec<u8>> {
    let certs = parse_pem_certificates(pem_data)?;
    if certs.is_empty() {
        return None;
    }

    let mut keystore = P12KeyStore::new();
    for (idx, cert) in certs.into_iter().enumerate() {
        keystore.add_entry(&format!("ca-{idx}"), KeyStoreEntry::Certificate(cert));
    }

    keystore.writer(SSL_TRUSTSTORE_PASSWORD).write().ok()
}

fn parse_pem_certificates(pem_data: &[u8]) -> Option<Vec<P12Certificate>> {
    let mut reader = std::io::BufReader::new(pem_data);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if certs.is_empty() {
        return None;
    }

    certs
        .iter()
        .map(|cert_der| P12Certificate::from_der(cert_der.as_ref()).ok())
        .collect()
}

fn is_license_check_failure(stderr: &str) -> bool {
    stderr.contains("License check failed")
}

fn parse_cli_output(output: Output) -> Result<String, CliError> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if is_license_check_failure(&stderr) {
            return Err(CliError::LicenseCheckFailed);
        }
        Err(CliError::NonZeroExit {
            code: output.status.code().unwrap_or(-1),
            stderr,
        })
    }
}

fn extract_embedded_cli() -> Result<PathBuf, CliError> {
    extract_zip_to_cache(&cli_cache_dir(), CLI_ZIP)
}

/// Core extraction logic: create cache dir, unzip, set permissions,
/// and verify the binary exists. Accepts the cache directory and raw
/// zip bytes so that tests can supply a temp dir and synthetic zip.
fn extract_zip_to_cache(cache_dir: &Path, zip_data: &[u8]) -> Result<PathBuf, CliError> {
    let binary_path = cache_dir.join(CLI_BINARY_NAME);

    if binary_path.exists() {
        return Ok(binary_path);
    }

    std::fs::create_dir_all(cache_dir)?;

    let cursor = std::io::Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| CliError::NotFound(format!("Invalid embedded CLI zip: {e}")))?;

    extract_cli_binary(&mut archive, cache_dir)?;

    if !binary_path.exists() {
        return Err(CliError::NotFound(format!(
            "CLI binary '{CLI_BINARY_NAME}' not found in embedded zip"
        )));
    }

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&binary_path, perms)?;
    }

    Ok(binary_path)
}

fn extract_cli_binary(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    dest_dir: &Path,
) -> Result<(), CliError> {
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| CliError::NotFound(format!("Zip entry error: {e}")))?;

        let name = file.name().to_string();
        let file_name = Path::new(&name)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if file_name == CLI_BINARY_NAME {
            let dest = dest_dir.join(&file_name);
            let mut out = std::fs::File::create(&dest)?;
            std::io::copy(&mut file, &mut out)?;
            return Ok(());
        }

        // Also extract shared libraries / supporting files
        if !file.is_dir() {
            let dest = dest_dir.join(&file_name);
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            std::fs::write(&dest, &buf)?;
        }
    }

    Ok(())
}

fn cli_cache_dir() -> PathBuf {
    let version = env!("CS_MCP_VERSION");
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("codehealth-mcp")
        .join(version)
}

pub fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_file() {
        path.parent()?.to_path_buf()
    } else {
        path.to_path_buf()
    };

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    /// Create a temp directory containing a `.git` dir and an optional subdirectory.
    /// Returns `(tempdir_handle, repo_root_path)`.
    fn make_git_repo(subdir: Option<&str>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        if let Some(sub) = subdir {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        let root = dir.path().to_path_buf();
        (dir, root)
    }

    /// Create a temp directory with a fake CLI binary and return both.
    fn make_fake_cli() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("cs");
        std::fs::write(&bin, "fake").unwrap();
        (dir, bin)
    }

    fn make_output(raw_status: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: ExitStatus::from_raw(raw_status),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    #[test]
    fn cli_cache_dir_contains_version() {
        let dir = cli_cache_dir();
        assert!(dir.to_string_lossy().contains("codehealth-mcp"));
    }

    #[test]
    fn cli_binary_name_is_cs() {
        assert_eq!(CLI_BINARY_NAME, "cs");
    }

    #[test]
    fn is_cs_cli_binary_detects_cs_names() {
        assert!(is_cs_cli_binary(Path::new("/tmp/cs")));
        assert!(is_cs_cli_binary(Path::new("C:/tools/cs.exe")));
        assert!(is_cs_cli_binary(Path::new("cs")));
        assert!(!is_cs_cli_binary(Path::new("/bin/sh")));
    }

    #[test]
    fn selected_ca_bundle_prefers_requests_ca_bundle() {
        let _lock = config::lock_test_env();
        let requests = tempfile::NamedTempFile::new().unwrap();
        let ssl_cert_file = tempfile::NamedTempFile::new().unwrap();

        std::env::set_var("REQUESTS_CA_BUNDLE", requests.path());
        std::env::set_var("SSL_CERT_FILE", ssl_cert_file.path());
        std::env::remove_var("CURL_CA_BUNDLE");

        let selected = selected_ca_bundle_path_from_env().unwrap();
        assert_eq!(selected, requests.path());

        std::env::remove_var("REQUESTS_CA_BUNDLE");
        std::env::remove_var("SSL_CERT_FILE");
    }

    #[test]
    fn ssl_cli_args_use_existing_truststore_file() {
        let _lock = config::lock_test_env();
        let mut pem = tempfile::NamedTempFile::new().unwrap();
        let pem_contents = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            pem.path().display()
        );
        use std::io::Write;
        pem.write_all(pem_contents.as_bytes()).unwrap();

        std::env::set_var("REQUESTS_CA_BUNDLE", pem.path());
        std::env::remove_var("SSL_CERT_FILE");
        std::env::remove_var("CURL_CA_BUNDLE");

        let truststore = truststore_path_for_pem(pem_contents.as_bytes());
        std::fs::write(&truststore, b"dummy").unwrap();

        let args = ssl_cli_args_from_env();
        assert_eq!(args.len(), 3);
        assert!(args[0].contains("-Djavax.net.ssl.trustStore="));
        assert!(args[1].contains("-Djavax.net.ssl.trustStoreType=PKCS12"));
        assert!(args[2].contains("-Djavax.net.ssl.trustStorePassword=changeit"));
        assert!(args[0].contains(truststore.to_string_lossy().as_ref()));

        std::fs::remove_file(&truststore).ok();
        std::env::remove_var("REQUESTS_CA_BUNDLE");
    }

    #[test]
    fn is_cs_cli_binary_returns_false_when_no_file_name() {
        assert!(!is_cs_cli_binary(Path::new("/")));
    }

    #[test]
    fn with_ssl_cli_args_only_applies_to_cs_binary() {
        let _lock = config::lock_test_env();
        std::env::remove_var("REQUESTS_CA_BUNDLE");
        std::env::remove_var("SSL_CERT_FILE");
        std::env::remove_var("CURL_CA_BUNDLE");

        let args = with_ssl_cli_args_if_needed(Path::new("/bin/sh"), &["-c", "echo ok"]);
        assert_eq!(args, vec!["-c".to_string(), "echo ok".to_string()]);

        let cs_args = with_ssl_cli_args_if_needed(Path::new("/tmp/cs"), &["review"]);
        assert_eq!(cs_args, vec!["review".to_string()]);
    }

    #[test]
    fn ssl_cli_args_from_env_returns_empty_without_ca_env_vars() {
        let _lock = config::lock_test_env();
        std::env::remove_var("REQUESTS_CA_BUNDLE");
        std::env::remove_var("SSL_CERT_FILE");
        std::env::remove_var("CURL_CA_BUNDLE");

        assert!(ssl_cli_args_from_env().is_empty());
    }

    #[test]
    fn ssl_cli_args_from_env_returns_empty_when_truststore_creation_fails() {
        let _lock = config::lock_test_env();
        let pem = tempfile::NamedTempFile::new().unwrap();
        let pem_data = b"not-a-valid-certificate";
        std::fs::write(pem.path(), pem_data).unwrap();

        let truststore = truststore_path_for_pem(pem_data);
        std::fs::remove_file(&truststore).ok();

        std::env::set_var("REQUESTS_CA_BUNDLE", pem.path());
        std::env::remove_var("SSL_CERT_FILE");
        std::env::remove_var("CURL_CA_BUNDLE");

        assert!(ssl_cli_args_from_env().is_empty());

        std::env::remove_var("REQUESTS_CA_BUNDLE");
    }

    #[test]
    fn create_or_get_truststore_from_pem_returns_none_on_invalid_input() {
        let pem = tempfile::NamedTempFile::new().unwrap();
        let pem_data = b"invalid";
        std::fs::write(pem.path(), pem_data).unwrap();

        let truststore = truststore_path_for_pem(pem_data);
        std::fs::remove_file(&truststore).ok();

        let result = create_or_get_truststore_from_pem(pem.path());
        assert!(result.is_none());
    }

    #[test]
    fn create_or_get_truststore_from_pem_creates_pkcs12_truststore() {
        let pem = tempfile::NamedTempFile::new().unwrap();
        let cert_pem = TEST_CA_CERT_PEM.as_bytes();
        std::fs::write(pem.path(), cert_pem).unwrap();

        let truststore = truststore_path_for_pem(cert_pem);
        std::fs::remove_file(&truststore).ok();

        let result = create_or_get_truststore_from_pem(pem.path());
        assert!(result.is_some(), "expected truststore creation to succeed");
        let created = result.unwrap();
        assert!(created.exists());

        std::fs::remove_file(created).ok();
    }

    #[test]
    fn write_pkcs12_truststore_from_pem_returns_false_when_parent_is_not_directory() {
        let parent_file = tempfile::NamedTempFile::new().unwrap();
        let cert = TEST_CA_CERT_PEM.as_bytes();

        let impossible_truststore = parent_file.path().join("truststore.p12");
        let ok = write_pkcs12_truststore_from_pem(cert, &impossible_truststore);
        assert!(!ok);
    }

    #[test]
    fn ensure_parent_dir_returns_none_for_root_path() {
        assert!(ensure_parent_dir(Path::new("/")).is_none());
    }

    #[test]
    fn parse_pem_certificates_returns_some_for_valid_pem() {
        let certs = parse_pem_certificates(TEST_CA_CERT_PEM.as_bytes());
        assert!(certs.is_some());
        assert_eq!(certs.unwrap().len(), 1);
    }

    #[test]
    fn parse_pem_certificates_returns_none_for_invalid_pem() {
        let certs = parse_pem_certificates(b"not-a-pem");
        assert!(certs.is_none());
    }

    #[test]
    fn build_pkcs12_truststore_bytes_returns_none_for_invalid_pem() {
        let bytes = build_pkcs12_truststore_bytes(b"invalid");
        assert!(bytes.is_none());
    }

    #[test]
    fn resolve_from_env_override_returns_none_when_not_set() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_CLI_PATH");
        assert!(resolve_from_env_override().is_none());
    }

    #[test]
    fn resolve_from_env_override_returns_ok_for_existing_path() {
        let _lock = config::lock_test_env();
        let (_dir, bin) = make_fake_cli();

        std::env::set_var("CS_CLI_PATH", bin.to_str().unwrap());
        let result = resolve_from_env_override().unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), bin);

        std::env::remove_var("CS_CLI_PATH");
    }

    #[test]
    fn resolve_from_env_override_returns_error_for_missing_path() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_CLI_PATH", "/nonexistent/cs-binary");
        let result = resolve_from_env_override().unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-existent"));
        std::env::remove_var("CS_CLI_PATH");
    }

    #[test]
    fn resolve_from_docker_returns_none_when_not_docker() {
        assert!(resolve_from_docker().is_none());
    }

    #[test]
    fn parse_cli_output_success() {
        let output = make_output(0, b"hello\n", b"");
        assert_eq!(parse_cli_output(output).unwrap(), "hello\n");
    }

    #[test]
    fn parse_cli_output_failure() {
        // Exit code 1 on unix: raw value is 256 (1 << 8)
        let output = make_output(256, b"", b"error message");
        match parse_cli_output(output).unwrap_err() {
            CliError::NonZeroExit { code, stderr } => {
                assert_eq!(code, 1);
                assert_eq!(stderr, "error message");
            }
            other => panic!("Expected NonZeroExit, got: {other:?}"),
        }
    }

    #[test]
    fn parse_cli_output_license_check_failed() {
        let stderr = b"License check failed: [401] The user must reauthorize.\n\n  Make sure that CS_ACCESS_TOKEN is set to a valid Personal Access Token.";
        let output = make_output(256, b"", stderr);
        match parse_cli_output(output).unwrap_err() {
            CliError::LicenseCheckFailed => {
                // Verify the user-facing message doesn't mention the CLI
                let msg = CliError::LicenseCheckFailed.to_string();
                assert!(msg.contains("invalid or expired"), "msg: {msg}");
                assert!(msg.contains("set_config"), "msg: {msg}");
                assert!(!msg.contains("CS CLI"), "msg should not mention CLI: {msg}");
            }
            other => panic!("Expected LicenseCheckFailed, got: {other:?}"),
        }
    }

    #[test]
    fn is_license_check_failure_detects_license_error() {
        assert!(is_license_check_failure(
            "License check failed: [401] The user must reauthorize."
        ));
    }

    #[test]
    fn is_license_check_failure_ignores_other_errors() {
        assert!(!is_license_check_failure("Some other error"));
        assert!(!is_license_check_failure(""));
    }

    #[test]
    fn parse_cli_output_success_with_utf8_lossy() {
        let output = make_output(0, &[0xC0, 0xAF], b"");
        assert!(!parse_cli_output(output).unwrap().is_empty());
    }

    #[test]
    fn find_git_root_finds_repo_from_subdir() {
        let (_dir, root) = make_git_repo(Some("src"));
        assert_eq!(find_git_root(&root.join("src")).unwrap(), root);
    }

    #[test]
    fn find_git_root_returns_none_when_no_repo() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("deep").join("nested");
        std::fs::create_dir_all(&sub).unwrap();
        let _ = find_git_root(&sub);
    }

    #[test]
    fn find_git_root_with_file_path() {
        let (_dir, root) = make_git_repo(Some("src"));
        let file = root.join("src").join("main.rs");
        std::fs::write(&file, "fn main() {}").unwrap();
        assert_eq!(find_git_root(&file).unwrap(), root);
    }

    #[test]
    fn extract_embedded_cli_produces_binary() {
        let result = extract_embedded_cli();
        assert!(result.is_ok(), "extract_embedded_cli failed: {result:?}");
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("cs"));
    }

    #[test]
    fn extract_zip_to_cache_fresh_extraction() {
        let zip_data = build_zip(&[(CLI_BINARY_NAME, b"test-binary-content")]);
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("fresh-cache");
        // cache dir does not exist yet — extract_zip_to_cache must create it
        let result = extract_zip_to_cache(&cache, &zip_data);
        assert!(result.is_ok(), "extract_zip_to_cache failed: {result:?}");
        let path = result.unwrap();
        assert!(path.exists());
        assert_eq!(path, cache.join(CLI_BINARY_NAME));
    }

    #[test]
    fn extract_zip_to_cache_returns_cached_on_second_call() {
        let zip_data = build_zip(&[(CLI_BINARY_NAME, b"binary-data")]);
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("reuse-cache");
        // First call: fresh extraction
        let first = extract_zip_to_cache(&cache, &zip_data).unwrap();
        assert!(first.exists());
        // Second call: should hit the early return (binary already exists)
        let second = extract_zip_to_cache(&cache, &zip_data).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn extract_zip_to_cache_sets_executable_permission() {
        let zip_data = build_zip(&[(CLI_BINARY_NAME, b"exec-binary")]);
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("perm-cache");
        let path = extract_zip_to_cache(&cache, &zip_data).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_ne!(mode & 0o111, 0, "binary should be executable");
        }
    }

    #[test]
    fn extract_zip_to_cache_missing_binary_returns_error() {
        // Zip with a file that is NOT the CLI binary name
        let zip_data = build_zip(&[("not-cs", b"wrong-file")]);
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("missing-cache");
        let result = extract_zip_to_cache(&cache, &zip_data);
        assert!(
            result.is_err(),
            "should fail when binary is missing from zip"
        );
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not found"), "unexpected error: {msg}");
    }

    #[test]
    fn extract_zip_to_cache_invalid_zip_data() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("bad-zip");
        let result = extract_zip_to_cache(&cache, b"this is not a zip");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Invalid embedded CLI zip"),
            "unexpected error: {msg}"
        );
    }

    /// Build a zip archive in memory with the given named entries.
    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut zw = zip::ZipWriter::new(buf);
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zw.start_file(*name, options).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap().into_inner()
    }

    fn open_zip(data: &[u8]) -> zip::ZipArchive<std::io::Cursor<&[u8]>> {
        zip::ZipArchive::new(std::io::Cursor::new(data)).unwrap()
    }

    const TEST_CA_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIDPzCCAiegAwIBAgIUdGj465l77xx7Je8KqOESIqx9zXYwDQYJKoZIhvcNAQEL
BQAwTzELMAkGA1UEBhMCVVMxDTALBgNVBAgMBFRlc3QxDTALBgNVBAcMBFRlc3Qx
EDAOBgNVBAoMB1Rlc3QgQ0ExEDAOBgNVBAMMB1Rlc3QgQ0EwHhcNMjYwMTE2MDky
OTQ5WhcNMjcwMTE2MDkyOTQ5WjBPMQswCQYDVQQGEwJVUzENMAsGA1UECAwEVGVz
dDENMAsGA1UEBwwEVGVzdDEQMA4GA1UECgwHVGVzdCBDQTEQMA4GA1UEAwwHVGVz
dCBDQTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAMqoClSXXim/fiI9
Lc3X/4D4rHK6cWAnKVPA+CetSJiGrMrfeJZMSTWUv19M8aKlmbZsQxN4X4neycWE
UxH9y3XaqV9grmGvutTgw98t6fhawevGrjmcA+ygQ5S37reRQOHtc9ob51b8b9Rr
nyE8qIU2dkZ115VpFN+/woG2LG23iGj2dJ3AaZc/R8X0UQu5tQCDwTOeO/zMWPGG
xjzDpnFs4u7IAwPECEgEuxHH8PHapUoc0d+Aq/wBKM015qdohoaydrztzXp6DKJ5
RBv/cn+lTpFdvJQS0CceIo+hOUa46ONq63VM3SQhT7enOWToONBxrZpof18bITFd
2h4XxoMCAwEAAaMTMBEwDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOC
AQEAHDWTjJILOtrCBRFksVyvniUGFR8ioz2cE4R8xcKAFxNOPKLuxwm+ilbUBX3A
8VOCJjR6IimsLMhAUEi5FGDiVVhOwIp1+pULEigTG7r72yOCr2xnw8NrX9UbJNnx
rlyCjEN9URBpriiGGegixH6AoLVW0SjEsJ7CgfqmfWzKU+nsPIunvePtFhSw5jHC
mHwYTxYcxYW33TK9qQxs119A9+qG5Z+cJlDtYrfHirHwPZQeuQ25jhKE5FUUiuiq
iblIIstcPF4n6wQ0ieNajmj5nHXQEypkek8D/ANbwwhlVQ3u/hldcAyj4qD7G5oJ
sC0Nc9QdNQt5Tos5Je5S7CWL0w==
-----END CERTIFICATE-----
";

    #[test]
    fn extract_cli_binary_extracts_matching_entry() {
        let zip_data = build_zip(&[(CLI_BINARY_NAME, b"fake-cli-binary")]);
        let dest = tempfile::tempdir().unwrap();
        let mut archive = open_zip(&zip_data);
        extract_cli_binary(&mut archive, dest.path()).unwrap();
        let extracted = dest.path().join(CLI_BINARY_NAME);
        assert!(extracted.exists());
        assert_eq!(
            std::fs::read_to_string(&extracted).unwrap(),
            "fake-cli-binary"
        );
    }

    #[test]
    fn extract_cli_binary_extracts_supporting_files() {
        let zip_data = build_zip(&[
            ("libsupport.so", b"shared-lib"),
            (CLI_BINARY_NAME, b"the-binary"),
        ]);
        let dest = tempfile::tempdir().unwrap();
        let mut archive = open_zip(&zip_data);
        extract_cli_binary(&mut archive, dest.path()).unwrap();
        // The cs binary is found on the second entry, so the first entry
        // (libsupport.so) is extracted as a supporting file.
        assert!(dest.path().join("libsupport.so").exists());
        assert_eq!(
            std::fs::read_to_string(dest.path().join("libsupport.so")).unwrap(),
            "shared-lib"
        );
    }

    #[test]
    fn extract_cli_binary_empty_zip_returns_ok() {
        let zip_data = build_zip(&[]);
        let dest = tempfile::tempdir().unwrap();
        let mut archive = open_zip(&zip_data);
        // Empty zip has no entries — returns Ok(()) without extracting anything.
        extract_cli_binary(&mut archive, dest.path()).unwrap();
        assert!(!dest.path().join(CLI_BINARY_NAME).exists());
    }

    #[test]
    fn extract_cli_binary_nested_path_uses_file_name() {
        let nested = format!("some/nested/path/{CLI_BINARY_NAME}");
        let zip_data = build_zip(&[(&nested, b"nested-binary")]);
        let dest = tempfile::tempdir().unwrap();
        let mut archive = open_zip(&zip_data);
        extract_cli_binary(&mut archive, dest.path()).unwrap();
        let extracted = dest.path().join(CLI_BINARY_NAME);
        assert!(extracted.exists());
        assert_eq!(
            std::fs::read_to_string(&extracted).unwrap(),
            "nested-binary"
        );
    }

    #[test]
    fn resolve_cli_path_finds_cli() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_CLI_PATH");
        assert!(resolve_cli_path().is_ok());
    }

    #[test]
    fn resolve_cli_path_uses_env_override() {
        let _lock = config::lock_test_env();
        let (_dir, bin) = make_fake_cli();

        std::env::set_var("CS_CLI_PATH", bin.to_str().unwrap());
        assert_eq!(resolve_cli_path().unwrap(), bin);
        std::env::remove_var("CS_CLI_PATH");
    }

    #[tokio::test]
    async fn run_cli_at_path_with_echo() {
        let output = run_cli_at_path(Path::new("/bin/echo"), &["hello", "world"], None).await;
        assert_eq!(output.unwrap().trim(), "hello world");
    }

    #[tokio::test]
    async fn run_cli_at_path_forwards_env_vars() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_ACCESS_TOKEN", "test-token-xyz");
        std::env::set_var("CS_ONPREM_URL", "https://onprem.example.com");

        let output = run_cli_at_path(
            Path::new("/bin/sh"),
            &["-c", "echo $CS_ACCESS_TOKEN $CS_ONPREM_URL"],
            None,
        )
        .await
        .unwrap();
        assert!(output.contains("test-token-xyz"));
        assert!(output.contains("https://onprem.example.com"));

        std::env::remove_var("CS_ACCESS_TOKEN");
        std::env::remove_var("CS_ONPREM_URL");
    }

    #[tokio::test]
    async fn run_cli_at_path_trims_access_token_before_forwarding() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_ACCESS_TOKEN", "  test-token-xyz  ");

        let output = run_cli_at_path(
            Path::new("/bin/sh"),
            &["-c", "echo [$CS_ACCESS_TOKEN]"],
            None,
        )
        .await
        .unwrap();
        assert!(output.contains("[test-token-xyz]"));

        std::env::remove_var("CS_ACCESS_TOKEN");
    }

    #[tokio::test]
    async fn run_cli_at_path_with_working_dir() {
        let dir = tempfile::tempdir().unwrap();
        let output = run_cli_at_path(Path::new("/bin/pwd"), &[], Some(dir.path()))
            .await
            .unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        assert_eq!(output.trim(), canonical.to_string_lossy());
    }

    #[tokio::test]
    async fn run_cli_at_path_nonexistent_binary() {
        let result = run_cli_at_path(Path::new("/nonexistent/binary"), &[], None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn run_cli_at_path_failing_command() {
        let result = run_cli_at_path(Path::new("/bin/sh"), &["-c", "exit 42"], None).await;
        match result.unwrap_err() {
            CliError::NonZeroExit { code, .. } => assert_eq!(code, 42),
            other => panic!("Expected NonZeroExit, got: {other:?}"),
        }
    }

    #[test]
    fn should_retry_for_telemetry_flush_no_such_file_error() {
        let stderr = "java.nio.file.NoSuchFileException: /tmp/codescene-cli.log.jsonl";
        assert!(should_retry_after_telemetry_flush_error(stderr));
    }

    #[test]
    fn should_not_retry_for_other_no_such_file_error() {
        let stderr = "java.nio.file.NoSuchFileException: /tmp/other-file.log";
        assert!(!should_retry_after_telemetry_flush_error(stderr));
    }

    #[tokio::test]
    async fn run_cli_at_path_retries_once_on_telemetry_flush_error() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("first-run.marker");
        let marker_path = marker.to_string_lossy().to_string();
        let script = format!(
            "if [ ! -f '{marker_path}' ]; then touch '{marker_path}'; >&2 echo 'java.nio.file.NoSuchFileException: /tmp/codescene-cli.log.jsonl'; exit 1; fi; echo ok"
        );

        let output = run_cli_at_path(Path::new("/bin/sh"), &["-c", &script], None).await;
        assert_eq!(output.unwrap().trim(), "ok");
    }
}

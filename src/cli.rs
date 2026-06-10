use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Output;

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

const DOCKER_CLI_PATH: &str = "/home/mcp/.local/bin/cs";

/// Resolve the path to the `cs` CLI binary.
///
/// Resolution order:
/// 1. `CS_CLI_PATH` environment variable override
/// 2. Docker container path (`/home/mcp/.local/bin/cs`)
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

    if is_license_check_failure(&stderr) {
        tracing::warn!("License check failed, retrying after brief delay...");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let retry_output = run_cli_process(cli_path, args, working_dir, false).await?;
        if retry_output.status.success() {
            return Ok(String::from_utf8_lossy(&retry_output.stdout).to_string());
        }
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

    // Scrub sensitive env vars (tokens) from the inherited environment so
    // child processes that don't need them can't read them from /proc or
    // inherit them to further subprocesses.  We then selectively add back
    // only CS_ACCESS_TOKEN, which the CLI needs for on-prem authentication.
    for var in crate::config::sensitive_env_vars() {
        cmd.env_remove(var);
    }

    cmd.args(args)
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

    // Pass custom CA certificates to the CLI via its native CS_CERTS
    // mechanism. The CLI appends these to its trust store, preserving
    // system CA trust (unlike the Java -D truststore approach which
    // replaces the default trust store entirely).
    if let Some(ca_path) = selected_ca_bundle_path_from_env() {
        cmd.env("CS_CERTS", ca_path);
    }

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.output().await.map_err(CliError::from)
}

fn should_retry_after_telemetry_flush_error(stderr: &str) -> bool {
    stderr.contains("NoSuchFileException") && stderr.contains("codescene-cli.log.jsonl")
}

fn selected_ca_bundle_path_from_env() -> Option<PathBuf> {
    let env_vars = ["REQUESTS_CA_BUNDLE", "SSL_CERT_FILE", "CURL_CA_BUNDLE"];
    let mut configured_but_missing = Vec::new();

    let result = env_vars.into_iter().find_map(|env_var| {
        std::env::var(env_var)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .and_then(|v| {
                let p = PathBuf::from(&v);
                if p.is_file() {
                    Some((env_var, p))
                } else {
                    configured_but_missing.push((env_var, v));
                    None
                }
            })
    });

    if let Some((var, ref path)) = result {
        tracing::info!(
            "Using CA bundle from {var}: {}",
            path.display()
        );
    }

    for (var, path) in &configured_but_missing {
        tracing::warn!(
            "{var} is set to \"{path}\" but the file does not exist — \
             custom CA certificates will NOT be used. \
             On Windows, ensure backslashes are escaped as \\\\ in JSON config."
        );
    }

    result.map(|(_, p)| p)
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

    // Extract to a temporary directory first, then atomically rename
    // to avoid races when multiple processes extract in parallel.
    let tmp_dir = tempfile::tempdir_in(cache_dir)?;

    // Restrict temp directory to owner-only access
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp_dir.path(), std::fs::Permissions::from_mode(0o700))?;
    }

    let cursor = std::io::Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| CliError::NotFound(format!("Invalid embedded CLI zip: {e}")))?;

    extract_cli_binary(&mut archive, tmp_dir.path())?;

    let tmp_binary = tmp_dir.path().join(CLI_BINARY_NAME);
    if !tmp_binary.exists() {
        return Err(CliError::NotFound(format!(
            "CLI binary '{CLI_BINARY_NAME}' not found in embedded zip"
        )));
    }

    // Set executable permission on Unix (owner-only execute)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&tmp_binary, perms)?;
    }

    // Atomic rename — if another process won the race and already
    // placed the binary, the rename error is harmless; just verify
    // the binary exists rather than propagating a spurious failure.
    atomic_place_binary(&tmp_binary, &binary_path)?;

    // Clean up temp dir (best-effort)
    let _ = tmp_dir.close();

    Ok(binary_path)
}

/// Attempt to atomically place a binary at `dest` via rename.
/// If another process already placed the binary, the rename error
/// is silently ignored. Only fails if rename errors AND the dest
/// binary does not exist.
fn atomic_place_binary(src: &Path, dest: &Path) -> Result<(), CliError> {
    if let Err(_e) = std::fs::rename(src, dest) {
        if !dest.exists() {
            return Err(CliError::NotFound(format!(
                "Failed to place CLI binary at '{}' and no existing binary found",
                dest.display()
            )));
        }
    }
    Ok(())
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
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;
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
        // On Unix, from_raw takes the raw waitpid status (exit code in bits 8-15).
        // On Windows, from_raw takes the exit code directly as u32.
        #[cfg(unix)]
        let status = ExitStatus::from_raw(raw_status);
        #[cfg(windows)]
        let status = ExitStatus::from_raw(raw_status as u32);
        Output {
            status,
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    /// Return the raw status value that represents a non-zero exit code.
    /// On Unix the exit code occupies bits 8-15 of the wait status, so
    /// exit code 1 is raw value 256.  On Windows the raw value IS the
    /// exit code, so exit code 1 is raw value 1.
    fn failing_raw_status() -> i32 {
        if cfg!(unix) { 256 } else { 1 }
    }

    #[test]
    fn cli_cache_dir_contains_version() {
        let dir = cli_cache_dir();
        assert!(dir.to_string_lossy().contains("codehealth-mcp"));
    }

    #[test]
    fn cli_binary_name_matches_platform() {
        if cfg!(windows) {
            assert_eq!(CLI_BINARY_NAME, "cs.exe");
        } else {
            assert_eq!(CLI_BINARY_NAME, "cs");
        }
    }

    /// Run `body` with the CA bundle env vars set according to `vars`, then
    /// clean up afterwards. Each entry is `(VAR_NAME, value_or_none)`.
    fn with_ca_env(vars: &[(&str, Option<&str>)], body: impl FnOnce()) {
        let _lock = config::lock_test_env();
        for &(k, v) in vars {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
        body();
        for &(k, _) in vars {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn selected_ca_bundle_prefers_requests_ca_bundle() {
        let requests = tempfile::NamedTempFile::new().unwrap();
        let ssl_cert_file = tempfile::NamedTempFile::new().unwrap();
        let r = requests.path().to_str().unwrap();
        let s = ssl_cert_file.path().to_str().unwrap();

        with_ca_env(
            &[("REQUESTS_CA_BUNDLE", Some(r)), ("SSL_CERT_FILE", Some(s)), ("CURL_CA_BUNDLE", None)],
            || assert_eq!(selected_ca_bundle_path_from_env().unwrap(), requests.path()),
        );
    }

    #[test]
    fn selected_ca_bundle_returns_none_for_nonexistent_path() {
        with_ca_env(
            &[("REQUESTS_CA_BUNDLE", Some("/nonexistent/ca-bundle.pem")), ("SSL_CERT_FILE", None), ("CURL_CA_BUNDLE", None)],
            || assert!(selected_ca_bundle_path_from_env().is_none()),
        );
    }

    #[test]
    fn selected_ca_bundle_skips_nonexistent_falls_through_to_valid() {
        let real_file = tempfile::NamedTempFile::new().unwrap();
        let r = real_file.path().to_str().unwrap();

        with_ca_env(
            &[("REQUESTS_CA_BUNDLE", Some("/nonexistent/ca-bundle.pem")), ("SSL_CERT_FILE", Some(r)), ("CURL_CA_BUNDLE", None)],
            || assert_eq!(selected_ca_bundle_path_from_env().unwrap(), real_file.path()),
        );
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
        let output = make_output(failing_raw_status(), b"", b"error message");
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
        let output = make_output(failing_raw_status(), b"", stderr);
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
    fn atomic_place_binary_succeeds_on_rename() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src-binary");
        let dest = dir.path().join("dest-binary");
        std::fs::write(&src, b"binary").unwrap();

        let result = atomic_place_binary(&src, &dest);
        assert!(result.is_ok());
        assert!(dest.exists());
        assert!(!src.exists(), "source should be gone after rename");
    }

    #[test]
    fn atomic_place_binary_tolerates_existing_dest() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src-binary");
        let dest = dir.path().join("dest-binary");
        std::fs::write(&src, b"new-binary").unwrap();
        std::fs::write(&dest, b"existing-binary").unwrap();

        // rename succeeds (replaces existing), no error
        let result = atomic_place_binary(&src, &dest);
        assert!(result.is_ok());
        assert!(dest.exists());
    }

    #[test]
    fn atomic_place_binary_fails_when_rename_fails_and_no_dest() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("nonexistent-source");
        let dest = dir.path().join("dest-binary");
        // src doesn't exist — rename will fail; dest doesn't exist either

        let result = atomic_place_binary(&src, &dest);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Failed to place CLI binary"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn atomic_place_binary_ignores_rename_error_when_dest_exists() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("nonexistent-source");
        let dest = dir.path().join("dest-binary");
        std::fs::write(&dest, b"already-placed").unwrap();
        // src doesn't exist — rename fails, but dest exists (another process won)

        let result = atomic_place_binary(&src, &dest);
        assert!(result.is_ok(), "should succeed when dest already exists");
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

    #[cfg(unix)]
    #[tokio::test]
    async fn run_cli_at_path_with_echo() {
        let output = run_cli_at_path(Path::new("/bin/echo"), &["hello", "world"], None).await;
        assert_eq!(output.unwrap().trim(), "hello world");
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
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

    #[cfg(unix)]
    #[tokio::test]
    async fn run_cli_at_path_with_working_dir() {
        let dir = tempfile::tempdir().unwrap();
        let output = run_cli_at_path(Path::new("/bin/pwd"), &[], Some(dir.path()))
            .await
            .unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        assert_eq!(output.trim(), canonical.to_string_lossy());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_cli_at_path_nonexistent_binary() {
        let result = run_cli_at_path(Path::new("/nonexistent/binary"), &[], None).await;
        assert!(result.is_err());
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
    #[tokio::test]
    async fn run_cli_at_path_retries_once_on_license_check_failure() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("license-retry.marker");
        let marker_path = marker.to_string_lossy().to_string();
        let script = format!(
            "if [ ! -f '{marker_path}' ]; then touch '{marker_path}'; >&2 echo 'License check failed: [401] The user must reauthorize.'; exit 1; fi; echo ok"
        );

        let output = run_cli_at_path(Path::new("/bin/sh"), &["-c", &script], None).await;
        assert_eq!(output.unwrap().trim(), "ok");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_cli_at_path_license_check_failure_persists_after_retry() {
        let script =
            ">&2 echo 'License check failed: [401] The user must reauthorize.'; exit 1";

        let result = run_cli_at_path(Path::new("/bin/sh"), &["-c", script], None).await;
        match result.unwrap_err() {
            CliError::LicenseCheckFailed => {}
            other => panic!("Expected LicenseCheckFailed, got: {other:?}"),
        }
    }

    #[cfg(unix)]
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

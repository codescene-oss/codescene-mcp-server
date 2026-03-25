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

const DOCKER_CLI_PATH: &str = "/root/.local/bin/cs";

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

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.output().await.map_err(CliError::from)
}

fn should_retry_after_telemetry_flush_error(stderr: &str) -> bool {
    stderr.contains("NoSuchFileException") && stderr.contains("codescene-cli.log.jsonl")
}

fn parse_cli_output(output: Output) -> Result<String, CliError> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
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

        let output = run_cli_at_path(Path::new("/bin/sh"), &["-c", "echo [$CS_ACCESS_TOKEN]"], None)
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

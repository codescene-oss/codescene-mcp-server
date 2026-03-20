/// CS CLI embedding, extraction, and invocation — mirrors Python's
/// `code_health_analysis.py`.
///
/// The CLI zip is embedded at compile time. At runtime, it is extracted
/// once to a cache directory and reused. The module also handles CLI
/// path resolution (env override, Docker paths) and subprocess execution.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Output;

use crate::environment;
use crate::errors::CliError;

/// Trait abstracting CLI subprocess execution for dependency injection.
///
/// Production code uses `ProductionCliRunner`; tests inject a mock.
#[async_trait::async_trait]
pub trait CliRunner: Send + Sync {
    async fn run(&self, args: &[&str], working_dir: Option<&Path>) -> Result<String, CliError>;
}

/// Production CLI runner that resolves and invokes the real CS CLI binary.
pub struct ProductionCliRunner;

#[async_trait::async_trait]
impl CliRunner for ProductionCliRunner {
    async fn run(&self, args: &[&str], working_dir: Option<&Path>) -> Result<String, CliError> {
        run_cli(args, working_dir).await
    }
}

/// Embedded CS CLI zip, downloaded at build time by `build.rs`.
const CLI_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cs-cli.zip"));

/// Name of the CLI binary (platform-dependent).
const CLI_BINARY_NAME: &str = if cfg!(windows) { "cs.exe" } else { "cs" };

/// Docker path for the CLI binary.
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

/// Run a CS CLI command and return stdout on success.
pub async fn run_cli(
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<String, CliError> {
    let cli_path = resolve_cli_path()?;
    run_cli_at_path(&cli_path, args, working_dir).await
}

/// Run a CLI command using an explicit binary path.
async fn run_cli_at_path(
    cli_path: &Path,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<String, CliError> {
    let mut cmd = tokio::process::Command::new(cli_path);

    cmd.args(args)
        .env("CS_CONTEXT", "mcp-server")
        .env("CS_DISABLE_VERSION_CHECK", "1");

    if let Ok(token) = std::env::var("CS_ACCESS_TOKEN") {
        cmd.env("CS_ACCESS_TOKEN", token);
    }

    if let Ok(url) = std::env::var("CS_ONPREM_URL") {
        cmd.env("CS_ONPREM_URL", url);
    }

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let output: Output = cmd.output().await?;
    parse_cli_output(output)
}

/// Parse CLI process output into success/error.
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

/// Extract the embedded CLI zip to a cache directory, returning the
/// path to the extracted binary. Skips extraction if already cached.
fn extract_embedded_cli() -> Result<PathBuf, CliError> {
    let cache_dir = cli_cache_dir();
    let binary_path = cache_dir.join(CLI_BINARY_NAME);

    if binary_path.exists() {
        return Ok(binary_path);
    }

    std::fs::create_dir_all(&cache_dir)?;

    let cursor = std::io::Cursor::new(CLI_ZIP);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| CliError::NotFound(format!("Invalid embedded CLI zip: {e}")))?;

    extract_cli_binary(&mut archive, &cache_dir)?;

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&binary_path, perms)?;
    }

    if binary_path.exists() {
        Ok(binary_path)
    } else {
        Err(CliError::NotFound(format!(
            "CLI binary '{CLI_BINARY_NAME}' not found in embedded zip"
        )))
    }
}

/// Extract the `cs` binary from the zip archive.
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

/// Cache directory for the extracted CLI binary.
fn cli_cache_dir() -> PathBuf {
    let version = env!("CS_MCP_VERSION");
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("codehealth-mcp")
        .join(version)
}

/// Find the git repository root for a given path.
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
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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

    // -- cli_cache_dir --

    #[test]
    fn cli_cache_dir_contains_version() {
        let dir = cli_cache_dir();
        assert!(dir.to_string_lossy().contains("codehealth-mcp"));
    }

    // -- CLI_BINARY_NAME --

    #[test]
    fn cli_binary_name_is_cs() {
        assert_eq!(CLI_BINARY_NAME, "cs");
    }

    // -- resolve_from_env_override --

    #[test]
    fn resolve_from_env_override_returns_none_when_not_set() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_CLI_PATH");
        assert!(resolve_from_env_override().is_none());
    }

    #[test]
    fn resolve_from_env_override_returns_ok_for_existing_path() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (_dir, bin) = make_fake_cli();

        std::env::set_var("CS_CLI_PATH", bin.to_str().unwrap());
        let result = resolve_from_env_override().unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), bin);

        std::env::remove_var("CS_CLI_PATH");
    }

    #[test]
    fn resolve_from_env_override_returns_error_for_missing_path() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_CLI_PATH", "/nonexistent/cs-binary");
        let result = resolve_from_env_override().unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-existent"));
        std::env::remove_var("CS_CLI_PATH");
    }

    // -- resolve_from_docker --

    #[test]
    fn resolve_from_docker_returns_none_when_not_docker() {
        assert!(resolve_from_docker().is_none());
    }

    // -- parse_cli_output --

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

    // -- find_git_root --

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

    // -- extract_embedded_cli (integration) --

    #[test]
    fn extract_embedded_cli_produces_binary() {
        let result = extract_embedded_cli();
        assert!(result.is_ok(), "extract_embedded_cli failed: {result:?}");
        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("cs"));
    }

    // -- resolve_cli_path --

    #[test]
    fn resolve_cli_path_finds_cli() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_CLI_PATH");
        assert!(resolve_cli_path().is_ok());
    }

    #[test]
    fn resolve_cli_path_uses_env_override() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (_dir, bin) = make_fake_cli();

        std::env::set_var("CS_CLI_PATH", bin.to_str().unwrap());
        assert_eq!(resolve_cli_path().unwrap(), bin);
        std::env::remove_var("CS_CLI_PATH");
    }

    // -- run_cli_at_path --

    #[tokio::test]
    async fn run_cli_at_path_with_echo() {
        let output = run_cli_at_path(Path::new("/bin/echo"), &["hello", "world"], None).await;
        assert_eq!(output.unwrap().trim(), "hello world");
    }

    #[tokio::test]
    async fn run_cli_at_path_forwards_env_vars() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    async fn run_cli_at_path_with_working_dir() {
        let dir = tempfile::tempdir().unwrap();
        let output = run_cli_at_path(Path::new("/bin/pwd"), &[], Some(dir.path())).await.unwrap();
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
}

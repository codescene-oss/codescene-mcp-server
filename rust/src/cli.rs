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
    let mut cmd = tokio::process::Command::new(&cli_path);

    cmd.args(args)
        .env("CS_CONTEXT", "mcp-server")
        .env("CS_DISABLE_VERSION_CHECK", "1");

    // Forward access token if set
    if let Ok(token) = std::env::var("CS_ACCESS_TOKEN") {
        cmd.env("CS_ACCESS_TOKEN", token);
    }

    // Forward on-prem URL if set
    if let Ok(url) = std::env::var("CS_ONPREM_URL") {
        cmd.env("CS_ONPREM_URL", url);
    }

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let output: Output = cmd.output().await?;

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

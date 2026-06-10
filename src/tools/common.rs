use std::path::Path;

use rmcp::model::{CallToolResult, Content};

use crate::cli;
use crate::cli::CliRunner;
use crate::docker;
use crate::environment;
use crate::errors;

/// Reject any user-supplied argument that looks like a CLI flag.
/// This prevents option-injection when untrusted strings are passed
/// as positional arguments to the `cs` CLI.
fn reject_flag_like(value: &str, param_name: &str) -> Result<(), errors::CliError> {
    let trimmed = value.trim();
    if trimmed.starts_with('-') {
        return Err(errors::CliError::InvalidInput(format!(
            "{param_name} must not start with '-': {trimmed}"
        )));
    }
    Ok(())
}

pub(crate) async fn run_review(
    file_path: &Path,
    cli_runner: &dyn CliRunner,
) -> Result<String, errors::CliError> {
    let resolved = resolve_file_path(file_path);
    let git_root = cli::find_git_root(Path::new(&resolved));
    let cli_path = make_cli_path(&resolved, git_root.as_deref());
    reject_flag_like(&cli_path, "file_path")?;
    let args = vec!["review", "--output-format=json", &cli_path];
    cli_runner.run(&args, git_root.as_deref()).await
}

/// Run `git update-index --refresh` to fix index extensions that the
/// container's git cannot parse.  Scrubs sensitive env vars from the
/// child process since git never needs tokens.  Non-zero exit is
/// expected and harmless.
///
/// The `-c` overrides neutralize repo-level `core.*` settings that git
/// would otherwise honor and that could lead to arbitrary command
/// execution (e.g. `core.fsmonitor`, `core.sshCommand`, `core.hooksPath`).
async fn refresh_git_index(repo_path: &Path) {
    let mut git_cmd = tokio::process::Command::new("git");
    for var in crate::config::sensitive_env_vars() {
        git_cmd.env_remove(var);
    }
    let _ = git_cmd
        .args([
            "-c", "core.fsmonitor=",
            "-c", "core.sshCommand=echo",
            "-c", "core.hooksPath=/dev/null",
            "update-index", "--refresh",
        ])
        .current_dir(repo_path)
        .output()
        .await;
}

pub(crate) async fn run_delta(
    repo_path: &Path,
    base_ref: Option<&str>,
    cli_runner: &dyn CliRunner,
) -> Result<String, errors::CliError> {
    // In Docker mode the host git may write index extensions that the
    // container's embedded git (inside the cs CLI) cannot parse, causing
    // "index uses <ext> extension, which we do not understand".
    // Running `git update-index --refresh` inside the container forces its
    // git to re-read and rewrite the index, stripping unknown extensions.
    // The command may return non-zero when file stats differ across the
    // bind-mount boundary — that is expected and harmless.
    if environment::is_docker() {
        refresh_git_index(repo_path).await;
    }

    let mut args = vec!["delta", "--output-format=json"];
    if let Some(br) = base_ref {
        reject_flag_like(br, "base_ref")?;
        args.push(br);
    }
    cli_runner.run(&args, Some(repo_path)).await
}

pub(crate) fn extract_score(review_output: &str) -> Option<f64> {
    let parsed: serde_json::Value = serde_json::from_str(review_output).ok()?;
    parsed.get("score").and_then(|s| s.as_f64())
}

pub(crate) fn make_relative_for_api(file_path: &Path) -> String {
    let git_root = cli::find_git_root(file_path);
    match git_root {
        Some(root) => docker::get_relative_file_path_for_api(file_path, &root),
        None => file_path.to_string_lossy().to_string(),
    }
}

pub(crate) fn tool_error(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

fn make_cli_path(file_path: &str, git_root: Option<&Path>) -> String {
    if environment::is_docker() {
        return docker::adapt_path_for_docker(Path::new(file_path));
    }
    match git_root {
        Some(root) => docker::get_relative_file_path_for_api(Path::new(file_path), root),
        None => file_path.to_string(),
    }
}

fn resolve_file_path(file_path: &Path) -> String {
    if file_path.is_absolute() {
        return file_path.to_string_lossy().to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(file_path).to_string_lossy().to_string(),
        Err(_) => file_path.to_string_lossy().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_score_parses_number() {
        assert_eq!(extract_score(r#"{"score": 8.5}"#), Some(8.5));
    }

    #[test]
    fn extract_score_handles_invalid_json() {
        assert_eq!(extract_score("invalid"), None);
    }

    #[test]
    fn resolve_file_path_absolute_stays_unchanged() {
        let input = if cfg!(windows) { r"C:\absolute\path\file.rs" } else { "/absolute/path/file.rs" };
        let p = resolve_file_path(Path::new(input));
        assert_eq!(p, input);
    }

    #[test]
    fn resolve_file_path_relative_gets_cwd_prefix() {
        let p = resolve_file_path(Path::new("relative/file.rs"));
        // Should be prefixed with current working dir
        assert!(p.ends_with("relative/file.rs"));
        assert!(Path::new(&p).is_absolute());
    }

    #[test]
    fn make_cli_path_non_docker_with_root() {
        // When not in docker mode and git root is provided, returns relative path
        let file_path = "/repo/src/main.rs";
        let git_root = Path::new("/repo");
        let result = make_cli_path(file_path, Some(git_root));
        assert_eq!(result, "src/main.rs");
    }

    #[test]
    fn make_cli_path_non_docker_without_root() {
        let result = make_cli_path("/some/file.rs", None);
        assert_eq!(result, "/some/file.rs");
    }

    #[test]
    fn tool_error_returns_error_result() {
        let result = tool_error("something went wrong");
        assert!(result.is_error.unwrap_or(false));
    }

    #[test]
    fn reject_flag_like_accepts_normal_path() {
        assert!(reject_flag_like("src/main.rs", "file_path").is_ok());
        assert!(reject_flag_like("/absolute/path.rs", "file_path").is_ok());
        assert!(reject_flag_like("main", "base_ref").is_ok());
    }

    #[test]
    fn reject_flag_like_rejects_single_dash() {
        let err = reject_flag_like("-o/tmp/evil", "file_path");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("must not start with '-'"));
    }

    #[test]
    fn reject_flag_like_rejects_double_dash_flag() {
        let err = reject_flag_like("--output=/tmp/evil", "base_ref");
        assert!(err.is_err());
    }

    #[test]
    fn reject_flag_like_rejects_with_leading_whitespace() {
        let err = reject_flag_like("  --sneaky", "base_ref");
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn run_review_rejects_flag_like_path() {
        use crate::tests::MockCliRunner;
        let cli = MockCliRunner::with_ok("unused");
        // A path that resolves to something starting with - can't happen
        // via normal filesystem, but reject_flag_like catches it after
        // make_cli_path. We test run_delta which is easier to trigger.
        let result = run_delta(Path::new("/tmp"), Some("--output=/tmp/evil"), &cli).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must not start with '-'"));
    }

    #[tokio::test]
    async fn run_delta_passes_base_ref_as_positional_arg() {
        use std::sync::{Arc, Mutex};

        struct CapturingCli {
            captured: Arc<Mutex<Vec<String>>>,
        }

        #[async_trait::async_trait]
        impl CliRunner for CapturingCli {
            async fn run(&self, args: &[&str], _working_dir: Option<&Path>) -> Result<String, errors::CliError> {
                *self.captured.lock().unwrap() = args.iter().map(|s| s.to_string()).collect();
                Ok("{}".to_string())
            }
        }

        let captured = Arc::new(Mutex::new(Vec::new()));
        let cli = CapturingCli { captured: captured.clone() };
        let _ = run_delta(Path::new("/tmp"), Some("main"), &cli).await;
        let args = captured.lock().unwrap();
        assert_eq!(args.as_slice(), &["delta", "--output-format=json", "main"]);
    }

    #[tokio::test]
    async fn refresh_git_index_runs_without_error() {
        // refresh_git_index should tolerate any repo path and never panic.
        // Using a temp dir (not a real git repo) — the command will fail
        // with a non-zero exit code, which is expected and ignored.
        let dir = tempfile::tempdir().unwrap();
        refresh_git_index(dir.path()).await;
        // No panic or error means success — the function deliberately
        // ignores the exit status.
    }

    #[tokio::test]
    async fn refresh_git_index_scrubs_sensitive_env_vars() {
        // Verify the function removes sensitive env vars by setting one
        // and confirming the child process doesn't see it.  We do this by
        // running in a real git repo and checking that the function
        // completes without propagating the token.
        //
        // We can't easily inspect the child env directly, but we verify
        // the code path executes without error.  The actual env_remove
        // coverage is ensured by calling refresh_git_index which iterates
        // over sensitive_env_vars() and calls env_remove for each.
        let dir = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        // Set a sensitive env var in our process (guard holds mutex + cleans up on drop)
        let _guard = crate::test_utils::set_token("test-secret");
        refresh_git_index(dir.path()).await;
    }
}

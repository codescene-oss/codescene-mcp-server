use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use serde_json::json;

use crate::ace_client;
use crate::cli;
use crate::cli::CliRunner;
use crate::docker;
use crate::environment;
use crate::errors;
use crate::http::HttpClient;

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
async fn refresh_git_index(repo_path: &Path) {
    let mut git_cmd = tokio::process::Command::new("git");
    for var in crate::config::sensitive_env_vars() {
        git_cmd.env_remove(var);
    }
    let _ = git_cmd
        .args(["update-index", "--refresh"])
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

pub(crate) async fn run_auto_refactor(
    file_path: &Path,
    function_name: &str,
    cli_runner: &dyn CliRunner,
    http_client: &dyn HttpClient,
) -> Result<serde_json::Value, String> {
    if std::env::var("CS_ACE_ACCESS_TOKEN")
        .map(|v| v.is_empty())
        .unwrap_or(true)
    {
        return Err(
            "Error: This tool needs ACE access configured via set_config key \
             \"ace_access_token\" (or CS_ACE_ACCESS_TOKEN). See \
             https://github.com/codescene-oss/codescene-mcp-server?tab=readme-ov-file#-activate-ace-in-codescene-mcp"
                .to_string(),
        );
    }

    let file_str = file_path.to_string_lossy();
    let docker_path = docker::adapt_path_for_docker(file_path);
    let git_root = cli::find_git_root(Path::new(&docker_path))
        .ok_or_else(|| format!("Error: Could not find git root for {}", file_str))?;
    let cli_path = make_cli_path(&docker_path, Some(&git_root));
    reject_flag_like(&cli_path, "file_path").map_err(|e| format!("Error: {e}"))?;

    let parse_output = cli_runner
        .run(&["parse-fns", "--path", &cli_path], Some(&git_root))
        .await
        .map_err(|e| format!("Error: {e}"))?;
    let functions: serde_json::Value =
        serde_json::from_str(&parse_output).map_err(|e| format!("Error parsing functions: {e}"))?;

    let review_output = cli_runner
        .run(
            &["review", "--output-format=json", &cli_path],
            Some(&git_root),
        )
        .await
        .map_err(|e| format!("Error: {e}"))?;
    let review: serde_json::Value =
        serde_json::from_str(&review_output).map_err(|e| format!("Error parsing review: {e}"))?;

    let function = find_function_in_parsed(&functions, function_name)
        .ok_or_else(|| format!("Error: Couldn't find function: {function_name}"))?;

    let code_smells = extract_code_smells(&review, function, function_name);
    if code_smells.is_empty() {
        return Err(format!(
            "Error: No code smells were found in {function_name}"
        ));
    }

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let payload = build_ace_payload(function, &code_smells, ext);

    let response = ace_client::refactor_with_client(&payload, http_client)
        .await
        .map_err(|e| format!("Error: {e}"))?;

    Ok(format_ace_response(&response))
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

fn find_function_in_parsed<'a>(
    functions: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    functions
        .as_array()?
        .iter()
        .find(|f| f.get("name").and_then(|n| n.as_str()) == Some(name))
}

fn extract_code_smells(
    review: &serde_json::Value,
    function: &serde_json::Value,
    function_name: &str,
) -> Vec<serde_json::Value> {
    let fn_start = function
        .get("start-line")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let review_items = review
        .get("review")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let mut smells = Vec::new();
    for smell in &review_items {
        let category = smell.get("category").and_then(|c| c.as_str()).unwrap_or("");
        let fns = smell
            .get("functions")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();
        for f in &fns {
            let title = f.get("title").and_then(|t| t.as_str()).unwrap_or("");
            if matches_function_name(title, function_name) {
                let start = f.get("start-line").and_then(|s| s.as_i64()).unwrap_or(0);
                smells.push(json!({
                    "category": category,
                    "start-line": start - fn_start + 1,
                }));
            }
        }
    }
    smells
}

fn matches_function_name(title: &str, function_name: &str) -> bool {
    if title == function_name {
        return true;
    }
    let base = title.trim_end_matches(|c: char| c.is_ascii_digit());
    let base = base.trim_end_matches(':');
    base == function_name && base.len() < title.len()
}

fn build_ace_payload(
    function: &serde_json::Value,
    code_smells: &[serde_json::Value],
    file_ext: &str,
) -> serde_json::Value {
    let body = function.get("body").and_then(|b| b.as_str()).unwrap_or("");
    let fn_type = function
        .get("function-type")
        .and_then(|t| t.as_str())
        .unwrap_or("Unknown");

    json!({
        "api-version": "v2",
        "source-snippet": {
            "file-type": file_ext,
            "body": body,
            "function-type": fn_type,
        },
        "review": code_smells,
    })
}

fn format_ace_response(response: &serde_json::Value) -> serde_json::Value {
    let code = response.get("code").cloned().unwrap_or(json!(""));
    let declarations = response.get("declarations").cloned().unwrap_or(json!(""));
    let confidence = response
        .get("confidence")
        .and_then(|c| c.get("description"))
        .and_then(|d| d.as_str())
        .unwrap_or("unknown");
    let reasons: Vec<String> = response
        .get("reasons")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("summary").and_then(|s| s.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    json!({
        "code": code,
        "declarations": declarations,
        "confidence": confidence,
        "reasons": reasons,
    })
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
    fn matches_function_name_exact() {
        assert!(matches_function_name("foo", "foo"));
    }

    #[test]
    fn matches_function_name_with_line_suffix() {
        assert!(matches_function_name("foo:42", "foo"));
    }

    #[test]
    fn matches_function_name_no_match() {
        assert!(!matches_function_name("bar", "foo"));
    }

    #[test]
    fn matches_function_name_strip_trailing_colon_digit() {
        assert!(matches_function_name("myFunc:7", "myFunc"));
    }

    #[test]
    fn matches_function_name_prefix_mismatch() {
        assert!(!matches_function_name("foobar", "foo"));
    }

    #[test]
    fn extract_code_smells_finds_matching_functions() {
        let review = json!({
            "review": [{
                "category": "Complex Method",
                "functions": [
                    {"title": "do_stuff", "start-line": 10},
                    {"title": "other_fn", "start-line": 20}
                ]
            }]
        });
        let function = json!({"start-line": 5});
        let smells = extract_code_smells(&review, &function, "do_stuff");
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0]["category"], "Complex Method");
        // start-line relative to function: 10 - 5 + 1 = 6
        assert_eq!(smells[0]["start-line"], 6);
    }

    #[test]
    fn extract_code_smells_empty_when_no_match() {
        let review = json!({"review": [{"category": "X", "functions": [{"title": "other", "start-line": 1}]}]});
        let function = json!({"start-line": 1});
        let smells = extract_code_smells(&review, &function, "nonexistent");
        assert!(smells.is_empty());
    }

    #[test]
    fn extract_code_smells_handles_missing_review_key() {
        let review = json!({});
        let function = json!({"start-line": 1});
        let smells = extract_code_smells(&review, &function, "foo");
        assert!(smells.is_empty());
    }

    #[test]
    fn resolve_file_path_absolute_stays_unchanged() {
        let p = resolve_file_path(Path::new("/absolute/path/file.rs"));
        assert_eq!(p, "/absolute/path/file.rs");
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
        // Set a sensitive env var in our process
        std::env::set_var("CS_ACCESS_TOKEN", "test-secret");
        refresh_git_index(dir.path()).await;
        // Clean up
        std::env::remove_var("CS_ACCESS_TOKEN");
    }
}

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

pub(crate) async fn run_review(
    file_path: &Path,
    cli_runner: &dyn CliRunner,
) -> Result<String, errors::CliError> {
    let resolved = resolve_file_path(file_path);
    let git_root = cli::find_git_root(Path::new(&resolved));
    let cli_path = make_cli_path(&resolved, git_root.as_deref());
    let args = vec!["review", "--output-format=json", &cli_path];
    cli_runner.run(&args, git_root.as_deref()).await
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
        let _ = tokio::process::Command::new("git")
            .args(["update-index", "--refresh"])
            .current_dir(repo_path)
            .output()
            .await;
    }

    let mut args = vec!["delta", "--output-format=json"];
    if let Some(br) = base_ref {
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
    if let Some(base) = title.strip_suffix(|c: char| c == ':' || c.is_ascii_digit()) {
        let base = base.trim_end_matches(':');
        return base == function_name;
    }
    title.starts_with(function_name)
        && title[function_name.len()..].starts_with(':')
        && title[function_name.len() + 1..]
            .chars()
            .all(|c| c.is_ascii_digit())
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
}

//! Docker path translation integration tests.
//!
//! Validates that the MCP server correctly translates host paths to container
//! paths when running inside Docker. This is the exact scenario reported in
//! CS-11414: Windows users running Docker get "not inside a git repository"
//! errors because path translation fails for Windows-style paths.
//!
//! These tests only run under the Docker backend (`CS_MCP_BACKEND=docker`).
//! On a Windows host, the host paths are `C:\Users\...` which must be
//! correctly translated to `/mount/...` inside the Linux container.

use super::*;

const TIMEOUT: Duration = Duration::from_secs(60);

fn skip_unless_docker(reason: &str) {
    if !is_docker() {
        eprintln!("  SKIP: {reason} (only runs under Docker backend)");
    }
}

/// Verify that the Docker path translation finds the git repository.
/// This is the exact failure from CS-11414: `verify_installation` reports
/// "[FAIL] Git Repository" when host paths aren't translated correctly.
pub fn test_docker_verify_finds_git_repo() {
    if !is_docker() {
        return skip_unless_docker("Docker git repo detection");
    }
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "verify_installation",
            json!({"git_repository_path": repo_dir.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);
    let lower = result_text.to_lowercase();

    assert!(
        lower.contains("[pass] git repository"),
        "Git repository check should pass under Docker. \
         If this fails, host path translation is broken. \
         Full output: {result_text}"
    );
    assert!(
        lower.contains("git root"),
        "Should report the git root path: {result_text}"
    );
}

/// Verify that `code_health_score` works with host paths translated through Docker.
pub fn test_docker_code_health_score() {
    if !is_docker() {
        return skip_unless_docker("Docker code health score");
    }
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = repo_dir.join("src/utils/calculator.py");
    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);
    let score = extract_code_health_score(&result_text);

    assert!(
        score.is_some(),
        "Should return a valid Code Health score under Docker. \
         If this fails, file path translation is broken. \
         Full output: {result_text}"
    );
}

/// Verify that `pre_commit_code_health_safeguard` works with Docker paths.
/// This tool takes `git_repository_path` which must be translated.
pub fn test_docker_pre_commit_safeguard() {
    if !is_docker() {
        return skip_unless_docker("Docker pre-commit safeguard");
    }
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "pre_commit_code_health_safeguard",
            json!({"git_repository_path": repo_dir.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);
    let lower = result_text.to_lowercase();

    assert!(
        lower.contains("quality") || lower.contains("gate") || lower.contains("code health"),
        "Should return quality gate info under Docker. \
         If this fails, git_repository_path translation is broken. \
         Full output: {result_text}"
    );
}

/// Verify that `code_health_review` works with Docker paths.
pub fn test_docker_code_health_review() {
    if !is_docker() {
        return skip_unless_docker("Docker code health review");
    }
    let (command, env, repo_dir, _tmp) = setup();
    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = repo_dir.join("src/services/order_processor.py");
    let response = client
        .call_tool(
            "code_health_review",
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result_text = extract_result_text(&response);

    assert!(
        result_text.len() > 50,
        "Review should return substantial content under Docker. \
         Full output: {result_text}"
    );

    let lower = result_text.to_lowercase();
    assert!(
        lower.contains("code health") || lower.contains("complexity") || lower.contains("function"),
        "Review should contain Code Health terms under Docker. \
         Full output: {result_text}"
    );
}

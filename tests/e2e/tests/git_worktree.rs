//! Git worktree integration tests.
//!
//! Tests that the MCP server correctly handles git worktrees,
//! which have special path resolution requirements.

use super::*;
use std::process::Command;

const TIMEOUT: Duration = Duration::from_secs(60);

fn create_worktree(repo_dir: &Path, branch_name: &str) -> std::path::PathBuf {
    let worktree_dir = repo_dir
        .parent()
        .unwrap()
        .join(format!("worktree_{branch_name}"));
    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            branch_name,
            &worktree_dir.to_string_lossy(),
            "master",
        ])
        .current_dir(repo_dir)
        .output()
        .expect("git worktree add should execute");

    assert!(
        output.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    worktree_dir
}

fn cleanup_worktree(repo_dir: &Path, worktree_dir: &Path) {
    let _ = Command::new("git")
        .args([
            "worktree",
            "remove",
            &worktree_dir.to_string_lossy(),
            "--force",
        ])
        .current_dir(repo_dir)
        .output();
}

fn worktree_setup() -> (
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_worktree_").expect("temp dir");
    let repo_dir = create_git_repo(temp_dir.path(), &get_sample_files()).expect("git repo");

    let worktree_dir = create_worktree(&repo_dir, "test-feature");

    let base = base_env();
    let env = backend.get_env(&base, &worktree_dir);
    let env_vec: Vec<(String, String)> = env.into_iter().collect();
    let command = backend.get_command(&worktree_dir);

    (command, env_vec, repo_dir, worktree_dir, temp_dir)
}

fn run_worktree_score_test(
    worktree_dir: &Path,
    file_subpath: &str,
    command: &[String],
    env: &[(String, String)],
) -> String {
    let mut client = make_client(command, env, worktree_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = worktree_dir.join(file_subpath);
    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    let score = extract_code_health_score(&result);
    assert!(
        score.is_some(),
        "Should return a score in worktree: {result}"
    );
    result
}

pub fn test_worktree_code_health_score() {
    let (command, env, repo_dir, worktree_dir, _tmp) = worktree_setup();
    let result = run_worktree_score_test(&worktree_dir, "src/utils/calculator.py", &command, &env);

    let lower = result.to_lowercase();
    assert!(!lower.contains("nonetype"), "No worktree-related errors");
    assert!(!lower.contains("traceback"), "No traceback errors");

    cleanup_worktree(&repo_dir, &worktree_dir);
}

pub fn test_worktree_code_health_review() {
    let (command, env, repo_dir, worktree_dir, _tmp) = worktree_setup();
    let mut client = make_client(&command, &env, &worktree_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = worktree_dir.join("src/services/order_processor.py");
    let response = client
        .call_tool(
            "code_health_review",
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    assert!(
        result.len() > 50,
        "Review should return substantial content"
    );
    assert!(
        !result.to_lowercase().contains("traceback"),
        "No errors in response"
    );

    cleanup_worktree(&repo_dir, &worktree_dir);
}

pub fn test_worktree_pre_commit() {
    let (command, env, repo_dir, worktree_dir, _tmp) = worktree_setup();
    let mut client = make_client(&command, &env, &worktree_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let test_file = worktree_dir.join("src/utils/calculator.py");
    let original = std::fs::read_to_string(&test_file).expect("read file");
    std::fs::write(&test_file, format!("{original}\n# Worktree modification\n")).expect("write");

    Command::new("git")
        .args(["add", &test_file.to_string_lossy()])
        .current_dir(&worktree_dir)
        .output()
        .expect("git add");

    let response = client
        .call_tool(
            "pre_commit_code_health_safeguard",
            json!({"git_repository_path": worktree_dir.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    assert!(result.len() > 20, "Safeguard should return content");
    let lower = result.to_lowercase();
    assert!(!lower.contains("traceback"), "No errors");
    assert!(!lower.contains("nonetype"), "No NoneType errors");

    // Reset
    std::fs::write(&test_file, &original).expect("restore");
    let _ = Command::new("git")
        .args(["reset", "HEAD", &test_file.to_string_lossy()])
        .current_dir(&worktree_dir)
        .output();

    cleanup_worktree(&repo_dir, &worktree_dir);
}

pub fn test_worktree_absolute_paths() {
    let (command, env, repo_dir, worktree_dir, _tmp) = worktree_setup();
    run_worktree_score_test(&worktree_dir, "src/utils/calculator.py", &command, &env);
    cleanup_worktree(&repo_dir, &worktree_dir);
}

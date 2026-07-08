//! Git subtree integration tests.
//!
//! Tests that the MCP server correctly handles git subtrees,
//! where external repositories are nested as subdirectories.

use super::*;
use std::process::Command;

const TIMEOUT: Duration = Duration::from_secs(60);
const SUBTREE_PREFIX: &str = "lib/external";

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git command should execute");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_external_repo(base_dir: &Path) -> std::path::PathBuf {
    let dir = base_dir.join("external_lib");
    std::fs::create_dir_all(&dir).expect("create external dir");

    git(&dir, &["init", "-b", "master"]);
    git(&dir, &["config", "user.name", "Test User"]);
    git(&dir, &["config", "user.email", "test@example.com"]);

    let utils_content = r#""""Shared utility functions."""

def helper_function(value: int) -> int:
    """A simple helper function."""
    return value * 2


def validate_input(data: dict) -> bool:
    """Validate input data."""
    required_keys = ["id", "name"]
    return all(key in data for key in required_keys)
"#;

    let config_content = r#""""Configuration module."""

DEFAULT_TIMEOUT = 30
MAX_RETRIES = 3


class Config:
    """Configuration settings."""

    def __init__(self):
        self.timeout = DEFAULT_TIMEOUT
        self.retries = MAX_RETRIES

    def update(self, **kwargs):
        """Update configuration."""
        for key, value in kwargs.items():
            if hasattr(self, key):
                setattr(self, key, value)
"#;

    std::fs::write(dir.join("utils.py"), utils_content).expect("write utils");
    std::fs::write(dir.join("config.py"), config_content).expect("write config");

    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "Initial commit"]);

    dir
}

fn subtree_setup() -> Option<(
    Vec<String>,
    Vec<(String, String)>,
    std::path::PathBuf,
    tempfile::TempDir,
)> {
    // Check git subtree availability
    let check = Command::new("git").args(["subtree", "--help"]).output();
    if check.is_err() || !check.unwrap().status.success() {
        eprintln!("  SKIP: git subtree not available");
        return None;
    }

    let executable = find_or_build_executable();
    let backend = create_backend(executable);

    let temp_dir = create_temp_dir("cs_mcp_subtree_").expect("temp dir");

    let external_repo = create_external_repo(temp_dir.path());
    let main_dir = temp_dir.path().join("main_project");
    let repo_dir = create_git_repo(&main_dir, &get_sample_files()).expect("git repo");

    // Add subtree
    let output = Command::new("git")
        .args([
            "subtree",
            "add",
            "--prefix",
            SUBTREE_PREFIX,
            &external_repo.to_string_lossy(),
            "master",
            "--squash",
        ])
        .current_dir(&repo_dir)
        .output()
        .expect("git subtree add");
    assert!(
        output.status.success(),
        "git subtree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let base = base_env();
    let env = backend.get_env(&base, &repo_dir);
    let env_vec: Vec<(String, String)> = env.into_iter().collect();
    let command = backend.get_command(&repo_dir);

    Some((command, env_vec, repo_dir, temp_dir))
}

fn run_score_test(
    file_path: &Path,
    repo_dir: &Path,
    command: &[String],
    env: &[(String, String)],
) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_score",
            json!({"file_path": file_path.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    let score = extract_code_health_score(&result);
    assert!(score.is_some(), "Should get score: {result}");
    result
}

pub fn test_subtree_code_health_score() {
    let Some((command, env, repo_dir, _tmp)) = subtree_setup() else {
        return;
    };
    let test_file = repo_dir.join(SUBTREE_PREFIX).join("utils.py");
    assert!(test_file.exists(), "Subtree file should exist");
    run_score_test(&test_file, &repo_dir, &command, &env);
}

pub fn test_subtree_code_health_review() {
    let Some((command, env, repo_dir, _tmp)) = subtree_setup() else {
        return;
    };
    let test_file = repo_dir.join(SUBTREE_PREFIX).join("config.py");

    let mut client = make_client(&command, &env, &repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");

    let response = client
        .call_tool(
            "code_health_review",
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");

    let result = extract_result_text(&response);
    assert!(!result.is_empty(), "Review should return content");
    assert!(
        !result.to_lowercase().contains("traceback"),
        "No errors in response"
    );
}

pub fn test_subtree_pre_commit() {
    let Some((command, env, repo_dir, _tmp)) = subtree_setup() else {
        return;
    };
    let test_file = repo_dir.join(SUBTREE_PREFIX).join("utils.py");
    let original = std::fs::read_to_string(&test_file).expect("read");
    std::fs::write(
        &test_file,
        format!("{original}\n# Subtree modification test\n"),
    )
    .expect("write");

    Command::new("git")
        .args(["add", &test_file.to_string_lossy()])
        .current_dir(&repo_dir)
        .output()
        .expect("git add");

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

    let result = extract_result_text(&response);
    assert!(result.len() > 20, "Safeguard should return content");
    assert!(!result.to_lowercase().contains("traceback"), "No errors");

    // Reset
    std::fs::write(&test_file, &original).expect("restore");
    let _ = Command::new("git")
        .args(["reset", "HEAD", &test_file.to_string_lossy()])
        .current_dir(&repo_dir)
        .output();
}

pub fn test_subtree_absolute_paths() {
    let Some((command, env, repo_dir, _tmp)) = subtree_setup() else {
        return;
    };
    let test_file = repo_dir.join(SUBTREE_PREFIX).join("utils.py");
    run_score_test(&test_file, &repo_dir, &command, &env);
}

pub fn test_main_repo_still_works() {
    let Some((command, env, repo_dir, _tmp)) = subtree_setup() else {
        return;
    };
    let test_file = repo_dir.join("src/utils/calculator.py");
    run_score_test(&test_file, &repo_dir, &command, &env);
}

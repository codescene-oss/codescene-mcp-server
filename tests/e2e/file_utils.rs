//! File and git repository utilities for e2e tests.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Create a temporary git repository with sample files.
///
/// Returns the path to the repo directory within the temp dir.
pub fn create_git_repo(
    base_dir: &Path,
    sample_files: &HashMap<&str, &str>,
) -> Result<PathBuf, String> {
    let repo_dir = base_dir.join("test_repo");
    fs::create_dir_all(&repo_dir).map_err(|e| format!("Failed to create repo dir: {e}"))?;

    // Initialize git repo
    run_git(&repo_dir, &["init", "-b", "master"])?;
    run_git(&repo_dir, &["config", "user.name", "Test User"])?;
    run_git(&repo_dir, &["config", "user.email", "test@example.com"])?;
    run_git(&repo_dir, &["config", "index.version", "2"])?;

    // Create sample files
    for (file_path, content) in sample_files {
        let full_path = repo_dir.join(file_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir {}: {e}", parent.display()))?;
        }
        fs::write(&full_path, content)
            .map_err(|e| format!("Failed to write {}: {e}", full_path.display()))?;
    }

    // Initial commit
    run_git(&repo_dir, &["add", "."])?;
    run_git(&repo_dir, &["commit", "-m", "Initial commit"])?;

    Ok(repo_dir)
}

/// Run a git command in the given directory.
fn run_git(cwd: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to run git {}: {e}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {stderr}", args.join(" ")));
    }
    Ok(())
}

/// Create a temporary directory that is automatically cleaned up on drop.
pub fn create_temp_dir(prefix: &str) -> Result<TempDir, String> {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .map_err(|e| format!("Failed to create temp dir: {e}"))
}

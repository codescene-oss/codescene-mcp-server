/// Docker path adaptation — mirrors Python's `docker_path_adapter.py`.
///
/// Translates between host paths (via `CS_MOUNT_PATH`) and container
/// mount paths (`/mount/`). Handles Windows drive letters, worktrees,
/// and relative paths.

use std::path::{Path, PathBuf};

use crate::environment;

/// Container-side mount point.
const CONTAINER_MOUNT: &str = "/mount";

/// A host path normalized to forward slashes with any Windows drive
/// letter stripped — ready for case-insensitive prefix comparison.
struct NormalizedPath(String);

impl NormalizedPath {
    fn new(raw: &str) -> Self {
        let forward = raw.replace('\\', "/");
        let bytes = forward.as_bytes();
        let has_drive = forward.len() >= 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':';
        if has_drive {
            Self(forward[2..].to_string())
        } else {
            Self(forward)
        }
    }

    /// Strip `prefix` (case-insensitively) and return the relative tail.
    fn strip_prefix(&self, prefix: &NormalizedPath) -> Option<String> {
        let prefix_trimmed = prefix.0.to_lowercase();
        let prefix_trimmed = prefix_trimmed.trim_end_matches('/');

        if self.0.to_lowercase().starts_with(prefix_trimmed) {
            let rest = &self.0[prefix_trimmed.len()..];
            let rest = rest.strip_prefix('/').unwrap_or(rest);
            Some(rest.to_string())
        } else {
            None
        }
    }
}

/// Return the `CS_MOUNT_PATH` value if running in Docker, or `None`.
fn docker_mount_path() -> Option<String> {
    if !environment::is_docker() {
        return None;
    }
    std::env::var("CS_MOUNT_PATH").ok()
}

/// Adapt an incoming file path for use inside a Docker container.
///
/// If running in Docker, translates host paths (rooted at `CS_MOUNT_PATH`)
/// to container paths under `/mount/`. Otherwise returns the path unchanged.
pub fn adapt_path_for_docker(path: &str) -> String {
    let mount_raw = match docker_mount_path() {
        Some(m) => m,
        None => return path.to_string(),
    };

    let mount = NormalizedPath::new(&mount_raw);
    let normalized = NormalizedPath::new(path);

    if let Some(relative) = normalized.strip_prefix(&mount) {
        format!("{CONTAINER_MOUNT}/{relative}")
    } else {
        path.to_string()
    }
}

/// Convert a container path back to a host path for display.
#[allow(dead_code)]
pub fn adapt_path_from_docker(path: &str) -> String {
    let mount_path = match docker_mount_path() {
        Some(m) => m,
        None => return path.to_string(),
    };

    if let Some(relative) = path.strip_prefix(CONTAINER_MOUNT) {
        let relative = relative.strip_prefix('/').unwrap_or(relative);
        format!("{mount_path}/{relative}")
    } else {
        path.to_string()
    }
}

/// Get a relative file path suitable for API calls.
///
/// Strips the repository root to produce a repo-relative path.
pub fn get_relative_file_path_for_api(file_path: &str, repo_root: &str) -> String {
    let file = NormalizedPath::new(file_path);
    let root = NormalizedPath::new(repo_root);

    file.strip_prefix(&root)
        .unwrap_or_else(|| file_path.to_string())
}

/// Detect and return the git worktree `.git` directory if applicable.
#[allow(dead_code)]
pub fn get_worktree_gitdir(git_root: &Path) -> Option<PathBuf> {
    let git_path = git_root.join(".git");

    if git_path.is_file() {
        // Worktree: .git is a file containing "gitdir: <path>"
        let content = std::fs::read_to_string(&git_path).ok()?;
        let gitdir = content.strip_prefix("gitdir: ")?.trim();
        Some(PathBuf::from(gitdir))
    } else {
        None
    }
}

/// Adapt a worktree gitdir path for Docker.
#[allow(dead_code)]
pub fn adapt_worktree_gitdir_for_docker(gitdir: &Path) -> PathBuf {
    let path_str = gitdir.to_string_lossy().to_string();
    PathBuf::from(adapt_path_for_docker(&path_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- NormalizedPath::new ----

    #[test]
    fn strip_windows_drive_letter() {
        assert_eq!(NormalizedPath::new("C:/Users/foo").0, "/Users/foo");
        assert_eq!(NormalizedPath::new("/Users/foo").0, "/Users/foo");
    }

    #[test]
    fn normalizes_backslashes() {
        assert_eq!(NormalizedPath::new(r"C:\Users\foo\bar").0, "/Users/foo/bar");
    }

    #[test]
    fn no_drive_letter_unix_path() {
        assert_eq!(NormalizedPath::new("/home/user/project").0, "/home/user/project");
    }

    #[test]
    fn lowercase_drive_letter() {
        assert_eq!(NormalizedPath::new("d:/code/project").0, "/code/project");
    }

    #[test]
    fn empty_string() {
        assert_eq!(NormalizedPath::new("").0, "");
    }

    #[test]
    fn single_char_no_colon() {
        // "a" is only 1 char, no colon — should not strip
        assert_eq!(NormalizedPath::new("a").0, "a");
    }

    // ---- NormalizedPath::strip_prefix ----

    #[test]
    fn strip_prefix_works() {
        let path = NormalizedPath::new("/Users/foo/bar");
        let prefix = NormalizedPath::new("/Users/foo");
        assert_eq!(path.strip_prefix(&prefix), Some("bar".to_string()));
    }

    #[test]
    fn strip_prefix_case_insensitive() {
        let path = NormalizedPath::new("/USERS/FOO/bar");
        let prefix = NormalizedPath::new("/users/foo");
        assert_eq!(path.strip_prefix(&prefix), Some("bar".to_string()));
    }

    #[test]
    fn strip_prefix_trailing_slash_on_prefix() {
        let path = NormalizedPath::new("/Users/foo/bar");
        let prefix = NormalizedPath::new("/Users/foo/");
        assert_eq!(path.strip_prefix(&prefix), Some("bar".to_string()));
    }

    #[test]
    fn strip_prefix_no_match() {
        let path = NormalizedPath::new("/other/path");
        let prefix = NormalizedPath::new("/Users/foo");
        assert_eq!(path.strip_prefix(&prefix), None);
    }

    #[test]
    fn strip_prefix_exact_match() {
        let path = NormalizedPath::new("/Users/foo");
        let prefix = NormalizedPath::new("/Users/foo");
        assert_eq!(path.strip_prefix(&prefix), Some("".to_string()));
    }

    // ---- get_relative_file_path_for_api ----

    #[test]
    fn relative_path_strips_root() {
        let result = get_relative_file_path_for_api("/repo/src/main.rs", "/repo");
        assert_eq!(result, "src/main.rs");
    }

    #[test]
    fn relative_path_no_match_returns_original() {
        let result = get_relative_file_path_for_api("/other/file.rs", "/repo");
        assert_eq!(result, "/other/file.rs");
    }

    #[test]
    fn relative_path_windows_style() {
        let result = get_relative_file_path_for_api(r"C:\repo\src\main.rs", r"C:\repo");
        assert_eq!(result, "src/main.rs");
    }

    // ---- adapt_path_for_docker (non-Docker environment) ----

    #[test]
    fn adapt_path_for_docker_returns_unchanged_when_not_docker() {
        // In non-Docker env, should return path unchanged
        let result = adapt_path_for_docker("/some/path/file.rs");
        assert_eq!(result, "/some/path/file.rs");
    }

    #[test]
    fn adapt_path_from_docker_returns_unchanged_when_not_docker() {
        let result = adapt_path_from_docker("/mount/src/file.rs");
        assert_eq!(result, "/mount/src/file.rs");
    }

    // ---- get_worktree_gitdir ----

    #[test]
    fn get_worktree_gitdir_returns_none_for_normal_repo() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        // .git is a directory → not a worktree
        assert!(get_worktree_gitdir(dir.path()).is_none());
    }

    #[test]
    fn get_worktree_gitdir_returns_path_for_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let git_file = dir.path().join(".git");
        std::fs::write(&git_file, "gitdir: /some/other/.git/worktrees/branch").unwrap();
        let result = get_worktree_gitdir(dir.path());
        assert_eq!(
            result,
            Some(PathBuf::from("/some/other/.git/worktrees/branch"))
        );
    }

    #[test]
    fn get_worktree_gitdir_returns_none_if_no_git() {
        let dir = tempfile::tempdir().unwrap();
        assert!(get_worktree_gitdir(dir.path()).is_none());
    }

    // ---- adapt_worktree_gitdir_for_docker (non-Docker) ----

    #[test]
    fn adapt_worktree_gitdir_for_docker_returns_unchanged_when_not_docker() {
        let path = PathBuf::from("/some/gitdir");
        let result = adapt_worktree_gitdir_for_docker(&path);
        assert_eq!(result, path);
    }
}

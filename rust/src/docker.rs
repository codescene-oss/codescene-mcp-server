use std::path::{Path, PathBuf};

use crate::environment;

const CONTAINER_MOUNT: &str = "/mount";

/// A host path normalized to forward slashes with any Windows drive
/// letter stripped — ready for case-insensitive prefix comparison.
struct NormalizedPath(String);

impl NormalizedPath {
    fn from_path(path: &Path) -> Self {
        Self::from_str(&path.to_string_lossy())
    }

    fn from_str(raw: &str) -> Self {
        let forward = raw.replace('\\', "/");
        let bytes = forward.as_bytes();
        let has_drive = forward.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
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

pub fn adapt_path_for_docker(path: &Path) -> String {
    translate_to_container(path, docker_mount_path().as_deref().map(Path::new))
}

fn translate_to_container(path: &Path, mount_raw: Option<&Path>) -> String {
    let path_str = path.to_string_lossy();
    let mount_raw = match mount_raw {
        Some(m) => m,
        None => return path_str.to_string(),
    };

    let mount = NormalizedPath::from_path(mount_raw);
    let normalized = NormalizedPath::from_path(path);

    if let Some(relative) = normalized.strip_prefix(&mount) {
        format!("{CONTAINER_MOUNT}/{relative}")
    } else {
        path_str.to_string()
    }
}

#[allow(dead_code)]
pub fn adapt_path_from_docker(path: &Path) -> String {
    translate_from_container(path, docker_mount_path().as_deref().map(Path::new))
}

fn translate_from_container(path: &Path, mount_path: Option<&Path>) -> String {
    let path_str = path.to_string_lossy();
    let mount_path = match mount_path {
        Some(m) => m,
        None => return path_str.to_string(),
    };

    if let Some(relative) = path_str.strip_prefix(CONTAINER_MOUNT) {
        let relative = relative.strip_prefix('/').unwrap_or(relative);
        let mount_str = mount_path.to_string_lossy();
        format!("{mount_str}/{relative}")
    } else {
        path_str.to_string()
    }
}

pub fn get_relative_file_path_for_api(file_path: &Path, repo_root: &Path) -> String {
    let file = NormalizedPath::from_path(file_path);
    let root = NormalizedPath::from_path(repo_root);

    file.strip_prefix(&root)
        .unwrap_or_else(|| file_path.to_string_lossy().to_string())
}

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

#[allow(dead_code)]
pub fn adapt_worktree_gitdir_for_docker(gitdir: &Path) -> PathBuf {
    PathBuf::from(adapt_path_for_docker(gitdir))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- NormalizedPath::from_str / from_path ----

    #[test]
    fn strip_windows_drive_letter() {
        assert_eq!(NormalizedPath::from_str("C:/Users/foo").0, "/Users/foo");
        assert_eq!(NormalizedPath::from_str("/Users/foo").0, "/Users/foo");
    }

    #[test]
    fn normalizes_backslashes() {
        assert_eq!(
            NormalizedPath::from_str(r"C:\Users\foo\bar").0,
            "/Users/foo/bar"
        );
    }

    #[test]
    fn no_drive_letter_unix_path() {
        assert_eq!(
            NormalizedPath::from_path(Path::new("/home/user/project")).0,
            "/home/user/project"
        );
    }

    #[test]
    fn lowercase_drive_letter() {
        assert_eq!(
            NormalizedPath::from_str("d:/code/project").0,
            "/code/project"
        );
    }

    #[test]
    fn empty_string() {
        assert_eq!(NormalizedPath::from_str("").0, "");
    }

    #[test]
    fn single_char_no_colon() {
        assert_eq!(NormalizedPath::from_str("a").0, "a");
    }

    // ---- NormalizedPath::strip_prefix ----

    #[test]
    fn strip_prefix_works() {
        let path = NormalizedPath::from_str("/Users/foo/bar");
        let prefix = NormalizedPath::from_str("/Users/foo");
        assert_eq!(path.strip_prefix(&prefix), Some("bar".to_string()));
    }

    #[test]
    fn strip_prefix_case_insensitive() {
        let path = NormalizedPath::from_str("/USERS/FOO/bar");
        let prefix = NormalizedPath::from_str("/users/foo");
        assert_eq!(path.strip_prefix(&prefix), Some("bar".to_string()));
    }

    #[test]
    fn strip_prefix_trailing_slash_on_prefix() {
        let path = NormalizedPath::from_str("/Users/foo/bar");
        let prefix = NormalizedPath::from_str("/Users/foo/");
        assert_eq!(path.strip_prefix(&prefix), Some("bar".to_string()));
    }

    #[test]
    fn strip_prefix_no_match() {
        let path = NormalizedPath::from_str("/other/path");
        let prefix = NormalizedPath::from_str("/Users/foo");
        assert_eq!(path.strip_prefix(&prefix), None);
    }

    #[test]
    fn strip_prefix_exact_match() {
        let path = NormalizedPath::from_str("/Users/foo");
        let prefix = NormalizedPath::from_str("/Users/foo");
        assert_eq!(path.strip_prefix(&prefix), Some("".to_string()));
    }

    // ---- get_relative_file_path_for_api ----

    #[test]
    fn relative_path_strips_root() {
        assert_eq!(
            get_relative_file_path_for_api(Path::new("/repo/src/main.rs"), Path::new("/repo")),
            "src/main.rs"
        );
    }

    #[test]
    fn relative_path_no_match_returns_original() {
        assert_eq!(
            get_relative_file_path_for_api(Path::new("/other/file.rs"), Path::new("/repo")),
            "/other/file.rs"
        );
    }

    #[test]
    fn relative_path_windows_style() {
        assert_eq!(
            get_relative_file_path_for_api(
                Path::new(r"C:\repo\src\main.rs"),
                Path::new(r"C:\repo")
            ),
            "src/main.rs"
        );
    }

    // ---- docker_mount_path / translate functions ----

    #[test]
    fn docker_mount_path_returns_none_when_not_docker() {
        // In test env, is_docker() returns false
        assert!(docker_mount_path().is_none());
    }

    // ---- adapt_path_for_docker (via public API, non-Docker) ----

    #[test]
    fn adapt_path_for_docker_returns_unchanged_when_not_docker() {
        assert_eq!(
            adapt_path_for_docker(Path::new("/some/path/file.rs")),
            "/some/path/file.rs"
        );
    }

    // ---- translate_to_container (Docker active via direct call) ----

    #[test]
    fn translate_to_container_maps_matching_host_path() {
        let result = translate_to_container(
            Path::new("/host/project/src/main.rs"),
            Some(Path::new("/host/project")),
        );
        assert_eq!(result, "/mount/src/main.rs");
    }

    #[test]
    fn translate_to_container_returns_unchanged_when_no_mount_prefix() {
        let result = translate_to_container(
            Path::new("/other/path/file.rs"),
            Some(Path::new("/host/project")),
        );
        assert_eq!(result, "/other/path/file.rs");
    }

    #[test]
    fn translate_to_container_returns_unchanged_when_none() {
        assert_eq!(
            translate_to_container(Path::new("/some/path"), None),
            "/some/path"
        );
    }

    // ---- translate_from_container (Docker active via direct call) ----

    #[test]
    fn adapt_path_from_docker_returns_unchanged_when_not_docker() {
        assert_eq!(
            adapt_path_from_docker(Path::new("/mount/src/file.rs")),
            "/mount/src/file.rs"
        );
    }

    #[test]
    fn translate_from_container_maps_container_path() {
        let result = translate_from_container(
            Path::new("/mount/src/main.rs"),
            Some(Path::new("/host/project")),
        );
        assert_eq!(result, "/host/project/src/main.rs");
    }

    #[test]
    fn translate_from_container_returns_unchanged_when_no_mount_prefix() {
        let result = translate_from_container(
            Path::new("/other/path/file.rs"),
            Some(Path::new("/host/project")),
        );
        assert_eq!(result, "/other/path/file.rs");
    }

    #[test]
    fn translate_from_container_returns_unchanged_when_none() {
        assert_eq!(
            translate_from_container(Path::new("/mount/src/file.rs"), None),
            "/mount/src/file.rs"
        );
    }

    // ---- get_worktree_gitdir ----

    #[test]
    fn get_worktree_gitdir_returns_none_for_normal_repo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        assert!(get_worktree_gitdir(dir.path()).is_none());
    }

    #[test]
    fn get_worktree_gitdir_returns_path_for_worktree() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".git"),
            "gitdir: /some/other/.git/worktrees/branch",
        )
        .unwrap();
        assert_eq!(
            get_worktree_gitdir(dir.path()),
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
        assert_eq!(adapt_worktree_gitdir_for_docker(&path), path);
    }
}

use std::path::Path;
use std::process::Command;

use crate::git_repository::{
    discover_repository_ids, repository_action_path_with, ProductionGitRunner,
    RepositoryDiscoveryReason,
};

fn git(working_dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(working_dir)
        .output()
        .expect("Git should execute");
    assert!(
        output.status.success(),
        "Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repository() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "--quiet"]);
    directory
}

fn git_config(repository: &Path, key: &str, value: &str) {
    git(repository, &["config", "--add", key, value]);
}

fn commit_file(repository: &Path) {
    std::fs::write(repository.join("README.md"), "content").unwrap();
    git(repository, &["add", "README.md"]);
    git(
        repository,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--quiet",
            "-m",
            "test",
        ],
    );
}

#[tokio::test]
async fn discovers_ids_from_multiple_fetch_and_push_remotes() {
    let repository = init_repository();
    git(
        repository.path(),
        &["remote", "add", "origin", "git@github.com:Acme/Web.git"],
    );
    git_config(
        repository.path(),
        "remote.origin.url",
        "https://gitlab.com/Acme/Platform/Web.git",
    );
    git_config(
        repository.path(),
        "remote.origin.pushurl",
        "ssh://git@push.example.com/Acme/Web.git",
    );
    git(
        repository.path(),
        &[
            "remote",
            "add",
            "upstream",
            "https://example.org/Shared/Library.git",
        ],
    );

    assert_eq!(
        discover_repository_ids(&ProductionGitRunner, Some(repository.path()))
            .await
            .unwrap(),
        [
            "example.org/shared/library",
            "github.com/acme/web",
            "gitlab.com/acme/platform/web",
            "push.example.com/acme/web",
        ]
    );
}

#[tokio::test]
async fn expands_instead_of_and_push_instead_of() {
    let repository = init_repository();
    git_config(
        repository.path(),
        "url.https://fetch.example.com/.insteadOf",
        "fetch:",
    );
    git_config(
        repository.path(),
        "url.ssh://git@push.example.com/.pushInsteadOf",
        "fetch:",
    );
    git(
        repository.path(),
        &["remote", "add", "origin", "fetch:Acme/Web.git"],
    );

    assert_eq!(
        discover_repository_ids(&ProductionGitRunner, Some(repository.path()))
            .await
            .unwrap(),
        ["fetch.example.com/acme/web", "push.example.com/acme/web"]
    );
}

#[tokio::test]
async fn reports_unsupported_remotes() {
    let repository = init_repository();
    git(
        repository.path(),
        &["remote", "add", "origin", "../local/repository.git"],
    );

    assert_eq!(
        discover_repository_ids(&ProductionGitRunner, Some(repository.path())).await,
        Err(RepositoryDiscoveryReason::NoSupportedGitRemotes)
    );
}

#[tokio::test]
async fn discovers_remote_from_worktree() {
    let repository = init_repository();
    commit_file(repository.path());
    git(
        repository.path(),
        &["remote", "add", "origin", "git@github.com:Acme/Web.git"],
    );
    let worktree_parent = tempfile::tempdir().unwrap();
    let worktree = worktree_parent.path().join("feature");
    git(
        repository.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "integration-feature",
            worktree.to_str().unwrap(),
        ],
    );

    assert_eq!(
        discover_repository_ids(&ProductionGitRunner, Some(&worktree))
            .await
            .unwrap(),
        ["github.com/acme/web"]
    );
}

#[test]
fn selects_docker_mount_for_current_workspace() {
    let _lock = crate::config::lock_test_env();
    let _docker = crate::environment::force_docker(true);
    std::env::set_var("CS_MOUNT_PATH", "/host/project");
    let selected =
        repository_action_path_with(None, || panic!("current directory must not be read"));
    std::env::remove_var("CS_MOUNT_PATH");

    assert_eq!(selected.unwrap(), Path::new("/mount"));
}

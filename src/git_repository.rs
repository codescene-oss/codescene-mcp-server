use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct GitCommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum GitCommandError {
    #[error("failed to execute Git")]
    Io(#[source] std::io::Error),
    #[error("Git returned non-UTF-8 output")]
    InvalidOutput,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub(crate) enum RepositoryRootError {
    #[error("the action path has no existing parent directory")]
    NoExistingDirectory,
    #[error("the action path is not inside a Git repository")]
    NotInRepository,
    #[error("Git repository discovery failed")]
    GitCommandFailed,
    #[error("Git returned an invalid repository root")]
    InvalidRepositoryRoot,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub(crate) enum RemoteUrlError {
    #[error("Git remote discovery failed")]
    GitCommandFailed,
}

#[async_trait::async_trait]
pub(crate) trait GitRunner: Send + Sync {
    async fn run(
        &self,
        args: &[&str],
        working_dir: &Path,
    ) -> Result<GitCommandOutput, GitCommandError>;
}

pub(crate) struct ProductionGitRunner;

#[async_trait::async_trait]
impl GitRunner for ProductionGitRunner {
    async fn run(
        &self,
        args: &[&str],
        working_dir: &Path,
    ) -> Result<GitCommandOutput, GitCommandError> {
        let output = git_command(args, working_dir)
            .output()
            .await
            .map_err(GitCommandError::Io)?;
        Ok(GitCommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8(output.stdout).map_err(|_| GitCommandError::InvalidOutput)?,
            stderr: String::from_utf8(output.stderr).map_err(|_| GitCommandError::InvalidOutput)?,
        })
    }
}

fn git_command(args: &[&str], working_dir: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("git");
    command
        .args(args)
        .current_dir(working_dir)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0");
    for variable in crate::config::sensitive_env_vars() {
        command.env_remove(variable);
    }
    command
}

pub(crate) async fn resolve_repository_root(
    runner: &dyn GitRunner,
    action_path: &Path,
) -> Result<PathBuf, RepositoryRootError> {
    let adapted_path = action_path_for_git(action_path);
    let working_dir = nearest_existing_directory(&adapted_path)
        .ok_or(RepositoryRootError::NoExistingDirectory)?;
    let output = runner
        .run(&["rev-parse", "--show-toplevel"], &working_dir)
        .await
        .map_err(|_| RepositoryRootError::GitCommandFailed)?;

    if !output.success {
        return if output.stderr.contains("not a git repository") {
            Err(RepositoryRootError::NotInRepository)
        } else {
            Err(RepositoryRootError::GitCommandFailed)
        };
    }

    let root = output.stdout.trim_end_matches(['\r', '\n']);
    if root.is_empty() {
        return Err(RepositoryRootError::InvalidRepositoryRoot);
    }
    Ok(PathBuf::from(root))
}

pub(crate) async fn effective_remote_urls(
    runner: &dyn GitRunner,
    repository_root: &Path,
) -> Result<Vec<String>, RemoteUrlError> {
    let remotes = successful_git_output(runner, &["remote"], repository_root).await?;
    let mut urls = BTreeSet::new();

    for remote in remotes.lines().filter(|remote| !remote.is_empty()) {
        let fetch_urls = successful_git_output(
            runner,
            &["remote", "get-url", "--all", "--", remote],
            repository_root,
        )
        .await?;
        let push_urls = successful_git_output(
            runner,
            &["remote", "get-url", "--push", "--all", "--", remote],
            repository_root,
        )
        .await?;
        urls.extend(
            fetch_urls
                .lines()
                .chain(push_urls.lines())
                .filter(|url| !url.is_empty())
                .map(str::to_string),
        );
    }

    Ok(urls.into_iter().collect())
}

async fn successful_git_output(
    runner: &dyn GitRunner,
    args: &[&str],
    working_dir: &Path,
) -> Result<String, RemoteUrlError> {
    let output = runner
        .run(args, working_dir)
        .await
        .map_err(|_| RemoteUrlError::GitCommandFailed)?;
    output
        .success
        .then_some(output.stdout)
        .ok_or(RemoteUrlError::GitCommandFailed)
}

fn action_path_for_git(action_path: &Path) -> PathBuf {
    PathBuf::from(crate::docker::adapt_path_for_docker(action_path))
}

fn nearest_existing_directory(path: &Path) -> Option<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    path.ancestors()
        .find(|candidate| candidate.is_dir())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::process::Command;
    use std::sync::Mutex;

    use super::*;

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

    fn commit_file(repository: &Path, name: &str) {
        std::fs::write(repository.join(name), "content").unwrap();
        git(repository, &["add", name]);
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

    struct MockGitRunner {
        responses: Mutex<VecDeque<GitCommandOutput>>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl MockGitRunner {
        fn new(responses: impl IntoIterator<Item = GitCommandOutput>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl GitRunner for MockGitRunner {
        async fn run(
            &self,
            args: &[&str],
            _working_dir: &Path,
        ) -> Result<GitCommandOutput, GitCommandError> {
            self.calls
                .lock()
                .unwrap()
                .push(args.iter().map(|arg| arg.to_string()).collect());
            Ok(self.responses.lock().unwrap().pop_front().unwrap())
        }
    }

    fn successful_output(stdout: &str) -> GitCommandOutput {
        GitCommandOutput {
            success: true,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    #[tokio::test]
    async fn resolves_repository_from_directory_file_and_missing_path() {
        let repository = init_repository();
        let nested = repository.path().join("src/nested");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("main.rs");
        std::fs::write(&file, "fn main() {}").unwrap();
        let missing = nested.join("future/module.rs");

        for action_path in [nested, file, missing] {
            assert_eq!(
                resolve_repository_root(&ProductionGitRunner, &action_path)
                    .await
                    .unwrap(),
                repository.path().canonicalize().unwrap()
            );
        }
    }

    #[tokio::test]
    async fn resolves_worktree_root() {
        let repository = init_repository();
        commit_file(repository.path(), "README.md");
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree = worktree_parent.path().join("feature");
        git(
            repository.path(),
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "feature",
                worktree.to_str().unwrap(),
            ],
        );

        assert_eq!(
            resolve_repository_root(&ProductionGitRunner, &worktree)
                .await
                .unwrap(),
            worktree.canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn resolves_submodule_root() {
        let submodule = init_repository();
        commit_file(submodule.path(), "README.md");
        let repository = init_repository();
        let submodule_path = repository.path().join("vendor/library");
        git(
            repository.path(),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "--quiet",
                submodule.path().to_str().unwrap(),
                submodule_path.to_str().unwrap(),
            ],
        );

        assert_eq!(
            resolve_repository_root(&ProductionGitRunner, &submodule_path)
                .await
                .unwrap(),
            submodule_path.canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn enumerates_and_deduplicates_every_fetch_and_push_url() {
        let runner = MockGitRunner::new([
            successful_output("origin\nupstream\n"),
            successful_output(
                "https://example.com/acme/web.git\nssh://git@example.com/acme/web.git\n",
            ),
            successful_output("ssh://git@example.com/acme/web.git\n"),
            successful_output("https://example.org/acme/web.git\n"),
            successful_output(
                "ssh://git@example.org/acme/web.git\nhttps://example.com/acme/web.git\n",
            ),
        ]);

        assert_eq!(
            effective_remote_urls(&runner, Path::new("/repository"))
                .await
                .unwrap(),
            [
                "https://example.com/acme/web.git",
                "https://example.org/acme/web.git",
                "ssh://git@example.com/acme/web.git",
                "ssh://git@example.org/acme/web.git",
            ]
        );
        assert_eq!(
            *runner.calls.lock().unwrap(),
            [
                vec!["remote"],
                vec!["remote", "get-url", "--all", "--", "origin"],
                vec!["remote", "get-url", "--push", "--all", "--", "origin"],
                vec!["remote", "get-url", "--all", "--", "upstream"],
                vec!["remote", "get-url", "--push", "--all", "--", "upstream"],
            ]
        );
    }

    #[tokio::test]
    async fn returns_no_urls_when_repository_has_no_remotes() {
        let runner = MockGitRunner::new([successful_output("")]);

        assert!(effective_remote_urls(&runner, Path::new("/repository"))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(*runner.calls.lock().unwrap(), [vec!["remote"]]);
    }

    #[tokio::test]
    async fn fails_when_any_remote_url_cannot_be_resolved() {
        let runner = MockGitRunner::new([
            successful_output("origin\n"),
            GitCommandOutput {
                success: false,
                stdout: String::new(),
                stderr: "failure containing a sensitive remote URL".to_string(),
            },
        ]);

        assert_eq!(
            effective_remote_urls(&runner, Path::new("/repository")).await,
            Err(RemoteUrlError::GitCommandFailed)
        );
    }

    #[test]
    fn adapts_docker_action_path_before_repository_resolution() {
        let _lock = crate::config::lock_test_env();
        let _docker = crate::environment::force_docker(true);
        std::env::set_var("CS_MOUNT_PATH", "/host/project");
        let adapted = action_path_for_git(Path::new("/host/project/src/main.rs"));
        std::env::remove_var("CS_MOUNT_PATH");

        assert_eq!(adapted, Path::new("/mount/src/main.rs"));
    }

    #[test]
    fn builds_direct_git_command_and_scrubs_sensitive_environment() {
        let command = git_command(&["rev-parse", "--show-toplevel"], Path::new("/tmp"));
        let command = command.as_std();
        assert_eq!(command.get_program(), "git");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["rev-parse", "--show-toplevel"]
        );
        let environment = command.get_envs().collect::<Vec<_>>();
        for variable in crate::config::sensitive_env_vars() {
            assert!(environment
                .iter()
                .any(|(key, value)| *key == variable && value.is_none()));
        }
    }

    #[tokio::test]
    async fn distinguishes_outside_repository_from_git_failure() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_repository_root(&ProductionGitRunner, directory.path()).await,
            Err(RepositoryRootError::NotInRepository)
        );

        struct FailingRunner;
        #[async_trait::async_trait]
        impl GitRunner for FailingRunner {
            async fn run(
                &self,
                _args: &[&str],
                _working_dir: &Path,
            ) -> Result<GitCommandOutput, GitCommandError> {
                Ok(GitCommandOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: "fatal: configuration failure".to_string(),
                })
            }
        }

        assert_eq!(
            resolve_repository_root(&FailingRunner, directory.path()).await,
            Err(RepositoryRootError::GitCommandFailed)
        );
    }
}

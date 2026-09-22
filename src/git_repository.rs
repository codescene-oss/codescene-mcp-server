use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct GitCommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

#[cfg(test)]
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
    #[cfg(test)]
    #[error("Git repository discovery failed")]
    GitCommandFailed,
    #[cfg(test)]
    #[error("Git returned an invalid repository root")]
    InvalidRepositoryRoot,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub(crate) enum RemoteUrlError {
    #[error("Git remote discovery failed")]
    GitCommandFailed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RepositoryDiscoveryReason {
    NotInGitRepository,
    NoGitRemotes,
    NoSupportedGitRemotes,
    WorkingDirectoryUnavailable,
    GitCommandFailed,
}

#[derive(Debug, PartialEq)]
pub(crate) enum EffectiveRemoteUrls {
    NoRemotes,
    Found(Vec<String>),
}

#[async_trait::async_trait]
pub(crate) trait GitRunner: Send + Sync {
    #[cfg(test)]
    async fn run(
        &self,
        args: &[&str],
        working_dir: &Path,
    ) -> Result<GitCommandOutput, GitCommandError>;

    async fn remote_urls(
        &self,
        repository_root: &Path,
    ) -> Result<EffectiveRemoteUrls, RemoteUrlError>;

    async fn repository_root(&self, action_path: &Path) -> Result<PathBuf, RepositoryRootError>;
}

pub(crate) struct ProductionGitRunner;

#[async_trait::async_trait]
impl GitRunner for ProductionGitRunner {
    #[cfg(test)]
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

    async fn remote_urls(
        &self,
        repository_root: &Path,
    ) -> Result<EffectiveRemoteUrls, RemoteUrlError> {
        read_remote_urls(repository_root)
    }

    async fn repository_root(&self, action_path: &Path) -> Result<PathBuf, RepositoryRootError> {
        find_repository_root(action_path)
    }
}

#[cfg(test)]
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

#[cfg(test)]
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

fn find_repository_root(action_path: &Path) -> Result<PathBuf, RepositoryRootError> {
    let adapted_path = action_path_for_git(action_path);
    let directory = nearest_existing_directory(&adapted_path)
        .ok_or(RepositoryRootError::NoExistingDirectory)?;
    directory
        .ancestors()
        .find(|candidate| candidate.join(".git").exists())
        .map(Path::to_path_buf)
        .ok_or(RepositoryRootError::NotInRepository)
}

#[cfg(test)]
async fn effective_remote_urls<R: GitRunner>(
    runner: &R,
    repository_root: &Path,
) -> Result<EffectiveRemoteUrls, RemoteUrlError> {
    let remotes = successful_git_output(runner, &["remote", "-v"], repository_root).await?;
    let urls = remotes
        .lines()
        .filter_map(parse_remote_verbose_line)
        .collect::<BTreeSet<_>>();
    if urls.is_empty() {
        Ok(EffectiveRemoteUrls::NoRemotes)
    } else {
        Ok(EffectiveRemoteUrls::Found(urls.into_iter().collect()))
    }
}

#[cfg(test)]
fn parse_remote_verbose_line(line: &str) -> Option<String> {
    let mut fields = line.split_whitespace();
    let _remote = fields.next()?;
    let url = fields.next()?;
    let kind = fields.next()?;
    (kind == "(fetch)" || kind == "(push)").then(|| url.to_string())
}

fn read_remote_urls(repository_root: &Path) -> Result<EffectiveRemoteUrls, RemoteUrlError> {
    let config_path = git_config_path(repository_root).ok_or(RemoteUrlError::GitCommandFailed)?;
    let config =
        std::fs::read_to_string(config_path).map_err(|_| RemoteUrlError::GitCommandFailed)?;
    let urls = parse_remote_config(&config);
    if urls.is_empty() {
        Ok(EffectiveRemoteUrls::NoRemotes)
    } else {
        Ok(EffectiveRemoteUrls::Found(urls))
    }
}

fn git_config_path(repository_root: &Path) -> Option<PathBuf> {
    let git_path = repository_root.join(".git");
    if git_path.is_dir() {
        return Some(git_path.join("config"));
    }
    let gitdir_file = std::fs::read_to_string(git_path).ok()?;
    let gitdir = gitdir_file
        .lines()
        .find_map(|line| line.trim().strip_prefix("gitdir: "))?;
    let gitdir = PathBuf::from(gitdir);
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        repository_root.join(gitdir)
    };
    gitdir.parent()?.parent().map(|path| path.join("config"))
}

fn parse_remote_config(config: &str) -> Vec<String> {
    let mut section = ConfigSection::Other;
    let mut remotes = std::collections::BTreeMap::<String, RemoteConfig>::new();
    let mut rewrites = Vec::new();
    let mut push_rewrites = Vec::new();

    for line in config.lines() {
        parse_config_line(
            line,
            &mut section,
            &mut remotes,
            &mut rewrites,
            &mut push_rewrites,
        );
    }

    expanded_remote_urls(remotes.values(), &rewrites, &push_rewrites)
}

fn expanded_remote_urls<'a>(
    remotes: impl Iterator<Item = &'a RemoteConfig>,
    rewrites: &[(String, String)],
    push_rewrites: &[(String, String)],
) -> Vec<String> {
    let mut urls = BTreeSet::new();
    for remote in remotes {
        for url in &remote.urls {
            urls.insert(apply_rewrite(url, &rewrites));
        }
        if remote.push_urls.is_empty() {
            for url in &remote.urls {
                urls.insert(apply_rewrite(url, &push_rewrites));
            }
        } else {
            for url in &remote.push_urls {
                urls.insert(apply_rewrite(url, &rewrites));
            }
        }
    }
    urls.into_iter().collect()
}

fn parse_config_line(
    line: &str,
    section: &mut ConfigSection,
    remotes: &mut std::collections::BTreeMap<String, RemoteConfig>,
    rewrites: &mut Vec<(String, String)>,
    push_rewrites: &mut Vec<(String, String)>,
) {
    let line = line.trim();
    if line.starts_with('[') {
        *section = parse_config_section(line);
        return;
    }
    let Some((key, value)) = line.split_once('=') else {
        return;
    };
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    match section {
        ConfigSection::Remote(name) => add_remote_value(remotes, name, key.trim(), value),
        ConfigSection::Url(base) => add_rewrite(rewrites, push_rewrites, base, key, value),
        ConfigSection::Other => {}
    }
}

fn add_remote_value(
    remotes: &mut std::collections::BTreeMap<String, RemoteConfig>,
    name: &str,
    key: &str,
    value: &str,
) {
    let remote = remotes.entry(name.to_string()).or_default();
    match key {
        "url" => remote.urls.push(value.to_string()),
        "pushurl" => remote.push_urls.push(value.to_string()),
        _ => {}
    }
}

fn add_rewrite(
    rewrites: &mut Vec<(String, String)>,
    push_rewrites: &mut Vec<(String, String)>,
    base: &str,
    key: &str,
    value: &str,
) {
    let rewrite = (value.to_string(), base.to_string());
    match key.trim().to_ascii_lowercase().as_str() {
        "insteadof" => rewrites.push(rewrite),
        "pushinsteadof" => push_rewrites.push(rewrite),
        _ => {}
    }
}

#[derive(Default)]
struct RemoteConfig {
    urls: Vec<String>,
    push_urls: Vec<String>,
}

enum ConfigSection {
    Remote(String),
    Url(String),
    Other,
}

fn parse_config_section(line: &str) -> ConfigSection {
    let Some((kind, value)) = line
        .strip_prefix('[')
        .and_then(|line| line.strip_suffix(']'))
        .and_then(|line| line.split_once(' '))
    else {
        return ConfigSection::Other;
    };
    let value = value.trim().trim_matches('"').to_string();
    match kind.to_ascii_lowercase().as_str() {
        "remote" => ConfigSection::Remote(value),
        "url" => ConfigSection::Url(value),
        _ => ConfigSection::Other,
    }
}

fn apply_rewrite(url: &str, rewrites: &[(String, String)]) -> String {
    rewrites
        .iter()
        .filter(|(prefix, _)| url.starts_with(prefix))
        .max_by_key(|(prefix, _)| prefix.len())
        .map_or_else(
            || url.to_string(),
            |(prefix, replacement)| format!("{}{}", replacement, &url[prefix.len()..]),
        )
}

pub(crate) async fn discover_repository_ids(
    runner: &dyn GitRunner,
    action_path: Option<&Path>,
) -> Result<Vec<String>, RepositoryDiscoveryReason> {
    let action_path = repository_action_path_with(action_path, std::env::current_dir)?;
    let repository_root = runner
        .repository_root(&action_path)
        .await
        .map_err(repository_root_reason)?;
    let remotes = runner
        .remote_urls(&repository_root)
        .await
        .map_err(|_| RepositoryDiscoveryReason::GitCommandFailed)?;
    let remote_urls = match remotes {
        EffectiveRemoteUrls::NoRemotes => return Err(RepositoryDiscoveryReason::NoGitRemotes),
        EffectiveRemoteUrls::Found(urls) => urls,
    };
    let repository_ids = remote_urls
        .iter()
        .filter_map(|remote| crate::repository_url::canonical_repository_id(remote))
        .collect::<BTreeSet<_>>();
    if repository_ids.is_empty() {
        return Err(RepositoryDiscoveryReason::NoSupportedGitRemotes);
    }
    Ok(repository_ids.into_iter().collect())
}

fn repository_root_reason(error: RepositoryRootError) -> RepositoryDiscoveryReason {
    match error {
        RepositoryRootError::NotInRepository => RepositoryDiscoveryReason::NotInGitRepository,
        RepositoryRootError::NoExistingDirectory => RepositoryDiscoveryReason::GitCommandFailed,
        #[cfg(test)]
        RepositoryRootError::GitCommandFailed | RepositoryRootError::InvalidRepositoryRoot => {
            RepositoryDiscoveryReason::GitCommandFailed
        }
    }
}

pub(crate) fn repository_action_path_with(
    action_path: Option<&Path>,
    current_dir: impl FnOnce() -> std::io::Result<PathBuf>,
) -> Result<PathBuf, RepositoryDiscoveryReason> {
    if let Some(path) = action_path {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = crate::docker::container_workspace_dir() {
        return Ok(path);
    }
    current_dir().map_err(|_| RepositoryDiscoveryReason::WorkingDirectoryUnavailable)
}

#[cfg(test)]
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

    fn git_config(repository: &Path, key: &str, value: &str) {
        git(repository, &["config", "--add", key, value]);
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

        async fn remote_urls(
            &self,
            repository_root: &Path,
        ) -> Result<EffectiveRemoteUrls, RemoteUrlError> {
            effective_remote_urls(self, repository_root).await
        }

        async fn repository_root(
            &self,
            action_path: &Path,
        ) -> Result<PathBuf, RepositoryRootError> {
            resolve_repository_root(self, action_path).await
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
        let runner = MockGitRunner::new([successful_output(
            "origin\thttps://example.com/acme/web.git (fetch)\n\
                 origin\tssh://git@example.com/acme/web.git (push)\n\
                 upstream\thttps://example.org/acme/web.git (fetch)\n\
                 upstream\tssh://git@example.org/acme/web.git (push)\n",
        )]);

        assert_eq!(
            effective_remote_urls(&runner, Path::new("/repository"))
                .await
                .unwrap(),
            EffectiveRemoteUrls::Found(vec![
                "https://example.com/acme/web.git".to_string(),
                "https://example.org/acme/web.git".to_string(),
                "ssh://git@example.com/acme/web.git".to_string(),
                "ssh://git@example.org/acme/web.git".to_string(),
            ])
        );
        assert_eq!(*runner.calls.lock().unwrap(), [vec!["remote", "-v"]]);
    }

    #[tokio::test]
    async fn trims_windows_line_endings_from_remote_names_and_urls() {
        let runner = MockGitRunner::new([successful_output(
            "origin\thttps://example.com/acme/web.git (fetch)\r\n\
                 origin\thttps://example.com/acme/web.git (push)\r\n",
        )]);

        assert_eq!(
            effective_remote_urls(&runner, Path::new("/repository"))
                .await
                .unwrap(),
            EffectiveRemoteUrls::Found(vec!["https://example.com/acme/web.git".to_string()])
        );
        assert_eq!(*runner.calls.lock().unwrap(), [vec!["remote", "-v"]]);
    }

    #[tokio::test]
    async fn returns_no_urls_when_repository_has_no_remotes() {
        let runner = MockGitRunner::new([successful_output("")]);

        assert_eq!(
            effective_remote_urls(&runner, Path::new("/repository"))
                .await
                .unwrap(),
            EffectiveRemoteUrls::NoRemotes
        );
        assert_eq!(*runner.calls.lock().unwrap(), [vec!["remote", "-v"]]);
    }

    #[tokio::test]
    async fn fails_when_any_remote_url_cannot_be_resolved() {
        let runner = MockGitRunner::new([GitCommandOutput {
            success: false,
            stdout: String::new(),
            stderr: "failure containing a sensitive remote URL".to_string(),
        }]);

        assert_eq!(
            effective_remote_urls(&runner, Path::new("/repository")).await,
            Err(RemoteUrlError::GitCommandFailed)
        );
    }

    #[tokio::test]
    async fn discovers_sorted_canonical_repository_ids() {
        let directory = tempfile::tempdir().unwrap();
        let runner = MockGitRunner::new([
            successful_output("/repository\n"),
            successful_output(
                "origin\thttps://example.org/Team/Service.git (fetch)\n\
                 origin\tgit@example.com:Acme/Web.git (push)\n\
                 origin\thttps://example.org/team/service (push)\n",
            ),
        ]);

        assert_eq!(
            discover_repository_ids(&runner, Some(directory.path()))
                .await
                .unwrap(),
            ["example.com/acme/web", "example.org/team/service"]
        );
    }

    #[tokio::test]
    async fn maps_repository_discovery_failures_to_stable_reasons() {
        let directory = tempfile::tempdir().unwrap();
        let cases = [
            (
                vec![GitCommandOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: "fatal: not a git repository".to_string(),
                }],
                RepositoryDiscoveryReason::NotInGitRepository,
            ),
            (
                vec![successful_output("/repository\n"), successful_output("")],
                RepositoryDiscoveryReason::NoGitRemotes,
            ),
            (
                vec![
                    successful_output("/repository\n"),
                    successful_output(
                        "origin\t../local/repository.git (fetch)\n\
                         origin\tfile:///tmp/repository.git (push)\n",
                    ),
                ],
                RepositoryDiscoveryReason::NoSupportedGitRemotes,
            ),
            (
                vec![GitCommandOutput {
                    success: false,
                    stdout: String::new(),
                    stderr: "fatal: configuration failed".to_string(),
                }],
                RepositoryDiscoveryReason::GitCommandFailed,
            ),
        ];

        for (responses, expected) in cases {
            let runner = MockGitRunner::new(responses);
            assert_eq!(
                discover_repository_ids(&runner, Some(directory.path())).await,
                Err(expected)
            );
        }
    }

    #[test]
    fn reports_unavailable_current_workspace() {
        let _docker = crate::environment::force_docker(false);
        let result = repository_action_path_with(None, || {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "working directory unavailable",
            ))
        });

        assert_eq!(
            result,
            Err(RepositoryDiscoveryReason::WorkingDirectoryUnavailable)
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

            async fn remote_urls(
                &self,
                _repository_root: &Path,
            ) -> Result<EffectiveRemoteUrls, RemoteUrlError> {
                Err(RemoteUrlError::GitCommandFailed)
            }

            async fn repository_root(
                &self,
                _action_path: &Path,
            ) -> Result<PathBuf, RepositoryRootError> {
                Err(RepositoryRootError::GitCommandFailed)
            }
        }

        assert_eq!(
            resolve_repository_root(&FailingRunner, directory.path()).await,
            Err(RepositoryRootError::GitCommandFailed)
        );
    }

    #[tokio::test]
    async fn rejects_empty_repository_root_output() {
        let directory = tempfile::tempdir().unwrap();
        let runner = MockGitRunner::new([successful_output("\n")]);

        assert_eq!(
            resolve_repository_root(&runner, directory.path()).await,
            Err(RepositoryRootError::InvalidRepositoryRoot)
        );
    }

    #[test]
    fn reads_empty_remote_config_and_relative_worktree_gitdir() {
        let repository = tempfile::tempdir().unwrap();
        let git_dir = repository.path().join("metadata/worktrees/feature");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(
            repository.path().join("metadata/config"),
            "[core]\n\tbare = false\n",
        )
        .unwrap();
        std::fs::write(
            repository.path().join(".git"),
            "gitdir: metadata/worktrees/feature\n",
        )
        .unwrap();

        assert_eq!(
            read_remote_urls(repository.path()).unwrap(),
            EffectiveRemoteUrls::NoRemotes
        );
    }

    #[test]
    fn ignores_malformed_empty_and_unknown_config_entries() {
        let urls = parse_remote_config(
            "malformed\n\
             [unknown \"section\"]\n\
             value = ignored\n\
             [remote \"origin\"]\n\
             url =\n\
             unknown = ignored\n\
             url = https://example.com/Acme/Web.git\n\
             [url \"https://mirror.example.com/\"]\n\
             unknown = https://example.com/\n",
        );

        assert_eq!(urls, ["https://example.com/Acme/Web.git"]);
    }

    #[test]
    fn resolves_relative_paths_from_current_directory() {
        assert!(nearest_existing_directory(Path::new("src/future/file.rs"))
            .is_some_and(|path| path.ends_with("src")));
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
        commit_file(repository.path(), "README.md");
        git(
            repository.path(),
            &["remote", "add", "origin", "git@github.com:Acme/Web.git"],
        );
        let worktree_parent = tempfile::tempdir().unwrap();
        let worktree = worktree_parent.path().join("integration-feature");
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
}

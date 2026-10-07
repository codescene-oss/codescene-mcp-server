use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::auth::AuthCredential;
use crate::git_repository::{discover_repository_ids, ProductionGitRunner};
use crate::http::HttpClient;
use crate::repository_projects::{
    matching_project_ids, RepositoryProjects, RepositoryProjectsCache,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum AnalyticsContext {
    ExplicitProjectIds(Vec<i64>),
    Path(PathBuf),
    #[default]
    CurrentWorkspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoProjectMatchingReason {
    NotInGitRepository,
    NoGitRemotes,
    NoSupportedGitRemotes,
    WorkingDirectoryUnavailable,
    GitCommandFailed,
    AuthenticationUnavailable,
    RepositoryProjectsRequestFailed,
    InvalidRepositoryProjectsResponse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AttributionOutcome {
    ProjectIds(Vec<i64>),
    Failure(NoProjectMatchingReason),
}

pub(crate) async fn resolve_attribution(
    context: AnalyticsContext,
    credential: Option<AuthCredential>,
    client: Arc<dyn HttpClient>,
    cache: Arc<RepositoryProjectsCache>,
) -> AttributionOutcome {
    let action_path = match context {
        AnalyticsContext::ExplicitProjectIds(mut ids) => {
            ids.sort_unstable();
            ids.dedup();
            return AttributionOutcome::ProjectIds(ids);
        }
        AnalyticsContext::Path(path) => Some(path),
        AnalyticsContext::CurrentWorkspace => None,
    };
    let repository_ids =
        match discover_repository_ids(&ProductionGitRunner, action_path.as_deref()).await {
            Ok(ids) => ids,
            Err(reason) => return AttributionOutcome::Failure(reason.into()),
        };
    if debug_enabled() {
        tracing::info!(repository_ids = ?repository_ids, "Project matching repositories discovered");
    }
    let Some(credential) = credential.as_ref() else {
        return AttributionOutcome::Failure(NoProjectMatchingReason::AuthenticationUnavailable);
    };
    match cache.get_or_fetch(&*client, credential).await {
        Ok(mappings) => {
            log_repository_matches(&repository_ids, &mappings);
            AttributionOutcome::ProjectIds(matching_project_ids(&repository_ids, &mappings))
        }
        Err(error) => AttributionOutcome::Failure(error.into()),
    }
}

fn debug_enabled() -> bool {
    crate::config::try_read_env("CS_DEBUG")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true") || value.trim() == "1")
}

fn log_repository_matches(repository_ids: &[String], mappings: &RepositoryProjects) {
    if !debug_enabled() {
        return;
    }
    for repository_id in repository_ids {
        let project_ids = matching_project_ids(std::slice::from_ref(repository_id), mappings);
        let repository_found = mappings
            .repositories
            .iter()
            .any(|mapping| mapping.repository_id == *repository_id);
        let match_result = if !repository_found {
            "repository-not-found"
        } else if project_ids.is_empty() {
            "no-project-ids"
        } else {
            "matched"
        };
        tracing::info!(
            repository_id,
            project_ids = ?project_ids,
            match_result,
            "Project matching repository result"
        );
        if !repository_found {
            tracing::info!(
                repository_id,
                "No exact repository mapping found. SSH host aliases are not resolved: if the remote uses an alias, compare its SSH HostName with the repository host configured in CodeScene. Also check repository path and account access."
            );
        }
    }
}

pub(crate) fn log_attribution(outcome: &AttributionOutcome) {
    if !debug_enabled() {
        return;
    }
    match outcome {
        AttributionOutcome::ProjectIds(ids) => {
            tracing::info!(project_ids = ?ids, "Project matching completed");
        }
        AttributionOutcome::Failure(reason) => {
            tracing::info!(reason = reason.as_str(), "Project matching failed");
        }
    }
}

pub(crate) fn merge_attribution(properties: &mut Value, outcome: AttributionOutcome) {
    let Some(properties) = properties.as_object_mut() else {
        return;
    };
    properties.remove("project-ids");
    properties.remove("no-project-matching-reason");
    match outcome {
        AttributionOutcome::ProjectIds(ids) => {
            properties.insert("project-ids".to_string(), json!(ids));
        }
        AttributionOutcome::Failure(reason) => {
            properties.insert(
                "no-project-matching-reason".to_string(),
                json!(reason.as_str()),
            );
        }
    }
}

impl NoProjectMatchingReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotInGitRepository => "not-in-git-repository",
            Self::NoGitRemotes => "no-git-remotes",
            Self::NoSupportedGitRemotes => "no-supported-git-remotes",
            Self::WorkingDirectoryUnavailable => "working-directory-unavailable",
            Self::GitCommandFailed => "git-command-failed",
            Self::AuthenticationUnavailable => "authentication-unavailable",
            Self::RepositoryProjectsRequestFailed => "repository-projects-request-failed",
            Self::InvalidRepositoryProjectsResponse => "invalid-repository-projects-response",
        }
    }
}

impl From<crate::git_repository::RepositoryDiscoveryReason> for NoProjectMatchingReason {
    fn from(reason: crate::git_repository::RepositoryDiscoveryReason) -> Self {
        use crate::git_repository::RepositoryDiscoveryReason;
        match reason {
            RepositoryDiscoveryReason::NotInGitRepository => Self::NotInGitRepository,
            RepositoryDiscoveryReason::NoGitRemotes => Self::NoGitRemotes,
            RepositoryDiscoveryReason::NoSupportedGitRemotes => Self::NoSupportedGitRemotes,
            RepositoryDiscoveryReason::WorkingDirectoryUnavailable => {
                Self::WorkingDirectoryUnavailable
            }
            RepositoryDiscoveryReason::GitCommandFailed => Self::GitCommandFailed,
        }
    }
}

impl From<crate::repository_projects::RepositoryProjectsError> for NoProjectMatchingReason {
    fn from(error: crate::repository_projects::RepositoryProjectsError) -> Self {
        use crate::repository_projects::RepositoryProjectsError;
        match error {
            RepositoryProjectsError::RequestFailed => Self::RepositoryProjectsRequestFailed,
            RepositoryProjectsError::InvalidResponse => Self::InvalidRepositoryProjectsResponse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::MockHttpClient;
    use crate::http::HttpResponse;
    use std::io::Write;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct LogCapture(Arc<Mutex<Vec<u8>>>);

    impl Write for LogCapture {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture_logs(action: impl FnOnce()) -> String {
        let capture = LogCapture(Arc::new(Mutex::new(Vec::new())));
        let writer = capture.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, action);
        let bytes = capture.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn debug_logging_is_off_unless_explicitly_enabled() {
        let _lock = crate::config::lock_test_env();
        std::env::remove_var("CS_DEBUG");
        assert!(!debug_enabled());
        for value in ["false", "0", "", "invalid"] {
            std::env::set_var("CS_DEBUG", value);
            let logs = capture_logs(|| log_attribution(&AttributionOutcome::ProjectIds(vec![42])));
            assert!(logs.is_empty(), "Debug logging should be off for {value:?}");
        }
        for value in ["true", "TRUE", "1", " true "] {
            std::env::set_var("CS_DEBUG", value);
            assert!(debug_enabled());
        }
        std::env::remove_var("CS_DEBUG");
    }

    #[test]
    fn debug_logging_includes_matches_empty_results_and_failures() {
        let _lock = crate::config::lock_test_env();
        std::env::set_var("CS_DEBUG", "true");
        let logs = capture_logs(|| {
            log_attribution(&AttributionOutcome::ProjectIds(vec![7, 42]));
            log_attribution(&AttributionOutcome::ProjectIds(Vec::new()));
            log_attribution(&AttributionOutcome::Failure(
                NoProjectMatchingReason::NoSupportedGitRemotes,
            ));
        });
        std::env::remove_var("CS_DEBUG");

        assert!(logs.contains("project_ids=[7, 42]"));
        assert!(logs.contains("project_ids=[]"));
        assert!(logs.contains("no-supported-git-remotes"));
    }

    #[test]
    fn debug_logging_reports_each_repository_and_ssh_alias_hint() {
        let _lock = crate::config::lock_test_env();
        let mappings = RepositoryProjects {
            repositories: vec![
                crate::repository_projects::RepositoryProject {
                    repository_id: "github.com/acme/web".to_string(),
                    project_ids: vec![42],
                },
                crate::repository_projects::RepositoryProject {
                    repository_id: "github.com/acme/empty".to_string(),
                    project_ids: Vec::new(),
                },
            ],
        };
        let repository_ids = vec![
            "github.com/acme/web".to_string(),
            "github.com/acme/empty".to_string(),
            "github-work/acme/web".to_string(),
        ];
        std::env::remove_var("CS_DEBUG");
        assert!(capture_logs(|| log_repository_matches(&repository_ids, &mappings)).is_empty());
        std::env::set_var("CS_DEBUG", "true");
        let logs = capture_logs(|| log_repository_matches(&repository_ids, &mappings));
        std::env::remove_var("CS_DEBUG");

        assert!(logs.contains(
            "repository_id=\"github.com/acme/web\" project_ids=[42] match_result=\"matched\""
        ));
        assert!(logs.contains("repository_id=\"github.com/acme/empty\" project_ids=[] match_result=\"no-project-ids\""));
        assert!(logs.contains("repository_id=\"github-work/acme/web\" project_ids=[] match_result=\"repository-not-found\""));
        assert!(logs.contains("SSH host aliases are not resolved"));
        assert!(logs.contains("HostName"));
    }

    #[test]
    fn debug_repository_logging_does_not_include_credentials() {
        let _lock = crate::config::lock_test_env();
        std::env::set_var("CS_DEBUG", "true");
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir(repository.path().join(".git")).unwrap();
        std::fs::write(
            repository.path().join(".git/config"),
            "[remote \"origin\"]\nurl = https://secret-user:secret-password@github.com/Acme/Web.git\n",
        )
        .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let logs = capture_logs(|| {
            let outcome = runtime.block_on(resolve_attribution(
                AnalyticsContext::Path(repository.path().to_path_buf()),
                Some(configured_credential()),
                Arc::new(MockHttpClient::always(HttpResponse::ok(
                    r#"{"repositories":[{"repository_id":"github.com/acme/web","project_ids":[42]}]}"#,
                ))),
                Arc::new(RepositoryProjectsCache::default()),
            ));
            log_attribution(&outcome);
        });
        std::env::remove_var("CS_DEBUG");

        assert!(logs.contains("github.com/acme/web"));
        assert!(logs.contains("project_ids=[42]"));
        for secret in ["secret-user", "secret-password", "test-token", "https://"] {
            assert!(!logs.contains(secret));
        }
    }

    fn configured_credential() -> AuthCredential {
        AuthCredential::Configured {
            access_token: "test-token".to_string(),
            onprem_url: None,
        }
    }

    #[test]
    fn models_all_attribution_sources() {
        assert_eq!(
            AnalyticsContext::default(),
            AnalyticsContext::CurrentWorkspace
        );
        assert!(matches!(
            AnalyticsContext::ExplicitProjectIds(vec![42]),
            AnalyticsContext::ExplicitProjectIds(ids) if ids == [42]
        ));
        assert!(matches!(
            AnalyticsContext::Path(PathBuf::from("/workspace/src/main.rs")),
            AnalyticsContext::Path(path) if path == std::path::Path::new("/workspace/src/main.rs")
        ));
    }

    #[test]
    fn serializes_stable_non_sensitive_reason_codes() {
        let reasons = [
            (
                NoProjectMatchingReason::NotInGitRepository,
                "not-in-git-repository",
            ),
            (NoProjectMatchingReason::NoGitRemotes, "no-git-remotes"),
            (
                NoProjectMatchingReason::NoSupportedGitRemotes,
                "no-supported-git-remotes",
            ),
            (
                NoProjectMatchingReason::WorkingDirectoryUnavailable,
                "working-directory-unavailable",
            ),
            (
                NoProjectMatchingReason::GitCommandFailed,
                "git-command-failed",
            ),
            (
                NoProjectMatchingReason::AuthenticationUnavailable,
                "authentication-unavailable",
            ),
            (
                NoProjectMatchingReason::RepositoryProjectsRequestFailed,
                "repository-projects-request-failed",
            ),
            (
                NoProjectMatchingReason::InvalidRepositoryProjectsResponse,
                "invalid-repository-projects-response",
            ),
        ];

        for (reason, expected) in reasons {
            assert_eq!(reason.as_str(), expected);
        }
    }

    #[test]
    fn maps_typed_discovery_errors_to_stable_reasons() {
        use crate::git_repository::RepositoryDiscoveryReason;
        use crate::repository_projects::RepositoryProjectsError;

        let local_reasons = [
            (
                RepositoryDiscoveryReason::NotInGitRepository,
                NoProjectMatchingReason::NotInGitRepository,
            ),
            (
                RepositoryDiscoveryReason::NoGitRemotes,
                NoProjectMatchingReason::NoGitRemotes,
            ),
            (
                RepositoryDiscoveryReason::NoSupportedGitRemotes,
                NoProjectMatchingReason::NoSupportedGitRemotes,
            ),
            (
                RepositoryDiscoveryReason::WorkingDirectoryUnavailable,
                NoProjectMatchingReason::WorkingDirectoryUnavailable,
            ),
            (
                RepositoryDiscoveryReason::GitCommandFailed,
                NoProjectMatchingReason::GitCommandFailed,
            ),
        ];
        for (source, expected) in local_reasons {
            assert_eq!(NoProjectMatchingReason::from(source), expected);
        }
        assert_eq!(
            NoProjectMatchingReason::from(RepositoryProjectsError::RequestFailed),
            NoProjectMatchingReason::RepositoryProjectsRequestFailed
        );
        assert_eq!(
            NoProjectMatchingReason::from(RepositoryProjectsError::InvalidResponse),
            NoProjectMatchingReason::InvalidRepositoryProjectsResponse
        );
    }

    #[tokio::test]
    async fn explicit_project_ids_are_sorted_without_repository_lookup() {
        let client = MockHttpClient::new(Vec::new());
        let requests = client.captured_requests.clone();
        let cache = RepositoryProjectsCache::default();

        let result = resolve_attribution(
            AnalyticsContext::ExplicitProjectIds(vec![42, 7, 42]),
            None,
            Arc::new(client),
            Arc::new(cache),
        )
        .await;

        assert_eq!(result, AttributionOutcome::ProjectIds(vec![7, 42]));
        assert!(requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn path_context_reports_repository_discovery_failure() {
        let directory = tempfile::tempdir().unwrap();

        let result = resolve_attribution(
            AnalyticsContext::Path(directory.path().to_path_buf()),
            Some(configured_credential()),
            Arc::new(MockHttpClient::new(Vec::new())),
            Arc::new(RepositoryProjectsCache::default()),
        )
        .await;

        assert_eq!(
            result,
            AttributionOutcome::Failure(NoProjectMatchingReason::NotInGitRepository)
        );
    }

    #[tokio::test]
    async fn path_context_requires_authentication_after_repository_discovery() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir(repository.path().join(".git")).unwrap();
        std::fs::write(
            repository.path().join(".git/config"),
            "[remote \"origin\"]\nurl = https://github.com/Acme/Web.git\n",
        )
        .unwrap();

        let result = resolve_attribution(
            AnalyticsContext::Path(repository.path().to_path_buf()),
            None,
            Arc::new(MockHttpClient::new(Vec::new())),
            Arc::new(RepositoryProjectsCache::default()),
        )
        .await;

        assert_eq!(
            result,
            AttributionOutcome::Failure(NoProjectMatchingReason::AuthenticationUnavailable)
        );
    }

    #[tokio::test]
    async fn path_context_matches_repository_projects() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir(repository.path().join(".git")).unwrap();
        std::fs::write(
            repository.path().join(".git/config"),
            "[remote \"origin\"]\nurl = https://github.com/Acme/Web.git\n",
        )
        .unwrap();
        let client = MockHttpClient::always(HttpResponse::ok(
            r#"{"repositories":[{"repository_id":"github.com/acme/web","project_ids":[42]}]}"#,
        ));

        let result = resolve_attribution(
            AnalyticsContext::Path(repository.path().to_path_buf()),
            Some(configured_credential()),
            Arc::new(client),
            Arc::new(RepositoryProjectsCache::default()),
        )
        .await;

        assert_eq!(result, AttributionOutcome::ProjectIds(vec![42]));
    }

    #[test]
    fn merge_preserves_properties_and_keeps_outcomes_mutually_exclusive() {
        let cases = [
            (
                AttributionOutcome::ProjectIds(vec![7, 42]),
                json!({"tool": "review", "no-project-matching-reason": "old"}),
                "project-ids",
                json!([7, 42]),
                "no-project-matching-reason",
            ),
            (
                AttributionOutcome::Failure(NoProjectMatchingReason::GitCommandFailed),
                json!({"tool": "review", "project-ids": [99]}),
                "no-project-matching-reason",
                json!("git-command-failed"),
                "project-ids",
            ),
        ];

        for (outcome, mut properties, expected_key, expected_value, absent_key) in cases {
            merge_attribution(&mut properties, outcome);
            assert_eq!(properties["tool"], "review");
            assert_eq!(properties[expected_key], expected_value);
            assert!(properties.get(absent_key).is_none());
        }
    }

    #[test]
    fn merge_leaves_non_object_properties_unchanged() {
        let mut properties = json!("not-an-object");
        merge_attribution(
            &mut properties,
            AttributionOutcome::Failure(NoProjectMatchingReason::GitCommandFailed),
        );
        assert_eq!(properties, "not-an-object");
    }
}

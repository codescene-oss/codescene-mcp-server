use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::auth::AuthCredential;
use crate::git_repository::{discover_repository_ids, ProductionGitRunner};
use crate::http::HttpClient;
use crate::repository_projects::{matching_project_ids, RepositoryProjectsCache};

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
    let Some(credential) = credential.as_ref() else {
        return AttributionOutcome::Failure(NoProjectMatchingReason::AuthenticationUnavailable);
    };
    match cache.get_or_fetch(&*client, credential).await {
        Ok(mappings) => {
            AttributionOutcome::ProjectIds(matching_project_ids(&repository_ids, &mappings))
        }
        Err(error) => AttributionOutcome::Failure(error.into()),
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
            AnalyticsContext::Path(path) if path == PathBuf::from("/workspace/src/main.rs")
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

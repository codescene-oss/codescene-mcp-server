use std::path::PathBuf;

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
}

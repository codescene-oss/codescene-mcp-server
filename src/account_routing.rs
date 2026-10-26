use std::collections::BTreeSet;

use crate::analytics_attribution::AnalyticsContext;
use crate::auth::AuthCredential;
use crate::git_repository::{discover_repository_ids, ProductionGitRunner};
use crate::repository_projects::{ProjectAccount, RepositoryProjects};
use crate::CodeSceneServer;

impl CodeSceneServer {
    pub(crate) async fn route_oauth_account(
        &self,
        credential: AuthCredential,
        context: &AnalyticsContext,
    ) -> Result<AuthCredential, String> {
        if !matches!(credential, AuthCredential::OAuth { .. }) || self.is_standalone {
            return Ok(credential);
        }
        let path = match context {
            AnalyticsContext::Path(path) => Some(path.as_path()),
            _ => None,
        };
        let repository_ids = match discover_repository_ids(&ProductionGitRunner, path).await {
            Ok(ids) => ids,
            Err(reason) => {
                tracing::debug!(
                    ?reason,
                    "automatic account selection has no repository context"
                );
                return Ok(credential);
            }
        };
        let mappings = match self
            .repository_projects_cache
            .get_or_fetch(&*self.http_client, &credential)
            .await
        {
            Ok(mappings) => mappings,
            Err(error) => {
                tracing::warn!(%error, "automatic account selection unavailable; keeping current session");
                return Ok(credential);
            }
        };
        let accounts = candidate_accounts(&mappings, &repository_ids, context);
        let current = self.auth_manager.try_cached_oauth_account_id();
        let Some(target) = account_to_switch(&accounts, current) else {
            tracing::debug!(?accounts, ?current, "keeping current OAuth account");
            return Ok(credential);
        };
        tracing::info!(
            target,
            ?current,
            "automatically switching OAuth account for repository"
        );
        self.auth_manager
            .switch_account(&*self.cli_runner, target)
            .await?;
        if self.auth_manager.try_cached_oauth_account_id() != Some(target) {
            return Err(format!(
                "OAuth sign-in did not authenticate the requested account {target}"
            ));
        }
        self.auth_manager
            .resolve_credential(&*self.cli_runner)
            .await?
            .ok_or_else(|| {
                format!("No OAuth credential available after switching to account {target}")
            })
    }
}

fn candidate_accounts(
    mappings: &RepositoryProjects,
    repository_ids: &[String],
    context: &AnalyticsContext,
) -> BTreeSet<i64> {
    let mut accounts = BTreeSet::new();
    for repository in mappings
        .repositories
        .iter()
        .filter(|repository| repository_ids.contains(&repository.repository_id))
    {
        for project_id in &repository.project_ids {
            if !context_includes_project(context, *project_id) {
                continue;
            }
            let Some(project_accounts) = accounts_for_project(&repository.projects, *project_id)
            else {
                // Incomplete/legacy metadata cannot establish an unambiguous account.
                return BTreeSet::new();
            };
            accounts.extend(project_accounts);
        }
    }
    accounts
}

fn accounts_for_project(projects: &[ProjectAccount], project_id: i64) -> Option<BTreeSet<i64>> {
    let accounts: BTreeSet<_> = projects
        .iter()
        .filter(|project| project.project_id == project_id)
        .map(|project| project.account_id)
        .collect();
    if project_id <= 0 || accounts.first().is_none_or(|id| *id <= 0) {
        return None;
    }
    Some(accounts)
}

fn context_includes_project(context: &AnalyticsContext, project_id: i64) -> bool {
    match context {
        AnalyticsContext::ExplicitProjectIds(ids) => ids.contains(&project_id),
        _ => true,
    }
}

fn account_to_switch(accounts: &BTreeSet<i64>, current: Option<i64>) -> Option<i64> {
    if current.is_some_and(|id| accounts.contains(&id)) || accounts.len() != 1 {
        return None;
    }
    accounts.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository_projects::RepositoryProject;

    #[test]
    fn switches_only_to_a_unique_non_current_account() {
        for (accounts, current, expected) in [
            (vec![3], Some(1), Some(3)),
            (vec![3], Some(3), None),
            (vec![1, 3], Some(1), None),
            (vec![3, 4], Some(1), None),
            (vec![], Some(1), None),
            (vec![3], None, Some(3)),
        ] {
            assert_eq!(
                account_to_switch(&accounts.into_iter().collect(), current),
                expected
            );
        }
    }

    fn mappings() -> RepositoryProjects {
        RepositoryProjects {
            repositories: vec![
                RepositoryProject {
                    repository_id: "github.com/acme/web".to_string(),
                    project_ids: vec![2, 10],
                    projects: vec![
                        ProjectAccount {
                            project_id: 2,
                            account_id: 1,
                        },
                        ProjectAccount {
                            project_id: 10,
                            account_id: 3,
                        },
                    ],
                },
                RepositoryProject {
                    repository_id: "github.com/other/repo".to_string(),
                    project_ids: vec![20],
                    projects: vec![ProjectAccount {
                        project_id: 20,
                        account_id: 4,
                    }],
                },
            ],
        }
    }

    #[test]
    fn considers_only_matched_repositories_and_requested_projects() {
        let mappings = mappings();
        let repositories = vec!["github.com/acme/web".to_string()];
        for (context, expected) in [
            (AnalyticsContext::CurrentWorkspace, vec![1, 3]),
            (AnalyticsContext::ExplicitProjectIds(vec![10]), vec![3]),
            (AnalyticsContext::ExplicitProjectIds(vec![20]), vec![]),
        ] {
            assert_eq!(
                candidate_accounts(&mappings, &repositories, &context),
                expected.into_iter().collect()
            );
        }
    }

    #[test]
    fn incomplete_or_invalid_account_metadata_never_selects_an_account() {
        let repositories = vec!["github.com/acme/web".to_string()];
        for projects in [
            vec![],
            vec![ProjectAccount {
                project_id: 10,
                account_id: 3,
            }],
            vec![
                ProjectAccount {
                    project_id: 2,
                    account_id: 0,
                },
                ProjectAccount {
                    project_id: 10,
                    account_id: 3,
                },
            ],
        ] {
            let mut mappings = mappings();
            mappings.repositories[0].projects = projects;
            assert!(candidate_accounts(
                &mappings,
                &repositories,
                &AnalyticsContext::CurrentWorkspace
            )
            .is_empty());
        }
    }

    #[test]
    fn conflicting_metadata_does_not_hide_an_account() {
        let projects = [
            ProjectAccount {
                project_id: 10,
                account_id: 3,
            },
            ProjectAccount {
                project_id: 10,
                account_id: 4,
            },
        ];
        let accounts = accounts_for_project(&projects, 10).unwrap();
        assert_eq!(account_to_switch(&accounts, Some(1)), None);
        assert_eq!(accounts, BTreeSet::from([3, 4]));
    }
}

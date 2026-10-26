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
    use crate::http::{tests::MockHttpClient, HttpResponse};
    use crate::repository_projects::RepositoryProject;
    use crate::test_utils::{
        clear_token, make_server_with_mocks, result_text, MockCliRunner, TokenGuard,
    };
    use serde_json::json;

    struct RoutingEnv {
        _guard: TokenGuard<'static>,
        directory: tempfile::TempDir,
        previous_env: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl RoutingEnv {
        fn new() -> Self {
            let guard = clear_token();
            let directory = tempfile::tempdir().unwrap();
            let previous_env = ["CS_CONFIG_DIR", "CS_ONPREM_URL", "CS_DISABLE_TRACKING"]
                .into_iter()
                .map(|key| (key, std::env::var_os(key)))
                .collect();
            std::fs::create_dir(directory.path().join(".git")).unwrap();
            std::fs::write(
                directory.path().join(".git/config"),
                "[remote \"origin\"]\nurl = git@github.com:acme/web.git\n",
            )
            .unwrap();
            std::env::set_var("CS_CONFIG_DIR", directory.path());
            std::env::set_var("CS_DISABLE_TRACKING", "1");
            std::env::set_var("CS_ONPREM_URL", "https://codescene.example");
            std::env::set_var("CS_OAUTH_TOKEN", "original-token");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", "4102444800");
            std::env::set_var("CS_OAUTH_ACCOUNT_ID", "1");
            Self {
                _guard: guard,
                directory,
                previous_env,
            }
        }

        fn context(&self) -> AnalyticsContext {
            AnalyticsContext::Path(self.directory.path().to_path_buf())
        }

        fn credential(&self) -> AuthCredential {
            AuthCredential::OAuth {
                access_token: "original-token".to_string(),
                onprem_url: Some("https://codescene.example".to_string()),
            }
        }
    }

    impl Drop for RoutingEnv {
        fn drop(&mut self) {
            for (key, value) in &self.previous_env {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn mapping_response(projects: serde_json::Value) -> HttpResponse {
        HttpResponse::ok(
            &json!({"repositories": [{
                "repository_id": "github.com/acme/web",
                "project_ids": [10],
                "projects": projects,
            }]})
            .to_string(),
        )
    }

    fn target_mapping() -> HttpResponse {
        mapping_response(json!([{"project_id": 10, "account_id": 3}]))
    }

    fn signed_in(account: i64, expires_at: i64) -> String {
        json!({
            "status": "signed_in",
            "access-token": "target-token",
            "account-id": account,
            "expires-at": expires_at,
            "api-url": "https://codescene.example/api",
        })
        .to_string()
    }

    const SIGNED_OUT: &str = r#"{"status":"signed_out"}"#;

    #[tokio::test]
    async fn bypasses_routing_for_configured_credentials_and_standalone_mode() {
        let env = RoutingEnv::new();
        let configured = AuthCredential::Configured {
            access_token: "pat".to_string(),
            onprem_url: None,
        };
        for (credential, standalone) in [(configured, false), (env.credential(), true)] {
            let cli = MockCliRunner::with_responses(vec![]);
            let calls = cli.calls();
            let http = MockHttpClient::new(vec![]);
            let requests = http.captured_requests.clone();
            let server = make_server_with_mocks(standalone, cli, http);
            assert_eq!(
                server
                    .route_oauth_account(credential.clone(), &env.context())
                    .await
                    .unwrap(),
                credential
            );
            assert!(calls.lock().unwrap().is_empty());
            assert!(requests.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn keeps_credential_when_action_is_outside_a_repository() {
        let env = RoutingEnv::new();
        let outside = tempfile::tempdir().unwrap();
        let server = make_server_with_mocks(
            false,
            MockCliRunner::with_responses(vec![]),
            MockHttpClient::new(vec![]),
        );
        let credential = server
            .route_oauth_account(
                env.credential(),
                &AnalyticsContext::Path(outside.path().to_path_buf()),
            )
            .await
            .unwrap();
        assert_eq!(credential, env.credential());
    }

    #[tokio::test]
    async fn keeps_session_when_mapping_is_unavailable_or_not_unique() {
        let env = RoutingEnv::new();
        for response in [
            HttpResponse::error(503, "unavailable"),
            HttpResponse::ok("{}"),
            HttpResponse::ok(
                r#"{"repositories":[{"repository_id":"github.com/acme/web","project_ids":[10]}]}"#,
            ),
            mapping_response(json!([{"project_id": 10, "account_id": 1}])),
            mapping_response(
                json!([{"project_id": 10, "account_id": 3}, {"project_id": 10, "account_id": 4}]),
            ),
        ] {
            let cli = MockCliRunner::with_responses(vec![]);
            let calls = cli.calls();
            let http = MockHttpClient::new(vec![response]);
            let requests = http.captured_requests.clone();
            let server = make_server_with_mocks(false, cli, http);
            assert_eq!(
                server
                    .route_oauth_account(env.credential(), &env.context())
                    .await
                    .unwrap(),
                env.credential()
            );
            assert!(calls.lock().unwrap().is_empty());
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(
                requests[0].url,
                "https://codescene.example/api/v2/mcp/repository-projects"
            );
        }
    }

    #[tokio::test]
    async fn authenticates_target_account_before_allowing_a_file_tool() {
        for interactive in [false, true] {
            let env = RoutingEnv::new();
            let mut responses = vec![Ok(signed_in(3, 4102444800))];
            if interactive {
                responses.insert(0, Ok(SIGNED_OUT.to_string()));
            }
            let cli = MockCliRunner::with_responses(responses);
            let calls = cli.calls();
            let server =
                make_server_with_mocks(false, cli, MockHttpClient::new(vec![target_mapping()]));
            assert!(server
                .require_token_with_context("routing-test", env.context())
                .await
                .is_none());
            assert_eq!(server.auth_manager.try_cached_oauth_account_id(), Some(3));
            assert_eq!(std::env::var("CS_ACCOUNT_ID").unwrap(), "3");
            assert_eq!(std::env::var("CS_OAUTH_TOKEN").unwrap(), "target-token");
            assert_eq!(calls.lock().unwrap().len(), if interactive { 2 } else { 1 });
            let config = crate::config::load().unwrap();
            assert_eq!(config.values["oauth_account_id"], "3");
            assert_eq!(config.values["oauth_token"], "target-token");
        }
    }

    #[tokio::test]
    async fn failed_or_wrong_account_authentication_blocks_the_tool() {
        for responses in [
            vec![Ok(SIGNED_OUT.to_string()), Ok(SIGNED_OUT.to_string())],
            vec![Ok(signed_in(4, 4102444800))],
            vec![Ok(signed_in(3, 1)), Ok(SIGNED_OUT.to_string())],
            vec![
                Ok(signed_in(3, 1)),
                Err(crate::errors::CliError::NotFound("cs".to_string())),
            ],
        ] {
            let env = RoutingEnv::new();
            let server = make_server_with_mocks(
                false,
                MockCliRunner::with_responses(responses),
                MockHttpClient::new(vec![target_mapping()]),
            );
            let result = server
                .require_token_with_context("routing-test", env.context())
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            assert!(result_text(&result).contains("Automatic CodeScene account switching failed"));
        }
    }

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

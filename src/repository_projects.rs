use serde::Deserialize;
use std::collections::BTreeSet;

use crate::api_client;
use crate::auth::AuthCredential;
use crate::errors::ApiError;
use crate::http::HttpClient;

const ENDPOINT: &str = "mcp/repository-projects";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RepositoryProjects {
    pub(crate) repositories: Vec<RepositoryProject>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RepositoryProject {
    pub(crate) repository_id: String,
    pub(crate) project_ids: Vec<i64>,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub(crate) enum RepositoryProjectsError {
    #[error("repository-project mapping request failed")]
    RequestFailed,
    #[error("repository-project mapping response is invalid")]
    InvalidResponse,
}

#[derive(Deserialize)]
struct RepositoryProjectsResponse {
    repositories: Vec<RepositoryProjectResponse>,
}

#[derive(Deserialize)]
struct RepositoryProjectResponse {
    repository_id: String,
    project_ids: Vec<i64>,
}

pub(crate) async fn fetch_repository_projects(
    client: &dyn HttpClient,
    credential: &AuthCredential,
) -> Result<RepositoryProjects, RepositoryProjectsError> {
    let response = api_client::query_api_with_auth(ENDPOINT, client, Some(credential))
        .await
        .map_err(classify_api_error)?;
    let response: RepositoryProjectsResponse =
        serde_json::from_value(response).map_err(|_| RepositoryProjectsError::InvalidResponse)?;
    let repositories = response
        .repositories
        .into_iter()
        .map(|repository| {
            if repository.repository_id.trim().is_empty() {
                return Err(RepositoryProjectsError::InvalidResponse);
            }
            Ok(RepositoryProject {
                repository_id: repository.repository_id,
                project_ids: repository.project_ids,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(RepositoryProjects { repositories })
}

pub(crate) fn matching_project_ids(
    repository_ids: &[String],
    mappings: &RepositoryProjects,
) -> Vec<i64> {
    let repository_ids = repository_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    mappings
        .repositories
        .iter()
        .filter(|mapping| repository_ids.contains(mapping.repository_id.as_str()))
        .flat_map(|mapping| mapping.project_ids.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn classify_api_error(error: ApiError) -> RepositoryProjectsError {
    match error {
        ApiError::Status {
            status: 200..=299, ..
        } => RepositoryProjectsError::InvalidResponse,
        ApiError::Http(_) | ApiError::Transport(_) | ApiError::Status { .. } => {
            RepositoryProjectsError::RequestFailed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::MockHttpClient;
    use crate::http::{HttpResponse, Method};

    fn credential(onprem_url: Option<&str>) -> AuthCredential {
        AuthCredential::Configured {
            access_token: "secret-token".to_string(),
            onprem_url: onprem_url.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn fetches_typed_cloud_repository_projects() {
        let client = MockHttpClient::always(HttpResponse::ok(
            r#"{"repositories":[{"repository_id":"github.com/acme/web","project_ids":[42,7]}]}"#,
        ));

        let result = fetch_repository_projects(&client, &credential(None))
            .await
            .unwrap();

        assert_eq!(
            result.repositories,
            [RepositoryProject {
                repository_id: "github.com/acme/web".to_string(),
                project_ids: vec![42, 7],
            }]
        );
        let requests = client.captured_requests.lock().unwrap();
        assert_eq!(requests[0].method, Method::Get);
        assert_eq!(
            requests[0].url,
            "https://api.codescene.io/mcp/repository-projects"
        );
        assert_eq!(
            requests[0].headers.get("Authorization").map(String::as_str),
            Some("Bearer secret-token")
        );
    }

    #[tokio::test]
    async fn fetches_onprem_repository_projects_relative_to_api_root() {
        let client = MockHttpClient::always(HttpResponse::ok(r#"{"repositories":[]}"#));

        fetch_repository_projects(&client, &credential(Some("https://codescene.example")))
            .await
            .unwrap();

        assert_eq!(
            client.captured_requests.lock().unwrap()[0].url,
            "https://codescene.example/api/mcp/repository-projects"
        );
    }

    #[tokio::test]
    async fn rejects_structurally_invalid_responses() {
        for body in [
            r#"{}"#,
            r#"{"repositories":null}"#,
            r#"{"repositories":[{}]}"#,
            r#"{"repositories":[{"repository_id":1,"project_ids":[42]}]}"#,
            r#"{"repositories":[{"repository_id":"github.com/acme/web","project_ids":["42"]}]}"#,
            "not-json",
        ] {
            let client = MockHttpClient::always(HttpResponse::ok(body));
            assert_eq!(
                fetch_repository_projects(&client, &credential(None)).await,
                Err(RepositoryProjectsError::InvalidResponse),
                "invalid response was accepted: {body}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_blank_repository_identifiers() {
        for body in [
            r#"{"repositories":[{"repository_id":"","project_ids":[42]}]}"#,
            r#"{"repositories":[{"repository_id":"   ","project_ids":[42]}]}"#,
        ] {
            let client = MockHttpClient::always(HttpResponse::ok(body));
            assert_eq!(
                fetch_repository_projects(&client, &credential(None)).await,
                Err(RepositoryProjectsError::InvalidResponse)
            );
        }
    }

    #[tokio::test]
    async fn accepts_any_integer_project_id_from_the_endpoint_contract() {
        let client = MockHttpClient::always(HttpResponse::ok(
            r#"{"repositories":[{"repository_id":"github.com/acme/web","project_ids":[0,-1]}]}"#,
        ));

        let result = fetch_repository_projects(&client, &credential(None))
            .await
            .unwrap();

        assert_eq!(result.repositories[0].project_ids, [0, -1]);
    }

    #[tokio::test]
    async fn classifies_transport_and_status_failures_as_request_failures() {
        let transport_failure = MockHttpClient::new(Vec::new());
        assert_eq!(
            fetch_repository_projects(&transport_failure, &credential(None)).await,
            Err(RepositoryProjectsError::RequestFailed)
        );

        let status_failure = MockHttpClient::always(HttpResponse::error(503, "unavailable"));
        assert_eq!(
            fetch_repository_projects(&status_failure, &credential(None)).await,
            Err(RepositoryProjectsError::RequestFailed)
        );
    }

    #[test]
    fn matches_exact_repository_ids_and_unions_sorted_project_ids() {
        let mappings = RepositoryProjects {
            repositories: vec![
                RepositoryProject {
                    repository_id: "github.com/acme/web".to_string(),
                    project_ids: vec![456, 123, 456],
                },
                RepositoryProject {
                    repository_id: "gitlab.com/acme/web".to_string(),
                    project_ids: vec![789, 123],
                },
                RepositoryProject {
                    repository_id: "github.com/acme/web-extra".to_string(),
                    project_ids: vec![999],
                },
            ],
        };
        let repository_ids = vec![
            "gitlab.com/acme/web".to_string(),
            "github.com/acme/web".to_string(),
            "github.com/acme/web".to_string(),
        ];

        assert_eq!(
            matching_project_ids(&repository_ids, &mappings),
            [123, 456, 789]
        );
    }

    #[test]
    fn matching_is_case_sensitive_and_does_not_use_prefixes() {
        let mappings = RepositoryProjects {
            repositories: vec![RepositoryProject {
                repository_id: "github.com/acme/web".to_string(),
                project_ids: vec![42],
            }],
        };

        assert!(matching_project_ids(
            &[
                "GitHub.com/acme/web".to_string(),
                "github.com/acme".to_string(),
                "github.com/acme/web-extra".to_string(),
            ],
            &mappings,
        )
        .is_empty());
    }

    #[test]
    fn successful_non_match_returns_empty_project_ids() {
        let mappings = RepositoryProjects {
            repositories: vec![RepositoryProject {
                repository_id: "github.com/other/service".to_string(),
                project_ids: vec![42],
            }],
        };

        assert!(matching_project_ids(&["github.com/acme/web".to_string()], &mappings).is_empty());
        assert!(matching_project_ids(&[], &mappings).is_empty());
    }
}

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::api_client;
use crate::auth::AuthCredential;
use crate::errors::ApiError;
use crate::http::HttpClient;

const CLOUD_ENDPOINT: &str = "mcp/repository-projects";
const ONPREM_ENDPOINT: &str = "v2/mcp/repository-projects";
const SUCCESS_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const FAILURE_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RepositoryProjects {
    pub(crate) repositories: Vec<RepositoryProject>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RepositoryProject {
    pub(crate) repository_id: String,
    pub(crate) project_ids: Vec<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    api_root: String,
    token_fingerprint: [u8; 32],
}

struct CacheEntry {
    value: CachedValue,
    fetched_at: Instant,
}

#[derive(Clone)]
enum CachedValue {
    Success(RepositoryProjects),
    Failure(RepositoryProjectsError),
}

impl CachedValue {
    fn result(&self) -> Result<RepositoryProjects, RepositoryProjectsError> {
        match self {
            Self::Success(mappings) => Ok(mappings.clone()),
            Self::Failure(error) => Err(*error),
        }
    }
}

type SharedCacheEntry = Arc<Mutex<Option<CacheEntry>>>;

pub(crate) struct RepositoryProjectsCache {
    entries: Mutex<HashMap<CacheKey, SharedCacheEntry>>,
    success_ttl: Duration,
    failure_ttl: Duration,
}

impl Default for RepositoryProjectsCache {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            success_ttl: SUCCESS_CACHE_TTL,
            failure_ttl: FAILURE_CACHE_TTL,
        }
    }
}

impl RepositoryProjectsCache {
    pub(crate) async fn get_or_fetch(
        &self,
        client: &dyn HttpClient,
        credential: &AuthCredential,
    ) -> Result<RepositoryProjects, RepositoryProjectsError> {
        let key = cache_key(credential)?;
        let shared_entry = {
            let mut entries = self.entries.lock().await;
            entries
                .entry(key)
                .or_insert_with(|| Arc::new(Mutex::new(None)))
                .clone()
        };
        let mut entry = shared_entry.lock().await;
        if let Some(cached) = entry.as_ref().filter(|cached| {
            let ttl = match cached.value {
                CachedValue::Success(_) => self.success_ttl,
                CachedValue::Failure(_) => self.failure_ttl,
            };
            cached.fetched_at.elapsed() < ttl
        }) {
            return cached.value.result();
        }

        let result = fetch_repository_projects(client, credential).await;
        let value = match &result {
            Ok(mappings) => CachedValue::Success(mappings.clone()),
            Err(error) => CachedValue::Failure(*error),
        };
        *entry = Some(CacheEntry {
            value,
            fetched_at: Instant::now(),
        });
        result
    }

    #[cfg(test)]
    fn with_success_ttl(success_ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            success_ttl,
            failure_ttl: FAILURE_CACHE_TTL,
        }
    }

    #[cfg(test)]
    fn with_failure_ttl(failure_ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            success_ttl: SUCCESS_CACHE_TTL,
            failure_ttl,
        }
    }
}

fn cache_key(credential: &AuthCredential) -> Result<CacheKey, RepositoryProjectsError> {
    let api_root = credential
        .api_root()
        .map_err(|_| RepositoryProjectsError::RequestFailed)?;
    let api_root = reqwest::Url::parse(&api_root)
        .map_err(|_| RepositoryProjectsError::RequestFailed)?
        .to_string()
        .trim_end_matches('/')
        .to_string();
    let token_fingerprint = Sha256::digest(credential.access_token().as_bytes()).into();
    Ok(CacheKey {
        api_root,
        token_fingerprint,
    })
}

pub(crate) async fn fetch_repository_projects(
    client: &dyn HttpClient,
    credential: &AuthCredential,
) -> Result<RepositoryProjects, RepositoryProjectsError> {
    let endpoint = if credential.web_root().is_some() {
        ONPREM_ENDPOINT
    } else {
        CLOUD_ENDPOINT
    };
    let response = api_client::query_api_with_auth(endpoint, client, Some(credential))
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
    use crate::http::{HttpRequest, HttpResponse, Method};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn credential(onprem_url: Option<&str>) -> AuthCredential {
        AuthCredential::Configured {
            access_token: "secret-token".to_string(),
            onprem_url: onprem_url.map(str::to_string),
        }
    }

    async fn fetch_twice(cache: &RepositoryProjectsCache, client: &MockHttpClient) {
        let credential = credential(None);
        cache.get_or_fetch(client, &credential).await.unwrap();
        cache.get_or_fetch(client, &credential).await.unwrap();
    }

    async fn assert_cached_failure(response: HttpResponse, expected: RepositoryProjectsError) {
        let cache = RepositoryProjectsCache::default();
        let client = MockHttpClient::new(vec![response]);
        let credential = credential(None);

        assert_eq!(
            cache.get_or_fetch(&client, &credential).await,
            Err(expected)
        );
        assert_eq!(
            cache.get_or_fetch(&client, &credential).await,
            Err(expected)
        );
        assert_eq!(client.captured_requests.lock().unwrap().len(), 1);
    }

    struct DelayedHttpClient {
        requests: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl HttpClient for DelayedHttpClient {
        async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, String> {
            self.requests.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(HttpResponse::ok(
                r#"{"repositories":[{"repository_id":"github.com/acme/web","project_ids":[42]}]}"#,
            ))
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
            "https://codescene.example/api/v2/mcp/repository-projects"
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

    #[tokio::test]
    async fn reuses_successful_mapping_within_fifteen_minute_ttl() {
        let cache = RepositoryProjectsCache::default();
        let client = MockHttpClient::new(vec![HttpResponse::ok(r#"{"repositories":[]}"#)]);

        fetch_twice(&cache, &client).await;

        assert_eq!(client.captured_requests.lock().unwrap().len(), 1);
        assert_eq!(cache.success_ttl, Duration::from_secs(900));
    }

    #[tokio::test]
    async fn isolates_cached_mappings_by_api_root_and_token_fingerprint() {
        let cache = RepositoryProjectsCache::default();
        let client = MockHttpClient::new(vec![
            HttpResponse::ok(r#"{"repositories":[]}"#),
            HttpResponse::ok(r#"{"repositories":[]}"#),
            HttpResponse::ok(r#"{"repositories":[]}"#),
        ]);
        let cloud = credential(None);
        let other_token = AuthCredential::Configured {
            access_token: "other-token".to_string(),
            onprem_url: None,
        };
        let onprem = credential(Some("https://codescene.example/"));

        cache.get_or_fetch(&client, &cloud).await.unwrap();
        cache.get_or_fetch(&client, &other_token).await.unwrap();
        cache.get_or_fetch(&client, &onprem).await.unwrap();

        assert_eq!(client.captured_requests.lock().unwrap().len(), 3);
        let entries = cache.entries.lock().await;
        assert_eq!(entries.len(), 3);
        assert!(entries
            .keys()
            .all(|key| { !key.api_root.ends_with('/') && key.token_fingerprint.len() == 32 }));
    }

    #[tokio::test]
    async fn normalized_api_roots_share_the_same_cache_entry() {
        let cache = RepositoryProjectsCache::default();
        let client = MockHttpClient::new(vec![HttpResponse::ok(r#"{"repositories":[]}"#)]);
        let without_slash = credential(Some("https://codescene.example"));
        let with_slash = credential(Some("https://codescene.example/"));

        cache.get_or_fetch(&client, &without_slash).await.unwrap();
        cache.get_or_fetch(&client, &with_slash).await.unwrap();

        assert_eq!(client.captured_requests.lock().unwrap().len(), 1);
        assert_eq!(cache.entries.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn coalesces_concurrent_misses_for_the_same_authentication_context() {
        let cache = Arc::new(RepositoryProjectsCache::default());
        let client = Arc::new(DelayedHttpClient {
            requests: AtomicUsize::new(0),
        });
        let credential = Arc::new(credential(None));
        let first = cache.get_or_fetch(&*client, &credential);
        let second = cache.get_or_fetch(&*client, &credential);
        let third = cache.get_or_fetch(&*client, &credential);

        let (first, second, third) = tokio::join!(first, second, third);

        assert_eq!(first.as_ref().unwrap(), second.as_ref().unwrap());
        assert_eq!(second.as_ref().unwrap(), third.as_ref().unwrap());
        assert_eq!(client.requests.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn refreshes_successful_mapping_after_ttl() {
        let cache = RepositoryProjectsCache::with_success_ttl(Duration::ZERO);
        let client = MockHttpClient::new(vec![
            HttpResponse::ok(r#"{"repositories":[]}"#),
            HttpResponse::ok(r#"{"repositories":[]}"#),
        ]);

        fetch_twice(&cache, &client).await;

        assert_eq!(client.captured_requests.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn reuses_request_failure_during_thirty_second_cooldown() {
        assert_cached_failure(
            HttpResponse::error(503, "unavailable"),
            RepositoryProjectsError::RequestFailed,
        )
        .await;
        assert_eq!(FAILURE_CACHE_TTL, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn reuses_invalid_response_failure_without_returning_empty_mappings() {
        assert_cached_failure(
            HttpResponse::ok(r#"{"repositories":null}"#),
            RepositoryProjectsError::InvalidResponse,
        )
        .await;
    }

    #[tokio::test]
    async fn retries_mapping_request_after_failure_cooldown() {
        let cache = RepositoryProjectsCache::with_failure_ttl(Duration::ZERO);
        let client = MockHttpClient::new(vec![
            HttpResponse::error(503, "unavailable"),
            HttpResponse::ok(r#"{"repositories":[]}"#),
        ]);
        let credential = credential(None);

        assert_eq!(
            cache.get_or_fetch(&client, &credential).await,
            Err(RepositoryProjectsError::RequestFailed)
        );
        assert_eq!(
            cache.get_or_fetch(&client, &credential).await,
            Ok(RepositoryProjects {
                repositories: Vec::new()
            })
        );
        assert_eq!(client.captured_requests.lock().unwrap().len(), 2);
    }
}

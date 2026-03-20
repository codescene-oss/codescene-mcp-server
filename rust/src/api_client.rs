/// CodeScene API client — mirrors Python's `codescene_api_client.py`.
///
/// Handles paginated API requests to the CodeScene platform.

use std::collections::HashMap;

use serde_json::Value;

use crate::errors::ApiError;
use crate::http::{HttpClient, HttpRequest, HttpResponse, Method, ReqwestClient};

/// Resolve the CodeScene API base URL.
pub fn get_api_url() -> String {
    if let Ok(url) = std::env::var("CS_ONPREM_URL") {
        format!("{url}/api")
    } else {
        "https://api.codescene.io".to_string()
    }
}

/// Make an authenticated GET request to the CodeScene API (production entry point).
pub async fn query_api(endpoint: &str) -> Result<Value, ApiError> {
    query_api_with_client(endpoint, &ReqwestClient).await
}

/// Make an authenticated GET request using an injectable HTTP client.
pub async fn query_api_with_client(
    endpoint: &str,
    client: &dyn HttpClient,
) -> Result<Value, ApiError> {
    let url = format!("{}/{}", get_api_url(), endpoint.trim_start_matches('/'));
    let token = std::env::var("CS_ACCESS_TOKEN").unwrap_or_default();

    let request = HttpRequest {
        method: Method::Get,
        url,
        headers: HashMap::from([
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Accept".to_string(), "application/json".to_string()),
        ]),
        body: None,
        timeout_secs: 30,
    };

    let resp = client
        .send(request)
        .await
        .map_err(|e| ApiError::Transport(e))?;

    parse_api_response(resp)
}

/// Parse an HTTP response into a JSON value or error.
fn parse_api_response(resp: HttpResponse) -> Result<Value, ApiError> {
    if !resp.is_success() {
        return Err(ApiError::Status {
            status: resp.status,
            body: resp.body,
        });
    }
    serde_json::from_str(&resp.body).map_err(|e| ApiError::Status {
        status: resp.status,
        body: format!("JSON parse error: {e}"),
    })
}

/// Make a paginated API query, collecting all pages (production entry point).
pub async fn query_api_list(endpoint: &str) -> Result<Vec<Value>, ApiError> {
    query_api_list_with_client(endpoint, &ReqwestClient).await
}

/// Make a paginated API query using an injectable HTTP client.
pub async fn query_api_list_with_client(
    endpoint: &str,
    client: &dyn HttpClient,
) -> Result<Vec<Value>, ApiError> {
    let mut results = Vec::new();
    let mut page = 1;

    loop {
        let sep = if endpoint.contains('?') { '&' } else { '?' };
        let paged = format!("{endpoint}{sep}page={page}");
        let data = query_api_with_client(&paged, client).await?;

        let items = match data.as_array() {
            Some(arr) => arr,
            None => return Ok(collect_with_single(results, data)),
        };
        if items.is_empty() {
            break;
        }
        results.extend(items.iter().cloned());
        page += 1;
    }

    Ok(results)
}

fn collect_with_single(mut results: Vec<Value>, item: Value) -> Vec<Value> {
    results.push(item);
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::MockHttpClient;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Lock env vars and set up standard API test environment.
    /// Returns a guard that must be held for the duration of the test.
    fn lock_api_env(token: &str) -> MutexGuard<'static, ()> {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_ONPREM_URL");
        std::env::set_var("CS_ACCESS_TOKEN", token);
        guard
    }

    fn cleanup_api_env() {
        std::env::remove_var("CS_ACCESS_TOKEN");
    }

    fn assert_status_error(err: ApiError, expected_status: u16) {
        match err {
            ApiError::Status { status, .. } => assert_eq!(status, expected_status),
            other => panic!("Expected ApiError::Status({expected_status}), got {other:?}"),
        }
    }

    // -- get_api_url --

    #[test]
    fn get_api_url_default() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_ONPREM_URL");
        assert_eq!(get_api_url(), "https://api.codescene.io");
    }

    #[test]
    fn get_api_url_onprem() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_ONPREM_URL", "https://my-instance.com");
        assert_eq!(get_api_url(), "https://my-instance.com/api");
        std::env::remove_var("CS_ONPREM_URL");
    }

    // -- parse_api_response --

    #[test]
    fn parse_api_response_success() {
        let result = parse_api_response(HttpResponse::ok(r#"{"key":"value"}"#)).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn parse_api_response_error_status() {
        let err = parse_api_response(HttpResponse::error(403, "Forbidden")).unwrap_err();
        assert_status_error(err, 403);
    }

    #[test]
    fn parse_api_response_invalid_json() {
        let err = parse_api_response(HttpResponse::ok("not-json")).unwrap_err();
        match err {
            ApiError::Status { status, body } => {
                assert_eq!(status, 200);
                assert!(body.contains("JSON parse error"));
            }
            other => panic!("Expected Status, got {other:?}"),
        }
    }

    // -- query_api_with_client --

    #[tokio::test]
    async fn query_api_success() {
        let _g = lock_api_env("test-token");
        let mock = MockHttpClient::always(HttpResponse::ok(r#"{"projects":[]}"#));
        let result = query_api_with_client("v2/projects", &mock).await.unwrap();
        assert_eq!(result["projects"], serde_json::json!([]));
        cleanup_api_env();
    }

    #[tokio::test]
    async fn query_api_sends_correct_headers_and_strips_leading_slash() {
        let _g = lock_api_env("my-token");
        let mock = MockHttpClient::always(HttpResponse::ok(r#"{}"#));
        let captured = mock.captured_requests.clone();

        // First request: normal endpoint
        let _ = query_api_with_client("v2/test", &mock).await;
        // Second request: leading-slash endpoint
        let _ = query_api_with_client("/v2/test", &mock).await;

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 2);
        // Normal endpoint
        assert_eq!(reqs[0].method, Method::Get);
        assert!(reqs[0].url.contains("v2/test"));
        assert_eq!(reqs[0].headers.get("Authorization").unwrap(), "Bearer my-token");
        // Leading slash should not produce double-slash
        assert!(reqs[1].url.ends_with("/v2/test"));
        assert!(!reqs[1].url.contains("//v2"));
        cleanup_api_env();
    }

    #[tokio::test]
    async fn query_api_error_status() {
        let _g = lock_api_env("tok");
        let mock = MockHttpClient::new(vec![HttpResponse::error(401, "Unauthorized")]);
        assert_status_error(query_api_with_client("v2/projects", &mock).await.unwrap_err(), 401);
        cleanup_api_env();
    }

    #[tokio::test]
    async fn query_api_transport_error() {
        let _g = lock_api_env("tok");
        let mock = MockHttpClient::new(vec![]);
        assert!(matches!(query_api_with_client("v2/projects", &mock).await.unwrap_err(), ApiError::Transport(_)));
        cleanup_api_env();
    }

    // -- query_api_list_with_client --

    async fn run_list_query(responses: Vec<HttpResponse>, endpoint: &str) -> Result<Vec<Value>, ApiError> {
        let mock = MockHttpClient::new(responses);
        query_api_list_with_client(endpoint, &mock).await
    }

    #[tokio::test]
    async fn query_api_list_paginates_correctly() {
        let _g = lock_api_env("tok");
        // Single page
        let items = run_list_query(
            vec![HttpResponse::ok(r#"[{"id":1},{"id":2}]"#), HttpResponse::ok(r#"[]"#)],
            "v2/projects",
        ).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], 1);

        // Multiple pages
        let items = run_list_query(
            vec![HttpResponse::ok(r#"[{"id":1}]"#), HttpResponse::ok(r#"[{"id":2}]"#), HttpResponse::ok(r#"[]"#)],
            "v2/items",
        ).await.unwrap();
        assert_eq!(items.len(), 2);
        cleanup_api_env();
    }

    #[tokio::test]
    async fn query_api_list_non_array_response() {
        let _g = lock_api_env("tok");
        let items = run_list_query(
            vec![HttpResponse::ok(r#"{"single":"object"}"#)],
            "v2/single",
        ).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["single"], "object");
        cleanup_api_env();
    }

    #[tokio::test]
    async fn query_api_list_appends_page_param_correctly() {
        let _g = lock_api_env("tok");
        let mock = MockHttpClient::new(vec![HttpResponse::ok(r#"[]"#)]);
        let captured = mock.captured_requests.clone();
        let _ = query_api_list_with_client("v2/items?filter=test", &mock).await;
        let reqs = captured.lock().unwrap();
        // Should use '&' since '?' already exists
        assert!(reqs[0].url.contains("filter=test&page=1"));
        cleanup_api_env();
    }

    #[tokio::test]
    async fn query_api_list_error_on_first_page() {
        let _g = lock_api_env("tok");
        assert!(run_list_query(vec![HttpResponse::error(500, "Server Error")], "v2/items").await.is_err());
        cleanup_api_env();
    }

    // -- collect_with_single --

    #[test]
    fn collect_with_single_appends_item() {
        let result = collect_with_single(vec![serde_json::json!(1)], serde_json::json!(2));
        assert_eq!(result.len(), 2);
        assert_eq!(result[1], 2);
    }

    #[test]
    fn collect_with_single_empty_vec() {
        let result = collect_with_single(vec![], serde_json::json!("only"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "only");
    }
}

use std::collections::HashMap;

use serde_json::Value;
use tokio::time::Duration;

use crate::errors::ApiError;
use crate::http::{HttpClient, HttpRequest, Method};

const ACE_API_URL: &str = "https://devtools.codescene.io/api/refactor";

const MAX_RETRIES: u32 = 3;

const RETRYABLE_CODES: &[u16] = &[408, 504];

pub async fn refactor_with_client(
    payload: &Value,
    client: &dyn HttpClient,
) -> Result<Value, ApiError> {
    let url = ace_url();
    let token = std::env::var("CS_ACE_ACCESS_TOKEN").unwrap_or_default();
    let body = serde_json::to_string(payload).unwrap_or_default();

    let mut last_error = None;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt))).await;
        }

        let request = HttpRequest {
            method: Method::Post,
            url: url.clone(),
            headers: HashMap::from([
                ("Authorization".to_string(), format!("Bearer {token}")),
                ("Content-Type".to_string(), "application/json".to_string()),
                ("Accept".to_string(), "application/json".to_string()),
            ]),
            body: Some(body.clone()),
            timeout_secs: 120,
        };

        let resp = client.send(request).await.map_err(|e| {
            ApiError::Transport(e)
        })?;

        if resp.is_success() {
            let parsed: Value = serde_json::from_str(&resp.body).map_err(|e| {
                ApiError::Status {
                    status: resp.status,
                    body: format!("JSON parse error: {e}"),
                }
            })?;
            return Ok(parsed);
        }

        if RETRYABLE_CODES.contains(&resp.status) {
            last_error = Some(ApiError::Status {
                status: resp.status,
                body: resp.body,
            });
            continue;
        }

        return Err(ApiError::Status {
            status: resp.status,
            body: resp.body,
        });
    }

    Err(last_error.unwrap_or(ApiError::Status {
        status: 500,
        body: "Max retries exceeded".to_string(),
    }))
}

fn ace_url() -> String {
    std::env::var("CS_ACE_API_URL").unwrap_or_else(|_| ACE_API_URL.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::http::tests::MockHttpClient;
    use crate::http::HttpResponse;
    use std::sync::MutexGuard;

    /// Lock env vars and set up standard ACE test environment.
    fn lock_ace_env(token: &str) -> MutexGuard<'static, ()> {
        let guard = config::lock_test_env();
        std::env::set_var("CS_ACE_ACCESS_TOKEN", token);
        std::env::remove_var("CS_ACE_API_URL");
        guard
    }

    fn cleanup_ace_env() {
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");
        std::env::remove_var("CS_ACE_API_URL");
    }

    /// Convenience: call refactor_with_client with a trivial JSON payload.
    async fn call_refactor(mock: &MockHttpClient) -> Result<Value, ApiError> {
        refactor_with_client(&serde_json::json!({}), mock).await
    }

    fn assert_status_error(err: ApiError, expected_status: u16, expected_body: &str) {
        match err {
            ApiError::Status { status, body } => {
                assert_eq!(status, expected_status);
                assert_eq!(body, expected_body);
            }
            other => panic!("Expected ApiError::Status({expected_status}), got: {other:?}"),
        }
    }


    #[test]
    fn ace_url_default() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ACE_API_URL");
        assert_eq!(ace_url(), ACE_API_URL);
    }

    #[test]
    fn ace_url_override() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_ACE_API_URL", "http://custom/api");
        assert_eq!(ace_url(), "http://custom/api");
        std::env::remove_var("CS_ACE_API_URL");
    }


    #[tokio::test]
    async fn refactor_success_returns_parsed_json() {
        let _g = lock_ace_env("test-token");
        let mock = MockHttpClient::new(vec![HttpResponse::ok(
            r#"{"code":"refactored","confidence":{"description":"high"}}"#,
        )]);

        let payload = serde_json::json!({"source-snippet": {"body": "fn foo() {}"}});
        let value = refactor_with_client(&payload, &mock).await.unwrap();
        assert_eq!(value["code"], "refactored");
        cleanup_ace_env();
    }


    #[tokio::test]
    async fn refactor_non_retryable_error_returns_immediately() {
        let _g = lock_ace_env("test-token");
        let mock = MockHttpClient::new(vec![HttpResponse::error(400, "Bad Request")]);
        assert_status_error(call_refactor(&mock).await.unwrap_err(), 400, "Bad Request");
        cleanup_ace_env();
    }


    #[tokio::test]
    async fn refactor_retries_on_408_then_succeeds() {
        let _g = lock_ace_env("test-token");
        let mock = MockHttpClient::new(vec![
            HttpResponse::error(408, "Timeout"),
            HttpResponse::ok(r#"{"code":"ok"}"#),
        ]);
        assert_eq!(call_refactor(&mock).await.unwrap()["code"], "ok");
        cleanup_ace_env();
    }

    #[tokio::test]
    async fn refactor_retries_on_504_then_succeeds() {
        let _g = lock_ace_env("test-token");
        let mock = MockHttpClient::new(vec![
            HttpResponse::error(504, "Gateway Timeout"),
            HttpResponse::ok(r#"{"result":"done"}"#),
        ]);
        assert!(call_refactor(&mock).await.is_ok());
        cleanup_ace_env();
    }


    #[tokio::test]
    async fn refactor_exhausts_retries_returns_last_error() {
        let _g = lock_ace_env("test-token");
        let mock = MockHttpClient::new(vec![
            HttpResponse::error(408, "Timeout 1"),
            HttpResponse::error(408, "Timeout 2"),
            HttpResponse::error(408, "Timeout 3"),
        ]);
        assert_status_error(call_refactor(&mock).await.unwrap_err(), 408, "Timeout 3");
        cleanup_ace_env();
    }


    #[tokio::test]
    async fn refactor_http_transport_error() {
        let _g = lock_ace_env("test-token");
        let mock = MockHttpClient::new(vec![]); // empty = transport error
        assert!(matches!(call_refactor(&mock).await.unwrap_err(), ApiError::Transport(_)));
        cleanup_ace_env();
    }


    #[tokio::test]
    async fn refactor_sends_correct_request() {
        let _g = lock_ace_env("my-ace-token");
        std::env::set_var("CS_ACE_API_URL", "http://test-ace/refactor");

        let mock = MockHttpClient::always(HttpResponse::ok(r#"{"code":"x"}"#));
        let captured = mock.captured_requests.clone();
        let _ = call_refactor(&mock).await;

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].method, Method::Post);
        assert_eq!(reqs[0].url, "http://test-ace/refactor");
        assert_eq!(reqs[0].headers.get("Authorization").unwrap(), "Bearer my-ace-token");
        assert_eq!(reqs[0].headers.get("Content-Type").unwrap(), "application/json");
        cleanup_ace_env();
    }


    #[tokio::test]
    async fn refactor_with_empty_token_sends_empty_bearer() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ACE_ACCESS_TOKEN");

        let mock = MockHttpClient::always(HttpResponse::ok(r#"{"code":"x"}"#));
        let captured = mock.captured_requests.clone();
        let _ = call_refactor(&mock).await;

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs[0].headers.get("Authorization").unwrap(), "Bearer ");
    }


    #[tokio::test]
    async fn refactor_invalid_json_response_returns_error() {
        let _g = lock_ace_env("test-token");
        let mock = MockHttpClient::new(vec![HttpResponse::ok("not-valid-json")]);
        let err = call_refactor(&mock).await.unwrap_err();
        match err {
            ApiError::Status { status, body } => {
                assert_eq!(status, 200);
                assert!(body.contains("JSON parse error"));
            }
            other => panic!("Expected ApiError::Status, got: {other:?}"),
        }
        cleanup_ace_env();
    }
}

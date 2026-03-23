use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Test-only convenience constructors for `HttpResponse`.
#[cfg(test)]
impl HttpResponse {
    pub fn ok(body: &str) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
        }
    }

    pub fn error(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    Get,
    Post,
}

/// Trait abstracting HTTP operations for dependency injection.
#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, String>;
}

pub struct ReqwestClient;

#[async_trait::async_trait]
impl HttpClient for ReqwestClient {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, String> {
        let client = reqwest::Client::new();
        let builder = match request.method {
            Method::Get => client.get(&request.url),
            Method::Post => client.post(&request.url),
        };

        let mut builder = builder.timeout(std::time::Duration::from_secs(request.timeout_secs));

        for (key, value) in &request.headers {
            builder = builder.header(key.as_str(), value.as_str());
        }

        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }

        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let body = resp.text().await.map_err(|e| e.to_string())?;

        Ok(HttpResponse { status, body })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A mock HTTP client that returns preconfigured responses.
    pub struct MockHttpClient {
        responses: Mutex<Vec<HttpResponse>>,
        pub captured_requests: Arc<Mutex<Vec<HttpRequest>>>,
    }

    impl MockHttpClient {
        /// Create a mock that returns responses in order (FIFO).
        pub fn new(responses: Vec<HttpResponse>) -> Self {
            Self {
                responses: Mutex::new(responses),
                captured_requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Create a mock that always returns the same response.
        pub fn always(response: HttpResponse) -> Self {
            // We'll create a large queue of identical responses
            Self::new(vec![response; 100])
        }
    }

    #[async_trait::async_trait]
    impl HttpClient for MockHttpClient {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse, String> {
            self.captured_requests.lock().unwrap().push(request);
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                return Err("MockHttpClient: no more responses".to_string());
            }
            Ok(responses.remove(0))
        }
    }

    #[test]
    fn http_response_ok_creates_200() {
        let resp = HttpResponse::ok(r#"{"key":"value"}"#);
        assert_eq!(resp.status, 200);
        assert!(resp.is_success());
    }

    #[test]
    fn http_response_error_creates_non_success() {
        let resp = HttpResponse::error(404, "Not Found");
        assert_eq!(resp.status, 404);
        assert!(!resp.is_success());
    }

    #[test]
    fn http_response_is_success_boundary() {
        assert!(HttpResponse {
            status: 200,
            body: String::new()
        }
        .is_success());
        assert!(HttpResponse {
            status: 299,
            body: String::new()
        }
        .is_success());
        assert!(!HttpResponse {
            status: 300,
            body: String::new()
        }
        .is_success());
        assert!(!HttpResponse {
            status: 199,
            body: String::new()
        }
        .is_success());
    }

    #[tokio::test]
    async fn mock_client_returns_responses_in_order() {
        let client = MockHttpClient::new(vec![
            HttpResponse::ok("first"),
            HttpResponse::error(500, "second"),
        ]);

        let req = HttpRequest {
            method: Method::Get,
            url: "http://example.com".to_string(),
            headers: HashMap::new(),
            body: None,
            timeout_secs: 10,
        };

        let r1 = client.send(req.clone()).await.unwrap();
        assert_eq!(r1.body, "first");
        assert_eq!(r1.status, 200);

        let r2 = client.send(req).await.unwrap();
        assert_eq!(r2.body, "second");
        assert_eq!(r2.status, 500);
    }

    #[tokio::test]
    async fn mock_client_captures_requests() {
        let client = MockHttpClient::always(HttpResponse::ok("ok"));

        let req = HttpRequest {
            method: Method::Post,
            url: "http://example.com/api".to_string(),
            headers: HashMap::from([("Authorization".to_string(), "Bearer tok".to_string())]),
            body: Some(r#"{"data":1}"#.to_string()),
            timeout_secs: 30,
        };

        let _ = client.send(req).await;
        let captured = client.captured_requests.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].method, Method::Post);
        assert_eq!(captured[0].url, "http://example.com/api");
    }
}

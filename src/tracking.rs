use std::collections::HashMap;

use serde_json::{json, Value};

use crate::http::{HttpClient, HttpRequest, Method, ReqwestClient};

struct TrackingEvent {
    url: String,
    event: String,
    instance_id: String,
    environment: String,
    version: &'static str,
    properties: Value,
    access_token: String,
}

/// Auth context for tracking events — pre-resolved token and API root.
pub(crate) struct TrackingAuth {
    pub(crate) access_token: String,
    pub(crate) api_root: Option<String>,
}

/// Send a tracking event in the background (fire-and-forget).
pub fn track_event(event: &str, properties: Value, instance_id: &str, auth: &TrackingAuth) {
    if is_disabled() {
        return;
    }

    let te = TrackingEvent {
        url: match resolve_tracking_url(auth.api_root.as_deref()) {
            Some(url) => url,
            None => return,
        },
        event: format!("mcp-{event}"),
        instance_id: instance_id.to_string(),
        environment: tracking_environment(),
        version: env!("CS_MCP_VERSION"),
        properties,
        access_token: auth.access_token.clone(),
    };

    tokio::spawn(async move {
        let _ = send_event(te, &ReqwestClient).await;
    });
}

/// Data needed to track a tool error event.
pub struct ErrorEvent<'a> {
    pub error_kind: &'a str,
    pub tool_name: &'a str,
    pub instance_id: &'a str,
    pub detail: Option<&'a str>,
    pub auth: &'a TrackingAuth,
}

pub fn track_error(evt: &ErrorEvent<'_>) {
    let mut properties = json!({
        "error": evt.error_kind,
        "tool": evt.tool_name,
    });
    if let Some(d) = evt.detail {
        properties["detail"] = json!(d);
    }
    track_event("error", properties, evt.instance_id, evt.auth);
}

fn build_tracking_body(te: &mut TrackingEvent) -> Value {
    if let Some(map) = te.properties.as_object_mut() {
        map.insert("instance-id".to_string(), json!(te.instance_id));
        map.insert("environment".to_string(), json!(te.environment));
        map.insert("version".to_string(), json!(te.version));
    }
    json!({
        "event-type": te.event,
        "event-properties": te.properties,
    })
}

async fn send_event(mut te: TrackingEvent, client: &dyn HttpClient) -> Result<(), String> {
    let body = build_tracking_body(&mut te);
    let mut headers = HashMap::from([
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "application/json".to_string()),
        (
            "User-Agent".to_string(),
            format!("codescene-mcp/{}", env!("CS_MCP_VERSION")),
        ),
        ("X-CS-Source".to_string(), "mcp".to_string()),
    ]);
    if !te.access_token.is_empty() {
        headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", te.access_token),
        );
    }

    let request = HttpRequest {
        method: Method::Post,
        url: te.url.clone(),
        headers,
        body: Some(serde_json::to_string(&body).unwrap_or_default()),
        timeout_secs: 10,
    };

    let _response = client.send(request).await?;
    Ok(())
}

/// Resolve the tracking endpoint URL.
///
/// Priority:
/// 1. `CS_TRACKING_URL` env var (explicit override).
/// 2. `api_root` parameter (from OAuth credential).
/// 3. `default_api_root()` (from `CS_ONPREM_URL` or cloud fallback).
fn resolve_tracking_url(api_root: Option<&str>) -> Option<String> {
    if let Ok(url) = std::env::var("CS_TRACKING_URL") {
        if let Err(e) = crate::config::require_https("CS_TRACKING_URL", &url) {
            tracing::warn!("{e}");
            return None;
        }
        return Some(normalize_tracking_override(&url));
    }

    let base = match api_root {
        Some(root) => root.trim_end_matches('/').to_string(),
        None => crate::auth::default_api_root().ok()?,
    };
    Some(format!("{}/v2/analytics/track", base.trim_end_matches('/')))
}

fn tracking_environment() -> String {
    std::env::var("CS_ENVIRONMENT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| crate::environment::detect().to_string())
}

fn normalize_tracking_override(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    if trimmed.ends_with("/v2/analytics/track") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v2/analytics/track")
    }
}

fn is_disabled() -> bool {
    flag_enabled("CS_DISABLE_TRACKING")
}

fn flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|v| !v.is_empty() && v != "0" && v.to_lowercase() != "false")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::http::tests::MockHttpClient;
    use crate::http::HttpResponse;
    use tokio::time::Duration;

    #[test]
    fn is_disabled_returns_false_when_not_set() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_DISABLE_TRACKING");
        assert!(!is_disabled());
    }

    #[test]
    fn is_disabled_returns_false_for_empty_string() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_DISABLE_TRACKING", "");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_returns_false_for_zero() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_DISABLE_TRACKING", "0");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_returns_false_for_false_any_case() {
        let _lock = config::lock_test_env();
        for val in ["false", "FALSE", "False"] {
            std::env::set_var("CS_DISABLE_TRACKING", val);
            assert!(!is_disabled(), "Expected not disabled for {val:?}");
        }
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_returns_true_for_truthy_values() {
        let _lock = config::lock_test_env();
        for val in ["true", "1", "yes"] {
            std::env::set_var("CS_DISABLE_TRACKING", val);
            assert!(is_disabled(), "Expected disabled for {val:?}");
        }
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn tracking_url_default() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_TRACKING_URL");
        std::env::remove_var("CS_ONPREM_URL");
        assert_eq!(
            resolve_tracking_url(None),
            Some("https://api.codescene.io/v2/analytics/track".to_string())
        );
    }

    #[test]
    fn tracking_url_resolution_cases() {
        let _lock = config::lock_test_env();

        // CS_TRACKING_URL override: HTTP blocked
        std::env::set_var("CS_TRACKING_URL", "http://custom-tracking");
        assert_eq!(resolve_tracking_url(None), None);

        // CS_TRACKING_URL override: HTTPS without full path appends path
        std::env::set_var("CS_TRACKING_URL", "https://custom-tracking");
        assert_eq!(
            resolve_tracking_url(None),
            Some("https://custom-tracking/v2/analytics/track".to_string())
        );

        // CS_TRACKING_URL override: full path preserved as-is
        std::env::set_var(
            "CS_TRACKING_URL",
            "https://custom-tracking/v2/analytics/track",
        );
        assert_eq!(
            resolve_tracking_url(None),
            Some("https://custom-tracking/v2/analytics/track".to_string())
        );
        std::env::remove_var("CS_TRACKING_URL");

        // CS_ONPREM_URL fallback: derives from onprem + /api
        std::env::set_var("CS_ONPREM_URL", "https://my-instance.example.com");
        assert_eq!(
            resolve_tracking_url(None),
            Some("https://my-instance.example.com/api/v2/analytics/track".to_string())
        );

        // CS_ONPREM_URL: HTTP blocked
        std::env::set_var("CS_ONPREM_URL", "http://my-instance.example.com");
        assert_eq!(resolve_tracking_url(None), None);
        std::env::remove_var("CS_ONPREM_URL");
    }

    #[test]
    fn tracking_url_from_oauth_api_root() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_TRACKING_URL");
        std::env::remove_var("CS_ONPREM_URL");
        assert_eq!(
            resolve_tracking_url(Some("https://oauth-host.example.com/api")),
            Some("https://oauth-host.example.com/api/v2/analytics/track".to_string())
        );
        assert_eq!(
            resolve_tracking_url(Some("https://api.codescene.io")),
            Some("https://api.codescene.io/v2/analytics/track".to_string())
        );
    }

    #[test]
    fn tracking_url_resolution_allows_host_docker_internal() {
        // Regression test: e2e tests running under the Docker backend point
        // CS_TRACKING_URL at `http://host.docker.internal:<port>` so the
        // container can reach the fake tracking server on the host. This
        // must not be blocked by the HTTPS requirement.
        let _lock = config::lock_test_env();
        std::env::set_var("CS_TRACKING_URL", "http://host.docker.internal:12345");
        assert_eq!(
            resolve_tracking_url(None),
            Some("http://host.docker.internal:12345/v2/analytics/track".to_string())
        );
        std::env::remove_var("CS_TRACKING_URL");
    }

    #[test]
    fn tracking_environment_uses_detected_environment_when_unset() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ENVIRONMENT");
        assert_eq!(
            tracking_environment(),
            crate::environment::detect().to_string()
        );
    }

    #[test]
    fn tracking_environment_uses_override_when_set() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_ENVIRONMENT", "my-agent-name");
        assert_eq!(tracking_environment(), "my-agent-name");
        std::env::remove_var("CS_ENVIRONMENT");
    }

    #[test]
    fn tracking_environment_ignores_blank_override() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_ENVIRONMENT", "   ");
        assert_eq!(
            tracking_environment(),
            crate::environment::detect().to_string()
        );
        std::env::remove_var("CS_ENVIRONMENT");
    }

    #[test]
    fn build_tracking_body_enriches_properties() {
        let mut te = TrackingEvent {
            url: "http://test/track".to_string(),
            event: "mcp-test".to_string(),
            instance_id: "inst-123".to_string(),
            environment: "test-env".to_string(),
            version: "1.0.0",
            properties: json!({"tool": "review"}),
            access_token: String::new(),
        };
        let body = build_tracking_body(&mut te);
        assert_eq!(body["event-type"], "mcp-test");

        let props = &body["event-properties"];
        assert_eq!(props["tool"], "review");
        assert_eq!(props["instance-id"], "inst-123");
        assert_eq!(props["environment"], "test-env");
        assert_eq!(props["version"], "1.0.0");
    }

    #[test]
    fn build_tracking_body_handles_non_object_properties() {
        let mut te = TrackingEvent {
            url: "http://test/track".to_string(),
            event: "mcp-evt".to_string(),
            instance_id: "id".to_string(),
            environment: "env".to_string(),
            version: "1.0.0",
            properties: json!("not-an-object"),
            access_token: String::new(),
        };
        let body = build_tracking_body(&mut te);
        // Properties stay as-is when not an object
        assert_eq!(body["event-properties"], "not-an-object");
    }

    async fn send_event_and_capture_request(
        _token: Option<&str>,
        te: TrackingEvent,
    ) -> crate::http::HttpRequest {
        let mock = MockHttpClient::always(HttpResponse::ok(""));
        let captured = mock.captured_requests.clone();

        let result = send_event(te, &mock).await;
        assert!(result.is_ok());

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        reqs[0].clone()
    }

    fn assert_standard_headers(req: &HttpRequest) {
        assert_eq!(req.headers.get("Accept").unwrap(), "application/json");
        assert!(req
            .headers
            .get("User-Agent")
            .is_some_and(|v| v.starts_with("codescene-mcp/")));
    }

    #[tokio::test]
    async fn send_event_posts_to_correct_url() {
        let _lock = config::lock_test_env();
        let te = TrackingEvent {
            url: "http://track.test/v2/analytics/track".to_string(),
            event: "mcp-review".to_string(),
            instance_id: "i1".to_string(),
            environment: "test".to_string(),
            version: "1.0.0",
            properties: json!({"key": "val"}),
            access_token: "test-tok".to_string(),
        };
        let req = send_event_and_capture_request(Some("test-tok"), te).await;

        assert_eq!(req.url, "http://track.test/v2/analytics/track");
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.headers.get("Authorization").unwrap(), "Bearer test-tok");
        assert_standard_headers(&req);
    }

    #[tokio::test]
    async fn send_event_omits_authorization_when_no_token() {
        let _lock = config::lock_test_env();
        let te = TrackingEvent {
            url: "http://t/track".to_string(),
            event: "mcp-e".to_string(),
            instance_id: "i".to_string(),
            environment: "e".to_string(),
            version: "1.0.0",
            properties: json!({}),
            access_token: String::new(),
        };
        let req = send_event_and_capture_request(None, te).await;

        assert!(req.headers.get("Authorization").is_none());
        assert_eq!(req.headers.get("Content-Type").unwrap(), "application/json");
        assert_standard_headers(&req);
    }

    #[tokio::test]
    async fn send_event_serializes_body_with_event_type() {
        let _lock = config::lock_test_env();

        let mock = MockHttpClient::always(HttpResponse::ok(""));
        let captured = mock.captured_requests.clone();

        let te = TrackingEvent {
            url: "http://t/track".to_string(),
            event: "mcp-test-event".to_string(),
            instance_id: "inst".to_string(),
            environment: "env".to_string(),
            version: "2.0.0",
            properties: json!({"tool": "score"}),
            access_token: String::new(),
        };
        let _ = send_event(te, &mock).await;

        let reqs = captured.lock().unwrap();
        let body: Value = serde_json::from_str(reqs[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["event-type"], "mcp-test-event");
        assert_eq!(body["event-properties"]["tool"], "score");
        assert_eq!(body["event-properties"]["instance-id"], "inst");
    }

    #[tokio::test]
    async fn send_event_returns_error_on_transport_failure() {
        let _lock = config::lock_test_env();

        let mock = MockHttpClient::new(vec![]);

        let te = TrackingEvent {
            url: "http://t/track".to_string(),
            event: "mcp-e".to_string(),
            instance_id: "i".to_string(),
            environment: "e".to_string(),
            version: "1.0.0",
            properties: json!({}),
            access_token: String::new(),
        };
        let result = send_event(te, &mock).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn track_event_disabled_does_not_panic() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        let auth = TrackingAuth {
            access_token: String::new(),
            api_root: None,
        };
        track_event(
            "test-event",
            json!({"key": "value"}),
            "test-instance",
            &auth,
        );
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[tokio::test]
    async fn track_error_disabled_does_not_panic() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        let auth = TrackingAuth {
            access_token: String::new(),
            api_root: None,
        };
        track_error(&ErrorEvent {
            error_kind: "some error",
            tool_name: "some-tool",
            instance_id: "test-instance",
            detail: None,
            auth: &auth,
        });
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    async fn run_with_tracking_enabled(f: impl FnOnce()) {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_DISABLE_TRACKING");
        std::env::set_var("CS_TRACKING_URL", "http://192.0.2.1:1/track");
        f();
        tokio::time::sleep(Duration::from_millis(50)).await;
        std::env::remove_var("CS_TRACKING_URL");
    }

    #[tokio::test]
    async fn track_event_enabled_spawns_without_panic() {
        run_with_tracking_enabled(|| {
            let auth = TrackingAuth {
                access_token: String::new(),
                api_root: None,
            };
            track_event("test-enabled", json!({"key": "val"}), "test-id", &auth);
        })
        .await;
    }

    #[tokio::test]
    async fn track_error_enabled_spawns_without_panic() {
        run_with_tracking_enabled(|| {
            let auth = TrackingAuth {
                access_token: String::new(),
                api_root: None,
            };
            track_error(&ErrorEvent {
                error_kind: "err msg",
                tool_name: "tool-name",
                instance_id: "test-id",
                detail: Some(".txt"),
                auth: &auth,
            });
        })
        .await;
    }
}

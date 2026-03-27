use std::collections::HashMap;

use serde_json::{json, Value};

use crate::http::{HttpClient, HttpRequest, Method, ReqwestClient};

const DEFAULT_API_URL: &str = "https://api.codescene.io";

struct TrackingEvent {
    url: String,
    event: String,
    instance_id: String,
    environment: String,
    version: &'static str,
    properties: Value,
}

/// Send a tracking event in the background (fire-and-forget).
pub fn track_event(event: &str, properties: Value, instance_id: &str) {
    if is_disabled() {
        return;
    }

    let te = TrackingEvent {
        url: tracking_url(),
        event: format!("mcp-{event}"),
        instance_id: instance_id.to_string(),
        environment: crate::environment::detect().to_string(),
        version: env!("CS_MCP_VERSION"),
        properties,
    };

    tokio::spawn(async move {
        let _ = send_event(te, &ReqwestClient).await;
    });
}

pub fn track_error(error_msg: &str, tool_name: &str, instance_id: &str) {
    let properties = json!({
        "error": error_msg,
        "tool": tool_name,
    });
    track_event("error", properties, instance_id);
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
    let token = std::env::var("CS_ACCESS_TOKEN").unwrap_or_default();
    let mut headers = HashMap::from([
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Accept".to_string(), "application/json".to_string()),
        (
            "User-Agent".to_string(),
            format!("codescene-mcp/{}", env!("CS_MCP_VERSION")),
        ),
    ]);
    if !token.is_empty() {
        headers.insert("Authorization".to_string(), format!("Bearer {token}"));
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

fn tracking_url() -> String {
    if let Ok(url) = std::env::var("CS_TRACKING_URL") {
        return normalize_tracking_override(&url);
    }

    let api_url = std::env::var("CS_ONPREM_URL")
        .map(|u| format!("{u}/api"))
        .unwrap_or_else(|_| DEFAULT_API_URL.to_string());

    format!("{api_url}/v2/analytics/track")
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
            tracking_url(),
            "https://api.codescene.io/v2/analytics/track"
        );
    }

    #[test]
    fn tracking_url_override() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_TRACKING_URL", "http://custom-tracking");
        assert_eq!(tracking_url(), "http://custom-tracking/v2/analytics/track");
        std::env::remove_var("CS_TRACKING_URL");
    }

    #[test]
    fn tracking_url_override_with_full_path_keeps_path() {
        let _lock = config::lock_test_env();
        std::env::set_var(
            "CS_TRACKING_URL",
            "http://custom-tracking/v2/analytics/track",
        );
        assert_eq!(tracking_url(), "http://custom-tracking/v2/analytics/track");
        std::env::remove_var("CS_TRACKING_URL");
    }

    #[test]
    fn tracking_url_from_onprem() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_TRACKING_URL");
        std::env::set_var("CS_ONPREM_URL", "https://my-instance.example.com");
        assert_eq!(
            tracking_url(),
            "https://my-instance.example.com/api/v2/analytics/track"
        );
        std::env::remove_var("CS_ONPREM_URL");
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
        };
        let body = build_tracking_body(&mut te);
        // Properties stay as-is when not an object
        assert_eq!(body["event-properties"], "not-an-object");
    }

    async fn send_event_and_capture_request(
        token: Option<&str>,
        te: TrackingEvent,
    ) -> crate::http::HttpRequest {
        match token {
            Some(value) => std::env::set_var("CS_ACCESS_TOKEN", value),
            None => std::env::remove_var("CS_ACCESS_TOKEN"),
        }

        let mock = MockHttpClient::always(HttpResponse::ok(""));
        let captured = mock.captured_requests.clone();

        let result = send_event(te, &mock).await;
        assert!(result.is_ok());

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        reqs[0].clone()
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
        };
        let req = send_event_and_capture_request(Some("test-tok"), te).await;

        assert_eq!(req.url, "http://track.test/v2/analytics/track");
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.headers.get("Authorization").unwrap(), "Bearer test-tok");
        assert_eq!(req.headers.get("Accept").unwrap(), "application/json");
        assert!(req
            .headers
            .get("User-Agent")
            .is_some_and(|v| v.starts_with("codescene-mcp/")));

        std::env::remove_var("CS_ACCESS_TOKEN");
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
        };
        let req = send_event_and_capture_request(None, te).await;

        assert!(req.headers.get("Authorization").is_none());
        assert_eq!(req.headers.get("Content-Type").unwrap(), "application/json");
        assert_eq!(req.headers.get("Accept").unwrap(), "application/json");
        assert!(req
            .headers
            .get("User-Agent")
            .is_some_and(|v| v.starts_with("codescene-mcp/")));
    }

    #[tokio::test]
    async fn send_event_serializes_body_with_event_type() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_ACCESS_TOKEN");

        let mock = MockHttpClient::always(HttpResponse::ok(""));
        let captured = mock.captured_requests.clone();

        let te = TrackingEvent {
            url: "http://t/track".to_string(),
            event: "mcp-test-event".to_string(),
            instance_id: "inst".to_string(),
            environment: "env".to_string(),
            version: "2.0.0",
            properties: json!({"tool": "score"}),
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
        std::env::remove_var("CS_ACCESS_TOKEN");

        let mock = MockHttpClient::new(vec![]);

        let te = TrackingEvent {
            url: "http://t/track".to_string(),
            event: "mcp-e".to_string(),
            instance_id: "i".to_string(),
            environment: "e".to_string(),
            version: "1.0.0",
            properties: json!({}),
        };
        let result = send_event(te, &mock).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn track_event_disabled_does_not_panic() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        track_event("test-event", json!({"key": "value"}), "test-instance");
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[tokio::test]
    async fn track_error_disabled_does_not_panic() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        track_error("some error", "some-tool", "test-instance");
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[tokio::test]
    async fn track_event_enabled_spawns_without_panic() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_DISABLE_TRACKING");
        std::env::set_var("CS_TRACKING_URL", "http://192.0.2.1:1/track");
        track_event("test-enabled", json!({"key": "val"}), "test-id");
        tokio::time::sleep(Duration::from_millis(50)).await;
        std::env::remove_var("CS_TRACKING_URL");
    }

    #[tokio::test]
    async fn track_error_enabled_spawns_without_panic() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_DISABLE_TRACKING");
        std::env::set_var("CS_TRACKING_URL", "http://192.0.2.1:1/track");
        track_error("err msg", "tool-name", "test-id");
        tokio::time::sleep(Duration::from_millis(50)).await;
        std::env::remove_var("CS_TRACKING_URL");
    }
}

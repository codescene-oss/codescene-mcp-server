/// Analytics tracking — mirrors Python's `track.py`.
///
/// POSTs usage events to the CodeScene analytics endpoint in background tasks.
/// Events are prefixed with `mcp-`. Disabled by `CS_DISABLE_TRACKING`.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::http::{HttpClient, HttpRequest, Method, ReqwestClient};

/// Default tracking URL (derived from API URL).
const DEFAULT_API_URL: &str = "https://api.codescene.io";

/// All data needed to send a single tracking event.
struct TrackingEvent {
    url: String,
    event: String,
    instance_id: String,
    environment: String,
    version: &'static str,
    properties: Value,
}

/// Send a tracking event in the background. Non-blocking, fire-and-forget.
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

/// Send an error tracking event.
pub fn track_error(error_msg: &str, tool_name: &str, instance_id: &str) {
    let properties = json!({
        "error": error_msg,
        "tool": tool_name,
    });
    track_event("error", properties, instance_id);
}

/// Build the JSON body for a tracking event, enriching properties.
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

async fn send_event(
    mut te: TrackingEvent,
    client: &dyn HttpClient,
) -> Result<(), String> {
    let body = build_tracking_body(&mut te);
    let token = std::env::var("CS_ACCESS_TOKEN").unwrap_or_default();

    let request = HttpRequest {
        method: Method::Post,
        url: te.url.clone(),
        headers: HashMap::from([
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ]),
        body: Some(serde_json::to_string(&body).unwrap_or_default()),
        timeout_secs: 10,
    };

    client.send(request).await?;
    Ok(())
}

fn tracking_url() -> String {
    if let Ok(url) = std::env::var("CS_TRACKING_URL") {
        return url;
    }

    let api_url = std::env::var("CS_ONPREM_URL")
        .map(|u| format!("{u}/api"))
        .unwrap_or_else(|_| DEFAULT_API_URL.to_string());

    format!("{api_url}/v2/analytics/track")
}

fn is_disabled() -> bool {
    std::env::var("CS_DISABLE_TRACKING")
        .map(|v| !v.is_empty() && v != "0" && v.to_lowercase() != "false")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::MockHttpClient;
    use crate::http::HttpResponse;
    use std::sync::Mutex;
    use tokio::time::Duration;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // -- is_disabled --

    #[test]
    fn is_disabled_returns_false_when_not_set() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_DISABLE_TRACKING");
        assert!(!is_disabled());
    }

    #[test]
    fn is_disabled_returns_false_for_empty_string() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_returns_false_for_zero() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "0");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_returns_false_for_false_any_case() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for val in ["false", "FALSE", "False"] {
            std::env::set_var("CS_DISABLE_TRACKING", val);
            assert!(!is_disabled(), "Expected not disabled for {val:?}");
        }
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_returns_true_for_truthy_values() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for val in ["true", "1", "yes"] {
            std::env::set_var("CS_DISABLE_TRACKING", val);
            assert!(is_disabled(), "Expected disabled for {val:?}");
        }
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    // -- tracking_url --

    #[test]
    fn tracking_url_default() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_TRACKING_URL");
        std::env::remove_var("CS_ONPREM_URL");
        assert_eq!(tracking_url(), "https://api.codescene.io/v2/analytics/track");
    }

    #[test]
    fn tracking_url_override() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_TRACKING_URL", "http://custom-tracking/track");
        assert_eq!(tracking_url(), "http://custom-tracking/track");
        std::env::remove_var("CS_TRACKING_URL");
    }

    #[test]
    fn tracking_url_from_onprem() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_TRACKING_URL");
        std::env::set_var("CS_ONPREM_URL", "https://my-instance.example.com");
        assert_eq!(
            tracking_url(),
            "https://my-instance.example.com/api/v2/analytics/track"
        );
        std::env::remove_var("CS_ONPREM_URL");
    }

    // -- build_tracking_body --

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

    // -- send_event --

    #[tokio::test]
    async fn send_event_posts_to_correct_url() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_ACCESS_TOKEN", "test-tok");

        let mock = MockHttpClient::always(HttpResponse::ok(""));
        let captured = mock.captured_requests.clone();

        let te = TrackingEvent {
            url: "http://track.test/v2/analytics/track".to_string(),
            event: "mcp-review".to_string(),
            instance_id: "i1".to_string(),
            environment: "test".to_string(),
            version: "1.0.0",
            properties: json!({"key": "val"}),
        };
        let result = send_event(te, &mock).await;
        assert!(result.is_ok());

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "http://track.test/v2/analytics/track");
        assert_eq!(reqs[0].method, Method::Post);
        assert_eq!(reqs[0].headers.get("Authorization").unwrap(), "Bearer test-tok");

        std::env::remove_var("CS_ACCESS_TOKEN");
    }

    #[tokio::test]
    async fn send_event_uses_empty_bearer_when_no_token() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_ACCESS_TOKEN");

        let mock = MockHttpClient::always(HttpResponse::ok(""));
        let captured = mock.captured_requests.clone();

        let te = TrackingEvent {
            url: "http://t/track".to_string(),
            event: "mcp-e".to_string(),
            instance_id: "i".to_string(),
            environment: "e".to_string(),
            version: "1.0.0",
            properties: json!({}),
        };
        let _ = send_event(te, &mock).await;

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs[0].headers.get("Authorization").unwrap(), "Bearer ");
    }

    #[tokio::test]
    async fn send_event_serializes_body_with_event_type() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

    // -- track_event / track_error with tracking disabled --

    #[tokio::test]
    async fn track_event_disabled_does_not_panic() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        track_event("test-event", json!({"key": "value"}), "test-instance");
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[tokio::test]
    async fn track_error_disabled_does_not_panic() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        track_error("some error", "some-tool", "test-instance");
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    // -- track_event / track_error with tracking enabled (fire-and-forget) --

    #[tokio::test]
    async fn track_event_enabled_spawns_without_panic() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_DISABLE_TRACKING");
        std::env::set_var("CS_TRACKING_URL", "http://192.0.2.1:1/track");
        track_event("test-enabled", json!({"key": "val"}), "test-id");
        tokio::time::sleep(Duration::from_millis(50)).await;
        std::env::remove_var("CS_TRACKING_URL");
    }

    #[tokio::test]
    async fn track_error_enabled_spawns_without_panic() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_DISABLE_TRACKING");
        std::env::set_var("CS_TRACKING_URL", "http://192.0.2.1:1/track");
        track_error("err msg", "tool-name", "test-id");
        tokio::time::sleep(Duration::from_millis(50)).await;
        std::env::remove_var("CS_TRACKING_URL");
    }
}

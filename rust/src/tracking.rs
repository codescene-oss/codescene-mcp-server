/// Analytics tracking — mirrors Python's `track.py`.
///
/// POSTs usage events to the CodeScene analytics endpoint in background tasks.
/// Events are prefixed with `mcp-`. Disabled by `CS_DISABLE_TRACKING`.

use serde_json::{json, Value};
use tokio::time::Duration;

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
        let _ = send_event(te).await;
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

async fn send_event(mut te: TrackingEvent) -> Result<(), reqwest::Error> {
    if let Some(map) = te.properties.as_object_mut() {
        map.insert("instance-id".to_string(), json!(te.instance_id));
        map.insert("environment".to_string(), json!(te.environment));
        map.insert("version".to_string(), json!(te.version));
    }

    let body = json!({
        "event-type": te.event,
        "event-properties": te.properties,
    });

    let client = reqwest::Client::new();
    let token = std::env::var("CS_ACCESS_TOKEN").unwrap_or_default();

    client
        .post(&te.url)
        .header("Authorization", format!("Bearer {token}"))
        .json(&body)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

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
    use std::sync::Mutex;

    // Serialize env-var mutations so parallel tests don't race.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // -- is_disabled --

    #[test]
    fn is_disabled_when_not_set() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_DISABLE_TRACKING");
        assert!(!is_disabled());
    }

    #[test]
    fn is_disabled_when_empty() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_when_zero() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "0");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_when_false_lowercase() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "false");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_when_false_uppercase() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "FALSE");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_when_true() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "true");
        assert!(is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_when_one() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        assert!(is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    #[test]
    fn is_disabled_when_arbitrary_string() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "yes");
        assert!(is_disabled());
        std::env::remove_var("CS_DISABLE_TRACKING");
    }

    // -- tracking_url --

    #[test]
    fn tracking_url_default() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_TRACKING_URL");
        std::env::remove_var("CS_ONPREM_URL");
        let url = tracking_url();
        assert_eq!(url, "https://api.codescene.io/v2/analytics/track");
    }

    #[test]
    fn tracking_url_override() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_TRACKING_URL", "http://custom-tracking/track");
        let url = tracking_url();
        assert_eq!(url, "http://custom-tracking/track");
        std::env::remove_var("CS_TRACKING_URL");
    }

    #[test]
    fn tracking_url_from_onprem() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("CS_TRACKING_URL");
        std::env::set_var("CS_ONPREM_URL", "https://my-instance.example.com");
        let url = tracking_url();
        assert_eq!(
            url,
            "https://my-instance.example.com/api/v2/analytics/track"
        );
        std::env::remove_var("CS_ONPREM_URL");
    }

    // -- track_event / track_error with tracking disabled --

    #[tokio::test]
    async fn track_event_disabled_does_not_panic() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        // Should silently return without spawning anything
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
        // Point to a non-routable URL so the HTTP request fails silently
        std::env::set_var("CS_TRACKING_URL", "http://192.0.2.1:1/track");
        track_event("test-enabled", json!({"key": "val"}), "test-id");
        // Give the spawned task a moment to start
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

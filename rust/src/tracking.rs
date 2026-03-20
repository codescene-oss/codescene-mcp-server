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

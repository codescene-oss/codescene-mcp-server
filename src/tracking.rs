use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::analytics_attribution::{merge_attribution, resolve_attribution, AnalyticsContext};
use crate::auth::AuthCredential;
use crate::http::{HttpClient, HttpRequest, Method, ReqwestClient};
use crate::repository_projects::RepositoryProjectsCache;

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

pub(crate) struct TrackingAttribution {
    pub(crate) context: AnalyticsContext,
    pub(crate) credential: Option<AuthCredential>,
    pub(crate) http_client: Arc<dyn HttpClient>,
    pub(crate) cache: Arc<RepositoryProjectsCache>,
}

pub(crate) struct AttributedEvent<'a> {
    pub(crate) event: &'a str,
    pub(crate) properties: Value,
    pub(crate) instance_id: &'a str,
    pub(crate) auth: &'a TrackingAuth,
    pub(crate) attribution: TrackingAttribution,
}

pub(crate) fn track_event_with_attribution(event: AttributedEvent<'_>) {
    if is_disabled() {
        return;
    }
    let Some(tracking_event) =
        create_tracking_event(event.event, event.properties, event.instance_id, event.auth)
    else {
        return;
    };
    spawn_tracking_event(tracking_event, Some(event.attribution));
}

fn spawn_tracking_event(tracking_event: TrackingEvent, attribution: Option<TrackingAttribution>) {
    tokio::spawn(async move {
        let _ = process_tracking_event(tracking_event, attribution, &ReqwestClient).await;
    });
}

async fn process_tracking_event(
    mut tracking_event: TrackingEvent,
    attribution: Option<TrackingAttribution>,
    client: &dyn HttpClient,
) -> Result<(), String> {
    if let Some(attribution) = attribution {
        enrich_tracking_event(&mut tracking_event, attribution).await;
    }
    send_event(tracking_event, client).await
}

/// Data needed to track a tool error event.
pub struct ErrorEvent<'a> {
    pub error_kind: &'a str,
    pub tool_name: &'a str,
    pub instance_id: &'a str,
    pub detail: Option<&'a str>,
    pub auth: &'a TrackingAuth,
}

pub(crate) fn track_error_with_attribution(evt: &ErrorEvent<'_>, attribution: TrackingAttribution) {
    track_error_properties(evt, |properties| {
        track_event_with_attribution(AttributedEvent {
            event: "error",
            properties,
            instance_id: evt.instance_id,
            auth: evt.auth,
            attribution,
        })
    });
}

fn track_error_properties(evt: &ErrorEvent<'_>, track: impl FnOnce(Value)) {
    let mut properties = json!({
        "error": evt.error_kind,
        "tool": evt.tool_name,
    });
    if let Some(d) = evt.detail {
        properties["detail"] = json!(d);
    }
    track(properties);
}

fn create_tracking_event(
    event: &str,
    properties: Value,
    instance_id: &str,
    auth: &TrackingAuth,
) -> Option<TrackingEvent> {
    Some(TrackingEvent {
        url: resolve_tracking_url(auth.api_root.as_deref())?,
        event: format!("mcp-{event}"),
        instance_id: instance_id.to_string(),
        environment: tracking_environment(),
        version: env!("CS_MCP_VERSION"),
        properties,
        access_token: auth.access_token.clone(),
    })
}

async fn enrich_tracking_event(event: &mut TrackingEvent, attribution: TrackingAttribution) {
    let outcome = resolve_attribution(
        attribution.context,
        attribution.credential,
        attribution.http_client,
        attribution.cache,
    )
    .await;
    merge_attribution(&mut event.properties, outcome);
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

pub(crate) fn is_disabled() -> bool {
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
    use sha2::{Digest, Sha256};
    use std::path::Path;
    use std::process::Command;
    use tokio::sync::Notify;
    use tokio::time::{timeout, Duration};

    const ACCESS_TOKEN: &str = "sensitive-access-token";
    const REMOTE_NAME: &str = "sensitive-remote-name";
    const REMOTE_URL: &str =
        "https://sensitive-user:sensitive-password@example.test/private/repository.git";
    const CANONICAL_REPOSITORY_ID: &str = "example.test/private/repository";

    struct FailingHttpClient {
        error: &'static str,
    }

    #[async_trait::async_trait]
    impl HttpClient for FailingHttpClient {
        async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, String> {
            Err(self.error.to_string())
        }
    }

    struct BlockingHttpClient {
        started: Arc<Notify>,
        release: Arc<Notify>,
    }

    #[async_trait::async_trait]
    impl HttpClient for BlockingHttpClient {
        async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, String> {
            self.started.notify_one();
            self.release.notified().await;
            Err("released mapping request".to_string())
        }
    }

    fn test_attribution(client: Arc<dyn HttpClient>) -> TrackingAttribution {
        TrackingAttribution {
            context: AnalyticsContext::ExplicitProjectIds(vec![42]),
            credential: None,
            http_client: client,
            cache: Arc::new(RepositoryProjectsCache::default()),
        }
    }

    fn git(working_dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(working_dir)
            .output()
            .expect("Git should execute");
        assert!(
            output.status.success(),
            "Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn repository_with_sensitive_remote() -> tempfile::TempDir {
        let repository = tempfile::tempdir().unwrap();
        git(repository.path(), &["init", "--quiet"]);
        git(
            repository.path(),
            &["remote", "add", REMOTE_NAME, REMOTE_URL],
        );
        repository
    }

    fn path_attribution(path: &Path, client: Arc<dyn HttpClient>) -> TrackingAttribution {
        TrackingAttribution {
            context: AnalyticsContext::Path(path.to_path_buf()),
            credential: Some(AuthCredential::Configured {
                access_token: ACCESS_TOKEN.to_string(),
                onprem_url: None,
            }),
            http_client: client,
            cache: Arc::new(RepositoryProjectsCache::default()),
        }
    }

    fn tracking_event(event: &str, properties: Value) -> TrackingEvent {
        TrackingEvent {
            url: "http://tracking.test/v2/analytics/track".to_string(),
            event: event.to_string(),
            instance_id: "test-instance".to_string(),
            environment: "test-environment".to_string(),
            version: "1.0.0",
            properties,
            access_token: ACCESS_TOKEN.to_string(),
        }
    }

    async fn process_failed_enrichment(
        event: TrackingEvent,
        attribution: TrackingAttribution,
    ) -> Value {
        let delivery_client = MockHttpClient::always(HttpResponse::ok(""));
        let requests = delivery_client.captured_requests.clone();

        process_tracking_event(event, Some(attribution), &delivery_client)
            .await
            .unwrap();

        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap()
    }

    fn assert_private_attribution_data_absent(body: &Value, action_path: &Path, raw_error: &str) {
        let body = serde_json::to_string(body).unwrap();
        let action_path = action_path.to_string_lossy();
        let token_fingerprint = hex::encode(Sha256::digest(ACCESS_TOKEN.as_bytes()));
        for sensitive_value in [
            action_path.as_ref(),
            REMOTE_NAME,
            REMOTE_URL,
            "sensitive-user",
            "sensitive-password",
            CANONICAL_REPOSITORY_ID,
            ACCESS_TOKEN,
            token_fingerprint.as_str(),
            raw_error,
        ] {
            assert!(
                !body.contains(sensitive_value),
                "tracking properties exposed {sensitive_value:?}: {body}"
            );
        }
    }

    #[derive(Clone, Copy)]
    enum TestTrackingCall {
        Event,
        Error,
    }

    fn run_test_tracking_call(call: TestTrackingCall, client: Arc<dyn HttpClient>) {
        let auth = TrackingAuth {
            access_token: String::new(),
            api_root: None,
        };
        match call {
            TestTrackingCall::Event => track_event_with_attribution(AttributedEvent {
                event: "test-event",
                properties: json!({"key": "value"}),
                instance_id: "test-instance",
                auth: &auth,
                attribution: test_attribution(client),
            }),
            TestTrackingCall::Error => track_error_with_attribution(
                &ErrorEvent {
                    error_kind: "some-error",
                    tool_name: "some-tool",
                    instance_id: "test-instance",
                    detail: None,
                    auth: &auth,
                },
                test_attribution(client),
            ),
        }
    }

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

    #[tokio::test]
    async fn detached_enrichment_merges_attribution_before_delivery() {
        let mut event = TrackingEvent {
            url: "http://test/track".to_string(),
            event: "mcp-test".to_string(),
            instance_id: "inst-123".to_string(),
            environment: "test-env".to_string(),
            version: "1.0.0",
            properties: json!({"tool": "review"}),
            access_token: String::new(),
        };
        let client = crate::http::tests::MockHttpClient::new(Vec::new());
        let attribution = TrackingAttribution {
            context: AnalyticsContext::ExplicitProjectIds(vec![42, 7, 42]),
            credential: None,
            http_client: Arc::new(client),
            cache: Arc::new(RepositoryProjectsCache::default()),
        };

        enrich_tracking_event(&mut event, attribution).await;

        assert_eq!(event.properties["tool"], "review");
        assert_eq!(event.properties["project-ids"], json!([7, 42]));
        assert!(event.properties.get("no-project-matching-reason").is_none());
    }

    #[tokio::test]
    async fn enrichment_request_failure_still_delivers_privacy_safe_event() {
        const RAW_ERROR: &str = "raw mapping failure with private infrastructure details";
        let repository = repository_with_sensitive_remote();
        let action_path = repository.path().join("private-action-path.rs");
        let attribution = path_attribution(
            &action_path,
            Arc::new(FailingHttpClient { error: RAW_ERROR }),
        );

        let body = process_failed_enrichment(
            tracking_event("mcp-test", json!({"tool": "review"})),
            attribution,
        )
        .await;

        assert_eq!(body["event-properties"]["tool"], "review");
        assert_eq!(
            body["event-properties"]["no-project-matching-reason"],
            "repository-projects-request-failed"
        );
        assert!(body["event-properties"].get("project-ids").is_none());
        assert_private_attribution_data_absent(&body, &action_path, RAW_ERROR);
    }

    #[tokio::test]
    async fn invalid_enrichment_response_still_delivers_privacy_safe_error_event() {
        const RAW_RESPONSE: &str = "raw invalid response with private infrastructure details";
        let repository = repository_with_sensitive_remote();
        let action_path = repository.path().join("private-error-path.rs");
        let attribution = path_attribution(
            &action_path,
            Arc::new(MockHttpClient::always(HttpResponse::ok(RAW_RESPONSE))),
        );
        let auth = TrackingAuth {
            access_token: ACCESS_TOKEN.to_string(),
            api_root: None,
        };
        let error = ErrorEvent {
            error_kind: "safe-error-kind",
            tool_name: "safe-tool-name",
            instance_id: "test-instance",
            detail: None,
            auth: &auth,
        };
        let mut properties = None;
        track_error_properties(&error, |value| properties = Some(value));

        let body = process_failed_enrichment(
            tracking_event("mcp-error", properties.unwrap()),
            attribution,
        )
        .await;

        assert_eq!(body["event-properties"]["error"], "safe-error-kind");
        assert_eq!(body["event-properties"]["tool"], "safe-tool-name");
        assert_eq!(
            body["event-properties"]["no-project-matching-reason"],
            "invalid-repository-projects-response"
        );
        assert!(body["event-properties"].get("project-ids").is_none());
        assert_private_attribution_data_absent(&body, &action_path, RAW_RESPONSE);
    }

    #[tokio::test]
    async fn slow_enrichment_does_not_block_tracking_caller() {
        let _lock = config::lock_test_env();
        std::env::remove_var("CS_DISABLE_TRACKING");
        std::env::set_var("CS_TRACKING_URL", "http://127.0.0.1:1");
        let repository = repository_with_sensitive_remote();
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let client = Arc::new(BlockingHttpClient {
            started: started.clone(),
            release: release.clone(),
        });
        let auth = TrackingAuth {
            access_token: ACCESS_TOKEN.to_string(),
            api_root: None,
        };

        track_event_with_attribution(AttributedEvent {
            event: "test-event",
            properties: json!({"tool": "review"}),
            instance_id: "test-instance",
            auth: &auth,
            attribution: path_attribution(repository.path(), client),
        });

        timeout(Duration::from_secs(2), started.notified())
            .await
            .expect("detached attribution should start after the tracking call returns");
        release.notify_one();
        std::env::remove_var("CS_TRACKING_URL");
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
    async fn attributed_tracking_disabled_skips_all_attribution() {
        let _lock = config::lock_test_env();
        std::env::set_var("CS_DISABLE_TRACKING", "1");
        for call in [TestTrackingCall::Event, TestTrackingCall::Error] {
            let client = MockHttpClient::new(Vec::new());
            let requests = client.captured_requests.clone();
            run_test_tracking_call(call, Arc::new(client));
            assert!(requests.lock().unwrap().is_empty());
        }
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
    async fn attributed_tracking_enabled_spawns_without_panic() {
        run_with_tracking_enabled(|| {
            for call in [TestTrackingCall::Event, TestTrackingCall::Error] {
                run_test_tracking_call(call, Arc::new(MockHttpClient::new(Vec::new())));
            }
        })
        .await;
    }
}

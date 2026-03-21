use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

use crate::http::{HttpClient, HttpRequest, Method, ReqwestClient};

const CACHE_DURATION: Duration = Duration::from_secs(3600);

const DEFAULT_CHECK_URL: &str =
    "https://api.github.com/repos/codescene-oss/codescene-mcp-server/releases/latest";

#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub latest: String,
    pub current: String,
    pub is_outdated: bool,
}

struct CachedCheck {
    info: VersionInfo,
    checked_at: Instant,
}

#[derive(Clone)]
pub struct VersionChecker {
    cache: Arc<RwLock<Option<CachedCheck>>>,
    current_version: String,
}

impl VersionChecker {
    pub fn new(current_version: &str) -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            current_version: current_version.to_string(),
        }
    }

    pub fn check_in_background(&self) {
        if is_disabled() || self.current_version == "dev" {
            return;
        }

        let checker = self.clone();
        tokio::spawn(async move {
            checker.refresh_if_stale(&ReqwestClient).await;
        });
    }

    pub async fn try_read(&self) -> Option<VersionInfo> {
        let guard = self.cache.try_read().ok()?;
        guard.as_ref().map(|c| c.info.clone())
    }

    async fn refresh_if_stale(&self, client: &dyn HttpClient) {
        if self.is_cache_fresh().await {
            return;
        }

        if let Some(info) = fetch_latest_version(&self.current_version, client).await {
            let mut guard = self.cache.write().await;
            *guard = Some(CachedCheck {
                info,
                checked_at: Instant::now(),
            });
        }
    }

    async fn is_cache_fresh(&self) -> bool {
        let guard = self.cache.read().await;
        guard
            .as_ref()
            .map(|cached| cached.checked_at.elapsed() < CACHE_DURATION)
            .unwrap_or(false)
    }

    /// Pre-populate the cache for testing purposes.
    #[cfg(test)]
    pub async fn set_cached_info(&self, info: VersionInfo) {
        let mut guard = self.cache.write().await;
        *guard = Some(CachedCheck {
            info,
            checked_at: Instant::now(),
        });
    }
}

async fn fetch_latest_version(
    current: &str,
    client: &dyn HttpClient,
) -> Option<VersionInfo> {
    let url = check_url();

    let request = HttpRequest {
        method: Method::Get,
        url,
        headers: HashMap::from([
            ("User-Agent".to_string(), "cs-mcp".to_string()),
            (
                "Accept".to_string(),
                "application/vnd.github.v3+json".to_string(),
            ),
        ]),
        body: None,
        timeout_secs: 10,
    };

    let resp = client.send(request).await.ok()?;

    if !resp.is_success() {
        return None;
    }

    let body: serde_json::Value = serde_json::from_str(&resp.body).ok()?;
    let latest = body.get("tag_name")?.as_str()?.to_string();

    let is_outdated = latest != current && !current.is_empty() && current != "dev";

    Some(VersionInfo {
        latest,
        current: current.to_string(),
        is_outdated,
    })
}

fn check_url() -> String {
    std::env::var("CS_VERSION_CHECK_URL")
        .unwrap_or_else(|_| DEFAULT_CHECK_URL.to_string())
}

fn is_disabled() -> bool {
    std::env::var("CS_DISABLE_VERSION_CHECK")
        .map(|v| !v.is_empty() && v != "0" && v.to_lowercase() != "false")
        .unwrap_or(false)
}

pub fn format_version_warning(info: &VersionInfo) -> String {
    format!(
        "\n\
        ⚠️  VERSION UPDATE AVAILABLE: {current} → {latest}\n\
        \n\
        Update instructions:\n\
        - Homebrew: brew upgrade codescene-mcp-server\n\
        - Windows: winget upgrade CodeScene.MCP\n\
        - Docker: docker pull codescene/codescene-mcp:latest\n\
        - Manual: https://github.com/codescene-oss/codescene-mcp-server/releases/latest\n",
        current = info.current,
        latest = info.latest,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::MockHttpClient;
    use crate::http::HttpResponse;

    // ---- VersionInfo ----

    #[test]
    fn version_info_clone_and_debug() {
        let info = VersionInfo {
            latest: "1.2.0".to_string(),
            current: "1.0.0".to_string(),
            is_outdated: true,
        };
        let cloned = info.clone();
        assert_eq!(cloned.latest, "1.2.0");
        assert_eq!(cloned.current, "1.0.0");
        assert!(cloned.is_outdated);
        let _debug = format!("{:?}", info);
    }

    // ---- format_version_warning ----

    #[test]
    fn format_version_warning_contains_versions() {
        let info = VersionInfo {
            latest: "2.0.0".to_string(),
            current: "1.0.0".to_string(),
            is_outdated: true,
        };
        let warning = format_version_warning(&info);
        assert!(warning.contains("1.0.0"));
        assert!(warning.contains("2.0.0"));
        assert!(warning.contains("VERSION UPDATE AVAILABLE"));
        assert!(warning.contains("Homebrew"));
        assert!(warning.contains("Docker"));
    }

    // ---- is_disabled ----

    #[test]
    fn is_disabled_when_not_set() {
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
        assert!(!is_disabled());
    }

    #[test]
    fn is_disabled_when_empty() {
        std::env::set_var("CS_DISABLE_VERSION_CHECK", "");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
    }

    #[test]
    fn is_disabled_when_zero() {
        std::env::set_var("CS_DISABLE_VERSION_CHECK", "0");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
    }

    #[test]
    fn is_disabled_when_false() {
        std::env::set_var("CS_DISABLE_VERSION_CHECK", "false");
        assert!(!is_disabled());
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
    }

    #[test]
    fn is_disabled_when_true() {
        std::env::set_var("CS_DISABLE_VERSION_CHECK", "true");
        assert!(is_disabled());
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
    }

    #[test]
    fn is_disabled_when_one() {
        std::env::set_var("CS_DISABLE_VERSION_CHECK", "1");
        assert!(is_disabled());
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
    }

    // ---- check_url ----

    #[test]
    fn check_url_default() {
        std::env::remove_var("CS_VERSION_CHECK_URL");
        assert!(check_url().contains("github.com"));
    }

    #[test]
    fn check_url_override() {
        std::env::set_var("CS_VERSION_CHECK_URL", "https://custom.url/check");
        assert_eq!(check_url(), "https://custom.url/check");
        std::env::remove_var("CS_VERSION_CHECK_URL");
    }

    // ---- VersionChecker::new ----

    #[test]
    fn version_checker_new() {
        let vc = VersionChecker::new("1.0.0");
        assert_eq!(vc.current_version, "1.0.0");
    }

    // ---- VersionChecker::try_read ----

    #[tokio::test]
    async fn try_read_empty_cache() {
        let vc = VersionChecker::new("1.0.0");
        assert!(vc.try_read().await.is_none());
    }

    #[tokio::test]
    async fn try_read_returns_info_after_cache_set() {
        let vc = VersionChecker::new("1.0.0");
        {
            let mut guard = vc.cache.write().await;
            *guard = Some(CachedCheck {
                info: VersionInfo {
                    latest: "2.0.0".to_string(),
                    current: "1.0.0".to_string(),
                    is_outdated: true,
                },
                checked_at: Instant::now(),
            });
        }
        let info = vc.try_read().await.unwrap();
        assert_eq!(info.latest, "2.0.0");
        assert!(info.is_outdated);
    }

    // ---- check_in_background disabled ----

    #[test]
    fn check_in_background_disabled_does_nothing() {
        std::env::set_var("CS_DISABLE_VERSION_CHECK", "1");
        let vc = VersionChecker::new("1.0.0");
        vc.check_in_background();
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
    }

    #[test]
    fn check_in_background_dev_version_does_nothing() {
        let vc = VersionChecker::new("dev");
        vc.check_in_background();
    }

    #[tokio::test]
    async fn check_in_background_enabled_spawns_without_panic() {
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
        std::env::set_var("CS_VERSION_CHECK_URL", "http://192.0.2.1:1/check");
        let vc = VersionChecker::new("1.0.0");
        vc.check_in_background();
        tokio::time::sleep(Duration::from_millis(50)).await;
        std::env::remove_var("CS_VERSION_CHECK_URL");
    }

    // ---- refresh_if_stale ----

    #[tokio::test]
    async fn refresh_if_stale_skips_when_fresh() {
        let vc = VersionChecker::new("1.0.0");
        {
            let mut guard = vc.cache.write().await;
            *guard = Some(CachedCheck {
                info: VersionInfo {
                    latest: "1.0.0".to_string(),
                    current: "1.0.0".to_string(),
                    is_outdated: false,
                },
                checked_at: Instant::now(),
            });
        }
        // Mock should NOT be called — cache is fresh
        let mock = MockHttpClient::new(vec![]);
        vc.refresh_if_stale(&mock).await;
        let reqs = mock.captured_requests.lock().unwrap();
        assert_eq!(reqs.len(), 0);
    }

    #[tokio::test]
    async fn refresh_if_stale_fetches_when_cache_empty() {
        let vc = VersionChecker::new("1.0.0");
        let mock = MockHttpClient::new(vec![HttpResponse::ok(
            r#"{"tag_name":"2.0.0"}"#,
        )]);
        vc.refresh_if_stale(&mock).await;

        let info = vc.try_read().await.unwrap();
        assert_eq!(info.latest, "2.0.0");
        assert!(info.is_outdated);
    }

    // ---- fetch_latest_version ----

    #[tokio::test]
    async fn fetch_latest_version_success() {
        let mock = MockHttpClient::new(vec![HttpResponse::ok(
            r#"{"tag_name":"3.0.0"}"#,
        )]);
        let info = fetch_latest_version("2.0.0", &mock).await.unwrap();
        assert_eq!(info.latest, "3.0.0");
        assert_eq!(info.current, "2.0.0");
        assert!(info.is_outdated);
    }

    #[tokio::test]
    async fn fetch_latest_version_same_version_not_outdated() {
        let mock = MockHttpClient::new(vec![HttpResponse::ok(
            r#"{"tag_name":"1.0.0"}"#,
        )]);
        let info = fetch_latest_version("1.0.0", &mock).await.unwrap();
        assert!(!info.is_outdated);
    }

    #[tokio::test]
    async fn fetch_latest_version_dev_not_outdated() {
        let mock = MockHttpClient::new(vec![HttpResponse::ok(
            r#"{"tag_name":"1.0.0"}"#,
        )]);
        let info = fetch_latest_version("dev", &mock).await.unwrap();
        assert!(!info.is_outdated);
    }

    #[tokio::test]
    async fn fetch_latest_version_http_error_returns_none() {
        let mock = MockHttpClient::new(vec![HttpResponse::error(500, "fail")]);
        assert!(fetch_latest_version("1.0.0", &mock).await.is_none());
    }

    #[tokio::test]
    async fn fetch_latest_version_transport_error_returns_none() {
        let mock = MockHttpClient::new(vec![]);
        assert!(fetch_latest_version("1.0.0", &mock).await.is_none());
    }

    #[tokio::test]
    async fn fetch_latest_version_missing_tag_name_returns_none() {
        let mock = MockHttpClient::new(vec![HttpResponse::ok(r#"{"name":"v1"}"#)]);
        assert!(fetch_latest_version("1.0.0", &mock).await.is_none());
    }

    #[tokio::test]
    async fn fetch_latest_version_invalid_json_returns_none() {
        let mock = MockHttpClient::new(vec![HttpResponse::ok("not json")]);
        assert!(fetch_latest_version("1.0.0", &mock).await.is_none());
    }

    #[tokio::test]
    async fn fetch_latest_version_sends_correct_headers() {
        let mock = MockHttpClient::new(vec![HttpResponse::ok(
            r#"{"tag_name":"1.0.0"}"#,
        )]);
        let captured = mock.captured_requests.clone();

        let _ = fetch_latest_version("1.0.0", &mock).await;

        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].method, Method::Get);
        assert_eq!(reqs[0].headers.get("User-Agent").unwrap(), "cs-mcp");
        assert!(reqs[0].headers.get("Accept").unwrap().contains("github"));
    }
}

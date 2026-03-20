/// Background version checker — mirrors Python's `version_checker.py`.
///
/// Checks GitHub releases API for the latest version. Cached for 1 hour.
/// Non-blocking: the check runs in a background task and results are read
/// via `try_read()`. Disabled by `CS_DISABLE_VERSION_CHECK` env var.

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

/// How long to cache a version check result.
const CACHE_DURATION: Duration = Duration::from_secs(3600);

/// Default GitHub releases API URL.
const DEFAULT_CHECK_URL: &str =
    "https://api.github.com/repos/codescene-oss/codescene-mcp-server/releases/latest";

/// Result of a version check.
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

/// Non-blocking version checker with background refresh.
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

    /// Spawn a background version check if the cache is stale or empty.
    pub fn check_in_background(&self) {
        if is_disabled() || self.current_version == "dev" {
            return;
        }

        let checker = self.clone();
        tokio::spawn(async move {
            checker.refresh_if_stale().await;
        });
    }

    /// Try to read the cached version info without blocking.
    /// Returns `None` if no check has completed yet or the cache is empty.
    pub async fn try_read(&self) -> Option<VersionInfo> {
        let guard = self.cache.try_read().ok()?;
        guard.as_ref().map(|c| c.info.clone())
    }

    async fn refresh_if_stale(&self) {
        // Check if cache is still fresh
        {
            let guard = self.cache.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.checked_at.elapsed() < CACHE_DURATION {
                    return;
                }
            }
        }

        if let Some(info) = fetch_latest_version(&self.current_version).await {
            let mut guard = self.cache.write().await;
            *guard = Some(CachedCheck {
                info,
                checked_at: Instant::now(),
            });
        }
    }
}

async fn fetch_latest_version(current: &str) -> Option<VersionInfo> {
    let url = check_url();
    let client = reqwest::Client::new();

    let resp = client
        .get(&url)
        .header("User-Agent", "cs-mcp")
        .header("Accept", "application/vnd.github.v3+json")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    let latest = body.get("tag_name")?.as_str()?.to_string();

    let is_outdated = latest != current
        && !current.is_empty()
        && current != "dev";

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

/// Format the version warning message, matching the Python output exactly.
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
        // Debug impl exists
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
        assert!(warning.contains("Windows"));
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
        let url = check_url();
        assert!(url.contains("github.com"));
        assert!(url.contains("releases/latest"));
    }

    #[test]
    fn check_url_override() {
        std::env::set_var("CS_VERSION_CHECK_URL", "https://custom.url/check");
        let url = check_url();
        assert_eq!(url, "https://custom.url/check");
        std::env::remove_var("CS_VERSION_CHECK_URL");
    }

    // ---- VersionChecker::new ----

    #[test]
    fn version_checker_new() {
        let vc = VersionChecker::new("1.0.0");
        assert_eq!(vc.current_version, "1.0.0");
    }

    // ---- VersionChecker::try_read (empty cache) ----

    #[tokio::test]
    async fn try_read_empty_cache() {
        let vc = VersionChecker::new("1.0.0");
        assert!(vc.try_read().await.is_none());
    }

    // ---- check_in_background disabled ----

    #[test]
    fn check_in_background_disabled_does_nothing() {
        std::env::set_var("CS_DISABLE_VERSION_CHECK", "1");
        let vc = VersionChecker::new("1.0.0");
        // Should not panic
        vc.check_in_background();
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
    }

    #[test]
    fn check_in_background_dev_version_does_nothing() {
        let vc = VersionChecker::new("dev");
        // Should not panic
        vc.check_in_background();
    }

    // ---- check_in_background enabled (fire-and-forget) ----

    #[tokio::test]
    async fn check_in_background_enabled_spawns_without_panic() {
        std::env::remove_var("CS_DISABLE_VERSION_CHECK");
        // Point to a non-routable URL so the HTTP request fails silently
        std::env::set_var("CS_VERSION_CHECK_URL", "http://192.0.2.1:1/check");
        let vc = VersionChecker::new("1.0.0");
        vc.check_in_background();
        // Give the spawned task a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;
        std::env::remove_var("CS_VERSION_CHECK_URL");
    }

    // ---- try_read after manual cache population ----

    #[tokio::test]
    async fn try_read_returns_info_after_cache_set() {
        let vc = VersionChecker::new("1.0.0");
        // Manually populate the cache
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
        let info = vc.try_read().await;
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.latest, "2.0.0");
        assert!(info.is_outdated);
    }

    // ---- refresh_if_stale with fresh cache ----

    #[tokio::test]
    async fn refresh_if_stale_skips_when_fresh() {
        let vc = VersionChecker::new("1.0.0");
        // Set up a fresh cache entry
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
        // Point to non-routable URL
        std::env::set_var("CS_VERSION_CHECK_URL", "http://192.0.2.1:1/check");
        // Should not refresh since cache is fresh
        vc.refresh_if_stale().await;
        // Cache should still have the same data
        let info = vc.try_read().await.unwrap();
        assert_eq!(info.latest, "1.0.0");
        assert!(!info.is_outdated);
        std::env::remove_var("CS_VERSION_CHECK_URL");
    }
}

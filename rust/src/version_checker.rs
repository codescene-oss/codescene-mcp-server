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

use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::cli::CliRunner;

use super::{
    configured_credential, credential_from_response, fetch_token, now_epoch_secs, run_login,
    AuthCredential, CliTokenResponse,
};

/// Tokens are considered stale this many seconds before their actual `expires-at`.
/// This avoids making an API call with a token that's about to expire mid-flight.
/// 120 s was chosen to accommodate typical request round-trip + CLI overhead.
const TOKEN_EXPIRY_MARGIN_SECS: i64 = 60;

/// When the CLI response does not include `expires-at` (older CLI versions),
/// we cache the response for this duration before re-checking. 5 minutes is
/// conservative enough to avoid excess CLI calls while limiting staleness.
const TOKEN_FALLBACK_TTL_SECS: i64 = 300;

/// Time to cache a "signed out" result to avoid launching the CLI on every
/// tool call when no credentials are configured.
const SIGNED_OUT_CACHE_TTL_SECS: i64 = 30;

/// Manages OAuth token lifecycle: caching, expiry-aware refresh, and
/// serialization of concurrent CLI calls behind a single mutex.
#[derive(Clone)]
pub(crate) struct AuthManager {
    cache: Arc<RwLock<Option<CachedToken>>>,
    refresh_lock: Arc<Mutex<()>>,
    /// Timestamp of the last "signed out" response. Used for negative caching.
    signed_out_at: Arc<RwLock<Option<i64>>>,
}

#[derive(Clone, Debug)]
struct CachedToken {
    response: CliTokenResponse,
    cached_at: i64,
}

impl AuthManager {
    pub(crate) fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            refresh_lock: Arc::new(Mutex::new(())),
            signed_out_at: Arc::new(RwLock::new(None)),
        }
    }

    pub(crate) async fn resolve_credential(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<Option<AuthCredential>, String> {
        if let Some(credential) = configured_credential() {
            return Ok(Some(credential));
        }
        Ok(self
            .current_token(cli_runner)
            .await?
            .and_then(|resp| credential_from_response(&resp)))
    }

    /// Get the current OAuth token (from cache or by refreshing via CLI).
    /// Does NOT check `configured_credential()` — callers that need to prefer
    /// a user-configured PAT should use `resolve_credential` instead.
    /// Used directly by `login::try_existing_session` which has already
    /// short-circuited on configured credentials at the handler level.
    pub(crate) async fn current_token(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<Option<CliTokenResponse>, String> {
        if let Some(cached) = self.fresh_cached_token().await {
            return Ok(Some(cached.response));
        }

        if self.recently_signed_out().await {
            return Ok(None);
        }

        let _guard = self.refresh_lock.lock().await;
        if let Some(cached) = self.fresh_cached_token().await {
            return Ok(Some(cached.response));
        }
        if self.recently_signed_out().await {
            return Ok(None);
        }

        let token = fetch_token(cli_runner).await?;
        if let Some(resp) = token.as_ref() {
            self.store_at(resp.clone(), now_epoch_secs()).await;
        } else {
            self.mark_signed_out().await;
        }
        Ok(token)
    }

    /// Run the interactive login flow. Serialized with token refreshes.
    pub(crate) async fn login(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<CliTokenResponse, String> {
        let _guard = self.refresh_lock.lock().await;
        let resp = run_login(cli_runner).await?;
        if resp.is_signed_in() {
            self.store_at(resp.clone(), now_epoch_secs()).await;
        } else {
            self.mark_signed_out().await;
        }
        Ok(resp)
    }

    async fn fresh_cached_token(&self) -> Option<CachedToken> {
        let guard = self.cache.read().await;
        guard.as_ref().filter(|cached| cached.is_fresh()).cloned()
    }

    async fn recently_signed_out(&self) -> bool {
        let guard = self.signed_out_at.read().await;
        guard
            .map(|ts| now_epoch_secs() - ts < SIGNED_OUT_CACHE_TTL_SECS)
            .unwrap_or(false)
    }

    async fn mark_signed_out(&self) {
        // Clear cache and mark signed-out together to avoid a window where
        // a reader sees empty cache + recently_signed_out == false.
        {
            let mut guard = self.cache.write().await;
            *guard = None;
        }
        let mut guard = self.signed_out_at.write().await;
        *guard = Some(now_epoch_secs());
    }

    async fn store_at(&self, response: CliTokenResponse, cached_at: i64) {
        {
            let mut guard = self.cache.write().await;
            *guard = Some(CachedToken {
                response,
                cached_at,
            });
        }
        // A successful store invalidates any negative cache.
        let mut signed_out = self.signed_out_at.write().await;
        *signed_out = None;
    }

    /// Synchronously try to read the cached OAuth credentials for tracking.
    /// Returns `(access_token, normalized_api_root)` if the cache is available and populated.
    /// Returns `(None, None)` if the cache is empty or contended.
    /// Used for fire-and-forget operations (like tracking) that can't await.
    fn try_cached_auth_info(&self) -> (Option<String>, Option<String>) {
        let Some(guard) = self.cache.try_read().ok() else {
            return (None, None);
        };
        let Some(cached) = guard.as_ref() else {
            return (None, None);
        };
        let token = cached
            .response
            .access_token
            .as_deref()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let api_root =
            credential_from_response(&cached.response).and_then(|cred| cred.api_root().ok());
        (token, api_root)
    }

    pub(crate) fn try_cached_access_token(&self) -> Option<String> {
        self.try_cached_auth_info().0
    }

    pub(crate) fn try_cached_api_root(&self) -> Option<String> {
        self.try_cached_auth_info().1
    }

    #[cfg(test)]
    pub(crate) async fn set_cached_response(&self, response: CliTokenResponse, cached_at: i64) {
        self.store_at(response, cached_at).await;
    }
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedToken {
    fn is_fresh(&self) -> bool {
        let now = now_epoch_secs();
        match self.response.expires_at {
            Some(expires_at) => expires_at > now + TOKEN_EXPIRY_MARGIN_SECS,
            None => self.cached_at + TOKEN_FALLBACK_TTL_SECS > now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockCliRunner;

    fn with_env_lock() -> impl Drop {
        crate::config::lock_test_env()
    }

    fn make_response(access_token: Option<&str>, api_url: Option<&str>) -> CliTokenResponse {
        CliTokenResponse {
            status: "signed_in".into(),
            access_token: access_token.map(Into::into),
            api_url: api_url.map(Into::into),
            expires_at: None,
            refresh_token_expires_at: None,
        }
    }

    fn signed_in_json(token: &str, expires_at: i64) -> String {
        format!(
            r#"{{"status":"signed_in","access-token":"{token}","api-url":"https://api.codescene.io/api","expires-at":{expires_at}}}"#
        )
    }

    async fn with_clean_env<F, Fut>(f: F)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let _lock = with_env_lock();
        std::env::remove_var("CS_ACCESS_TOKEN");
        f().await;
        std::env::remove_var("CS_ACCESS_TOKEN");
    }

    struct EnsureTokenCase<'a> {
        name: &'a str,
        cached_token: Option<&'a str>,
        cached_expires_at: Option<i64>,
        preset_env: Option<&'a str>,
        cli_response: Option<&'a str>,
        expect_token: &'a str,
        expect_cli_calls: usize,
        expect_env_token: Option<&'a str>,
    }

    /// Runs a single resolve_credential scenario: seeds cache, calls auth, checks result.
    async fn assert_resolve_credential(case: &EnsureTokenCase<'_>) {
        let _lock = with_env_lock();
        std::env::remove_var("CS_ACCESS_TOKEN");
        let manager = AuthManager::new();
        if let Some(token) = case.cached_token {
            let mut resp = make_response(Some(token), Some("https://api.codescene.io/api"));
            resp.expires_at = case.cached_expires_at;
            manager.set_cached_response(resp, now_epoch_secs()).await;
        }
        if let Some(env_val) = case.preset_env {
            std::env::set_var("CS_ACCESS_TOKEN", env_val);
        }
        let cli = match case.cli_response {
            Some(json) => MockCliRunner::with_ok(json),
            None => MockCliRunner::with_responses(vec![]),
        };
        let credential = manager.resolve_credential(&cli).await.unwrap().unwrap();
        assert_eq!(
            credential.access_token(),
            case.expect_token,
            "{}",
            case.name
        );
        match case.expect_env_token {
            Some(token) => assert_eq!(std::env::var("CS_ACCESS_TOKEN").unwrap(), token),
            None => assert!(std::env::var("CS_ACCESS_TOKEN").is_err()),
        }
        assert_eq!(
            cli.calls().lock().unwrap().len(),
            case.expect_cli_calls,
            "{}",
            case.name
        );
        std::env::remove_var("CS_ACCESS_TOKEN");
    }

    #[tokio::test]
    async fn resolve_credential_cache_scenarios() {
        let fresh = signed_in_json("fresh-token", now_epoch_secs() + 3600);
        let refreshed = signed_in_json("refreshed", now_epoch_secs() + 7200);
        let cases = [
            EnsureTokenCase {
                name: "fresh cache",
                cached_token: Some("cached-token"),
                cached_expires_at: None,
                preset_env: None,
                cli_response: None,
                expect_token: "cached-token",
                expect_cli_calls: 0,
                expect_env_token: None,
            },
            EnsureTokenCase {
                name: "expired",
                cached_token: Some("expired-token"),
                cached_expires_at: Some(now_epoch_secs() - 10),
                preset_env: None,
                cli_response: Some(&fresh),
                expect_token: "fresh-token",
                expect_cli_calls: 1,
                expect_env_token: None,
            },
            EnsureTokenCase {
                name: "configured env",
                cached_token: Some("expired-token"),
                cached_expires_at: Some(now_epoch_secs() - 10),
                preset_env: Some("pat-token"),
                cli_response: Some(&fresh),
                expect_token: "pat-token",
                expect_cli_calls: 0,
                expect_env_token: Some("pat-token"),
            },
            EnsureTokenCase {
                name: "boundary",
                cached_token: Some("boundary"),
                cached_expires_at: Some(now_epoch_secs() + TOKEN_EXPIRY_MARGIN_SECS),
                preset_env: None,
                cli_response: Some(&refreshed),
                expect_token: "refreshed",
                expect_cli_calls: 1,
                expect_env_token: None,
            },
            EnsureTokenCase {
                name: "above margin",
                cached_token: Some("still-fresh"),
                cached_expires_at: Some(now_epoch_secs() + TOKEN_EXPIRY_MARGIN_SECS + 1),
                preset_env: None,
                cli_response: None,
                expect_token: "still-fresh",
                expect_cli_calls: 0,
                expect_env_token: None,
            },
            EnsureTokenCase {
                name: "oauth without env mutation",
                cached_token: Some("oauth-tok"),
                cached_expires_at: None,
                preset_env: None,
                cli_response: None,
                expect_token: "oauth-tok",
                expect_cli_calls: 0,
                expect_env_token: None,
            },
        ];

        for case in cases {
            assert_resolve_credential(&case).await;
        }
    }

    #[tokio::test]
    async fn resolve_credential_serializes_concurrent_refreshes() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            let cli =
                MockCliRunner::with_ok(&signed_in_json("shared-token", now_epoch_secs() + 3600));
            let calls = cli.calls();
            let (a, b, c) = tokio::join!(
                manager.resolve_credential(&cli),
                manager.resolve_credential(&cli),
                manager.resolve_credential(&cli),
            );
            assert!(a.unwrap().is_some());
            assert!(b.unwrap().is_some());
            assert!(c.unwrap().is_some());
            assert!(std::env::var("CS_ACCESS_TOKEN").is_err());
            assert_eq!(calls.lock().unwrap().len(), 1);
        })
        .await;
    }

    #[tokio::test]
    async fn signed_out_is_cached_briefly() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            let signed_out = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
            let cli =
                MockCliRunner::with_responses(vec![Ok(signed_out.into()), Ok(signed_out.into())]);
            let calls = cli.calls();
            let result = manager.resolve_credential(&cli).await.unwrap();
            assert!(result.is_none());
            assert_eq!(calls.lock().unwrap().len(), 1);
            let result = manager.resolve_credential(&cli).await.unwrap();
            assert!(result.is_none());
            assert_eq!(calls.lock().unwrap().len(), 1);
        })
        .await;
    }

    #[tokio::test]
    async fn login_clears_negative_cache() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            manager.mark_signed_out().await;
            let cli = MockCliRunner::with_ok(&signed_in_json("new-tok", now_epoch_secs() + 3600));
            let resp = manager.login(&cli).await.unwrap();
            assert!(resp.is_signed_in());
            let fresh = manager.fresh_cached_token().await;
            assert!(fresh.is_some());
        })
        .await;
    }

    #[tokio::test]
    async fn cached_cloud_api_root_is_normalized_for_tracking() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            manager
                .store_at(
                    make_response(Some("oau-cloud"), Some("https://api.codescene.io/api")),
                    now_epoch_secs(),
                )
                .await;

            assert_eq!(
                manager.try_cached_api_root().as_deref(),
                Some("https://api.codescene.io")
            );
        })
        .await;
    }

    #[tokio::test]
    async fn current_token_error_preserves_cache() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            let mut resp = make_response(Some("old-tok"), Some("https://api.codescene.io/api"));
            resp.expires_at = Some(now_epoch_secs() - 10);
            manager.set_cached_response(resp, now_epoch_secs()).await;
            let cli = MockCliRunner::with_err(1, "network error");
            let err = manager.current_token(&cli).await;
            assert!(err.is_err());
            let cache = manager.cache.read().await;
            assert!(cache.is_some());
        })
        .await;
    }

    #[tokio::test]
    async fn resolve_credential_clears_oauth_cache_on_signed_out() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            manager
                .store_at(
                    make_response(Some("old-oauth"), Some("https://api.codescene.io/api")),
                    now_epoch_secs() - 600,
                )
                .await;
            {
                let mut guard = manager.cache.write().await;
                if let Some(cached) = guard.as_mut() {
                    cached.response.expires_at = Some(now_epoch_secs() - 10);
                }
            }
            let signed_out = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
            let cli = MockCliRunner::with_ok(signed_out);
            let result = manager.resolve_credential(&cli).await.unwrap();
            assert!(result.is_none());
            assert!(manager.cache.read().await.is_none());
            assert!(std::env::var("CS_ACCESS_TOKEN").is_err());
        })
        .await;
    }

    #[tokio::test]
    async fn concurrent_login_and_resolve_credential() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_responses(vec![Ok(signed_in_json(
                "login-token",
                now_epoch_secs() + 3600,
            ))]);
            let calls = cli.calls();
            let (login_res, ensure_res) =
                tokio::join!(manager.login(&cli), manager.resolve_credential(&cli),);
            assert!(login_res.unwrap().is_signed_in());
            assert!(ensure_res.unwrap().is_some());
            assert!(std::env::var("CS_ACCESS_TOKEN").is_err());
            assert_eq!(calls.lock().unwrap().len(), 1);
        })
        .await;
    }
}

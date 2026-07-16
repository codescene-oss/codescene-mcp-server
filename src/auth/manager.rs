use crate::cli::CliRunner;

use super::{
    configured_credential, credential_from_response, fetch_token, now_epoch_secs, run_login,
    AuthCredential, CliTokenResponse,
};

/// Tokens are considered stale this many seconds before their actual `expires-at`.
/// This avoids making an API call with a token that's about to expire mid-flight.
const TOKEN_EXPIRY_MARGIN_SECS: i64 = 60;

/// Sentinel value for `CS_OAUTH_EXPIRES_AT` indicating the user is signed out
/// and we should not retry the CLI until an explicit login is requested.
const SIGNED_OUT_SENTINEL: &str = "0";

/// Manages OAuth token lifecycle using the config env RwLock for all state.
///
/// All OAuth state lives in process env vars (backed by config file):
/// - `CS_OAUTH_TOKEN` — the access token
/// - `CS_OAUTH_EXPIRES_AT` — expiry as epoch seconds, or "0" for signed-out
/// - `CS_OAUTH_REFRESH_EXPIRES_AT` — refresh token expiry
///
/// The config `RwLock` serializes reads and writes. CLI calls that refresh
/// tokens hold the write lock for the entire duration (including the CLI
/// subprocess wait) to prevent concurrent refresh attempts and ensure
/// consistent state.
#[derive(Clone, Default)]
pub(crate) struct AuthManager;

impl AuthManager {
    pub(crate) fn new() -> Self {
        Self
    }

    /// Resolve the best available credential.
    /// Priority: PAT (CS_ACCESS_TOKEN) > fresh OAuth token > CLI refresh > None.
    pub(crate) async fn resolve_credential(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<Option<AuthCredential>, String> {
        // PAT takes priority.
        if let Some(credential) = configured_credential() {
            tracing::info!(source = "configured", "resolved auth credential from configured token");
            return Ok(Some(credential));
        }
        // Check persisted OAuth token.
        if let Some(cred) = Self::fresh_oauth_credential() {
            tracing::info!(
                source = "oauth_cache",
                has_onprem_url = cred.api_root().ok().is_some(),
                "resolved auth credential from cached OAuth token"
            );
            return Ok(Some(cred));
        }
        // Token missing or expired — try to refresh via CLI.
        tracing::info!(
            has_oauth_token = crate::config::try_read_env("CS_OAUTH_TOKEN").is_some(),
            oauth_expires_at = crate::config::try_read_env("CS_OAUTH_EXPIRES_AT"),
            signed_out_sentinel = Self::is_signed_out(),
            "no usable cached credential; attempting CLI auth token refresh"
        );
        self.refresh_token(cli_runner).await
    }

    /// Get the current OAuth token if it exists and is fresh.
    /// Does NOT check `configured_credential()` — callers that need to prefer
    /// a user-configured PAT should use `resolve_credential` instead.
    pub(crate) async fn current_token(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<Option<CliTokenResponse>, String> {
        // Check if we have a fresh token in env.
        if Self::has_fresh_oauth_token() {
            // Build a synthetic CliTokenResponse from env for compatibility.
            return Ok(Self::token_response_from_env());
        }
        // If signed out (sentinel), don't retry automatically.
        if Self::is_signed_out() {
            return Ok(None);
        }
        // Refresh via CLI under write lock.
        match self.refresh_token_raw(cli_runner).await? {
            Some(resp) => Ok(Some(resp)),
            None => Ok(None),
        }
    }

    /// Run the interactive login flow. Holds the config write lock for the
    /// entire duration (browser flow may take up to 2 minutes).
    pub(crate) async fn login(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<CliTokenResponse, String> {
        let guard = crate::config::acquire_write_lock().await;

        // Double-check: maybe another call just completed login.
        if Self::guard_has_fresh_oauth_token(&guard) {
            let cached = Self::build_token_response_from_guard(&guard);
            tracing::info!(
                oauth_expires_at = cached.expires_at,
                has_oauth_token = cached
                    .access_token
                    .as_deref()
                    .is_some_and(|token| !token.trim().is_empty()),
                "skipping interactive login because a fresh cached OAuth token already exists"
            );
            return Ok(cached);
        }

        let resp = run_login(cli_runner).await?;
        if resp.is_signed_in() {
            let persisted = if response_has_access_token(&resp) {
                Self::persist_response(&guard, &resp);
                resp.clone()
            } else {
                tracing::info!(
                    status = %resp.status,
                    "login response did not include an access token; fetching token export from CLI"
                );
                let token_resp = fetch_token(cli_runner).await?.ok_or_else(|| {
                    "CLI login succeeded but token export remained unavailable".to_string()
                })?;
                Self::persist_response(&guard, &token_resp);
                token_resp
            };
            tracing::info!(
                status = %persisted.status,
                has_access_token = response_has_access_token(&persisted),
                expires_at = persisted.expires_at,
                refresh_expires_at = persisted.refresh_token_expires_at,
                "persisted OAuth login response"
            );
            return Ok(persisted);
        } else {
            Self::persist_signed_out(&guard);
            tracing::info!(status = %resp.status, "login response was not signed in; persisted signed-out sentinel");
        }
        Ok(resp)
    }

    /// Try to read a fresh OAuth credential from env.
    fn fresh_oauth_credential() -> Option<AuthCredential> {
        let vals = crate::config::try_read_env_multi(&[
            "CS_OAUTH_TOKEN",
            "CS_OAUTH_EXPIRES_AT",
            "CS_ONPREM_URL",
        ])?;
        let token = vals[0].as_deref()?.trim().to_string();
        if token.is_empty() {
            tracing::info!(
                oauth_expires_at = vals[1].clone(),
                "cached OAuth state has no access token"
            );
            return None;
        }
        // Check expiry.
        if let Some(expires_str) = vals[1].as_deref() {
            if let Ok(expires_at) = expires_str.parse::<i64>() {
                if expires_at <= now_epoch_secs() + TOKEN_EXPIRY_MARGIN_SECS {
                    tracing::info!(oauth_expires_at = expires_at, "cached OAuth token is expired or within refresh margin");
                    return None; // expired or signed-out sentinel
                }
            }
        }
        let onprem_url = vals[2]
            .as_deref()
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty());
        Some(AuthCredential::OAuth {
            access_token: token,
            onprem_url,
        })
    }

    /// Check if we have a non-expired OAuth token in env.
    fn has_fresh_oauth_token() -> bool {
        Self::fresh_oauth_credential().is_some()
    }

    fn guard_has_fresh_oauth_token(guard: &crate::config::ConfigEnvWriteGuard) -> bool {
        let Some(token) = guard.read_env("CS_OAUTH_TOKEN") else {
            return false;
        };
        if token.trim().is_empty() {
            return false;
        }
        let Some(expires_str) = guard.read_env("CS_OAUTH_EXPIRES_AT") else {
            return false;
        };
        let Ok(expires_at) = expires_str.parse::<i64>() else {
            return false;
        };
        expires_at > now_epoch_secs() + TOKEN_EXPIRY_MARGIN_SECS
    }

    /// Check if the signed-out sentinel is set.
    fn is_signed_out() -> bool {
        crate::config::try_read_env("CS_OAUTH_EXPIRES_AT")
            .as_deref()
            == Some(SIGNED_OUT_SENTINEL)
    }

    /// Build a synthetic `CliTokenResponse` from env vars (for compatibility
    /// with callers that expect a response object).
    fn token_response_from_env() -> Option<CliTokenResponse> {
        let vals = crate::config::try_read_env_multi(&[
            "CS_OAUTH_TOKEN",
            "CS_OAUTH_EXPIRES_AT",
            "CS_OAUTH_REFRESH_EXPIRES_AT",
            "CS_ONPREM_URL",
        ])?;
        let token = vals[0].clone()?;
        let expires_at = vals[1].as_deref().and_then(|s| s.parse::<i64>().ok());
        let refresh_expires_at = vals[2].as_deref().and_then(|s| s.parse::<i64>().ok());
        let api_url = vals[3]
            .as_deref()
            .map(|u| format!("{}/api", u.trim_end_matches('/')));
        Some(CliTokenResponse {
            status: "signed_in".into(),
            access_token: Some(token),
            api_url,
            expires_at,
            refresh_token_expires_at: refresh_expires_at,
        })
    }

    /// Refresh token via CLI, acquiring the config write lock.
    /// Returns the resolved credential or None.
    async fn refresh_token(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<Option<AuthCredential>, String> {
        let resp = self.refresh_token_raw(cli_runner).await?;
        let credential = resp.and_then(|r| credential_from_response(&r));
        tracing::info!(resolved = credential.is_some(), "completed CLI auth token refresh");
        Ok(credential)
    }

    /// Refresh token via CLI under the config write lock.
    /// Returns the raw CliTokenResponse.
    async fn refresh_token_raw(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<Option<CliTokenResponse>, String> {
        let guard = crate::config::acquire_write_lock().await;

        // Double-check under write lock: another thread may have refreshed.
        if Self::guard_has_fresh_oauth_token(&guard) {
            let cached = Self::build_token_response_from_guard(&guard);
            tracing::info!(
                oauth_expires_at = cached.expires_at,
                has_oauth_token = cached
                    .access_token
                    .as_deref()
                    .is_some_and(|token| !token.trim().is_empty()),
                "reusing OAuth token that appeared while waiting for auth lock"
            );
            return Ok(Some(cached));
        }

        if guard.read_env("CS_OAUTH_EXPIRES_AT").as_deref() == Some(SIGNED_OUT_SENTINEL) {
            tracing::info!("skipping CLI auth token refresh because signed-out sentinel is set");
            return Ok(None);
        }

        tracing::info!("running CLI auth token refresh");
        let token = fetch_token(cli_runner).await?;
        match token {
            Some(resp) if resp.is_signed_in() => {
                Self::persist_response(&guard, &resp);
                tracing::info!(
                    status = %resp.status,
                    has_access_token = resp.access_token.as_deref().is_some_and(|t| !t.trim().is_empty()),
                    expires_at = resp.expires_at,
                    refresh_expires_at = resp.refresh_token_expires_at,
                    "CLI auth token refresh succeeded"
                );
                Ok(Some(resp))
            }
            _ => {
                Self::persist_signed_out(&guard);
                tracing::info!("CLI auth token refresh reported signed-out state; persisted sentinel");
                Ok(None)
            }
        }
    }

    /// Persist a successful token response to env + config under the write lock.
    fn persist_response(guard: &crate::config::ConfigEnvWriteGuard, response: &CliTokenResponse) {
        let token = response
            .access_token
            .as_deref()
            .unwrap_or("")
            .trim();
        let expires_at = response
            .expires_at
            .map(|v| v.to_string())
            .unwrap_or_default();
        let refresh_expires_at = response
            .refresh_token_expires_at
            .map(|v| v.to_string())
            .unwrap_or_default();

        let entries: &[(&str, &str)] = &[
            ("oauth_token", token),
            ("oauth_expires_at", &expires_at),
            ("oauth_refresh_expires_at", &refresh_expires_at),
        ];

        if let Err(e) = guard.write_env_multi(entries) {
            tracing::warn!(error = %e, "failed to persist OAuth state to config file");
        }
        tracing::info!(
            has_access_token = !token.is_empty(),
            expires_at = %expires_at,
            refresh_expires_at = %refresh_expires_at,
            "persisted OAuth state to config-backed environment"
        );
    }

    /// Write the signed-out sentinel to env + config under the write lock.
    fn persist_signed_out(guard: &crate::config::ConfigEnvWriteGuard) {
        let entries: &[(&str, &str)] = &[
            ("oauth_token", ""),
            ("oauth_expires_at", SIGNED_OUT_SENTINEL),
            ("oauth_refresh_expires_at", ""),
        ];
        if let Err(e) = guard.write_env_multi(entries) {
            tracing::warn!(error = %e, "failed to persist signed-out state to config file");
        }
        tracing::info!("persisted signed-out sentinel to config-backed environment");
    }

    /// Build a CliTokenResponse from env while holding the write guard.
    fn build_token_response_from_guard(
        guard: &crate::config::ConfigEnvWriteGuard,
    ) -> CliTokenResponse {
        let token = guard.read_env("CS_OAUTH_TOKEN");
        let expires_at = guard
            .read_env("CS_OAUTH_EXPIRES_AT")
            .and_then(|s| s.parse::<i64>().ok());
        let refresh_expires_at = guard
            .read_env("CS_OAUTH_REFRESH_EXPIRES_AT")
            .and_then(|s| s.parse::<i64>().ok());
        let api_url = guard
            .read_env("CS_ONPREM_URL")
            .map(|u| format!("{}/api", u.trim_end_matches('/')));
        CliTokenResponse {
            status: "signed_in".into(),
            access_token: token,
            api_url,
            expires_at,
            refresh_token_expires_at: refresh_expires_at,
        }
    }

    /// Synchronously try to read the OAuth token for tracking purposes.
    /// Returns `None` if not available.
    pub(crate) fn try_cached_access_token(&self) -> Option<String> {
        crate::config::try_read_env("CS_OAUTH_TOKEN")
    }

    /// Synchronously try to read the OAuth API root for tracking purposes.
    pub(crate) fn try_cached_api_root(&self) -> Option<String> {
        Self::fresh_oauth_credential().and_then(|cred| cred.api_root().ok())
    }

    #[cfg(test)]
    pub(crate) async fn set_cached_response(
        &self,
        response: CliTokenResponse,
        _cached_at: i64,
    ) {
        let guard = crate::config::acquire_write_lock().await;
        Self::persist_response(&guard, &response);
    }
}

fn response_has_access_token(response: &CliTokenResponse) -> bool {
    response
        .access_token
        .as_deref()
        .is_some_and(|token| !token.trim().is_empty())
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

    fn clean_oauth_env() {
        std::env::remove_var("CS_ACCESS_TOKEN");
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_REFRESH_EXPIRES_AT");
    }

    async fn with_clean_env<F, Fut>(f: F)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let _lock = with_env_lock();
        clean_oauth_env();
        f().await;
        clean_oauth_env();
    }

    fn empty_cli() -> MockCliRunner {
        MockCliRunner::with_responses(vec![])
    }

    fn cli_call_count(cli: &MockCliRunner) -> usize {
        cli.calls().lock().unwrap().len()
    }

    async fn assert_resolve_credential(
        setup: impl FnOnce(),
        cli: MockCliRunner,
        expected_token: Option<&str>,
        expected_cli_calls: usize,
    ) {
        let manager = AuthManager::new();
        setup();
        let result = manager.resolve_credential(&cli).await.unwrap();
        assert_eq!(result.as_ref().map(|cred| cred.access_token()), expected_token);
        assert_eq!(cli_call_count(&cli), expected_cli_calls);
    }

    async fn assert_current_token(
        setup: impl FnOnce(),
        cli: MockCliRunner,
        expected_token: Option<&str>,
        expected_cli_calls: usize,
    ) {
        let manager = AuthManager::new();
        setup();
        let result = manager.current_token(&cli).await.unwrap();
        assert_eq!(result.as_ref().and_then(|resp| resp.access_token.as_deref()), expected_token);
        assert_eq!(cli_call_count(&cli), expected_cli_calls);
    }

    #[tokio::test]
    async fn cached_auth_short_circuits_without_cli() {
        with_clean_env(|| async {
            assert_resolve_credential(
                || std::env::set_var("CS_ACCESS_TOKEN", "pat-token"),
                empty_cli(),
                Some("pat-token"),
                0,
            )
            .await;

            clean_oauth_env();
            assert_resolve_credential(
                || {
                    std::env::set_var("CS_OAUTH_TOKEN", "oau-fresh");
                    std::env::set_var(
                        "CS_OAUTH_EXPIRES_AT",
                        (now_epoch_secs() + 3600).to_string(),
                    );
                },
                empty_cli(),
                Some("oau-fresh"),
                0,
            )
            .await;

            clean_oauth_env();
            assert_resolve_credential(
                || std::env::set_var("CS_OAUTH_EXPIRES_AT", SIGNED_OUT_SENTINEL),
                empty_cli(),
                None,
                0,
            )
            .await;

            clean_oauth_env();
            assert_current_token(
                || std::env::set_var("CS_OAUTH_EXPIRES_AT", SIGNED_OUT_SENTINEL),
                empty_cli(),
                None,
                0,
            )
            .await;
        })
        .await;
    }

    #[tokio::test]
    async fn resolve_credential_refreshes_expired_oauth() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "oau-expired");
            std::env::set_var(
                "CS_OAUTH_EXPIRES_AT",
                (now_epoch_secs() - 10).to_string(),
            );
            let fresh = signed_in_json("oau-new", now_epoch_secs() + 3600);
            let cli = MockCliRunner::with_ok(&fresh);
            assert_resolve_credential(|| {}, cli, Some("oau-new"), 1).await;
        })
        .await;
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
            // Only one CLI call should have been made (write lock serialization).
            assert_eq!(calls.lock().unwrap().len(), 1);
        })
        .await;
    }

    #[tokio::test]
    async fn login_persists_and_returns_token() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_ok(&signed_in_json("login-tok", now_epoch_secs() + 3600));
            let resp = manager.login(&cli).await.unwrap();
            assert!(resp.is_signed_in());
            assert_eq!(resp.access_token.as_deref(), Some("login-tok"));
            // Verify persisted to env.
            assert_eq!(
                std::env::var("CS_OAUTH_TOKEN").ok().as_deref(),
                Some("login-tok")
            );
        })
        .await;
    }

    #[tokio::test]
    async fn login_fetches_token_when_login_response_omits_access_token() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            let login_json = format!(
                r#"{{"status":"signed_in","access-token":null,"api-url":"https://api.codescene.io/api","expires-at":{},"refresh-token-expires-at":{}}}"#,
                now_epoch_secs() + 3600,
                now_epoch_secs() + 7200
            );
            let token_json = signed_in_json("fetched-tok", now_epoch_secs() + 3600);
            let cli = MockCliRunner::with_responses(vec![Ok(login_json), Ok(token_json)]);
            let resp = manager.login(&cli).await.unwrap();
            assert!(resp.is_signed_in());
            assert_eq!(resp.access_token.as_deref(), Some("fetched-tok"));
            assert_eq!(std::env::var("CS_OAUTH_TOKEN").ok().as_deref(), Some("fetched-tok"));
            assert_eq!(cli.calls().lock().unwrap().len(), 2);
        })
        .await;
    }

    #[tokio::test]
    async fn login_skips_if_fresh_token_appeared() {
        with_clean_env(|| async {
            // Simulate another thread having just completed login.
            std::env::set_var("CS_OAUTH_TOKEN", "already-fresh");
            std::env::set_var(
                "CS_OAUTH_EXPIRES_AT",
                (now_epoch_secs() + 3600).to_string(),
            );
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_responses(vec![]);
            let resp = manager.login(&cli).await.unwrap();
            assert!(resp.is_signed_in());
            assert_eq!(resp.access_token.as_deref(), Some("already-fresh"));
            // CLI should not have been called.
            assert_eq!(cli.calls().lock().unwrap().len(), 0);
        })
        .await;
    }

    #[tokio::test]
    async fn current_token_refreshes_when_expiry_exists_but_token_missing() {
        with_clean_env(|| async {
            let cli = MockCliRunner::with_ok(&signed_in_json("recovered-token", now_epoch_secs() + 3600));
            assert_current_token(
                || {
                    std::env::set_var(
                        "CS_OAUTH_EXPIRES_AT",
                        (now_epoch_secs() + 3600).to_string(),
                    );
                },
                cli,
                Some("recovered-token"),
                1,
            )
            .await;
            assert_eq!(std::env::var("CS_OAUTH_TOKEN").ok().as_deref(), Some("recovered-token"));
        })
        .await;
    }

    #[tokio::test]
    async fn try_cached_access_token_reads_from_env() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "oau-tracking");
            let manager = AuthManager::new();
            assert_eq!(
                manager.try_cached_access_token().as_deref(),
                Some("oau-tracking")
            );
        })
        .await;
    }

    #[tokio::test]
    async fn login_marks_signed_out_on_failure() {
        with_clean_env(|| async {
            let signed_out = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_ok(signed_out);
            let resp = manager.login(&cli).await.unwrap();
            assert!(!resp.is_signed_in());
            assert_eq!(
                std::env::var("CS_OAUTH_EXPIRES_AT").ok().as_deref(),
                Some(SIGNED_OUT_SENTINEL)
            );
        })
        .await;
    }
}

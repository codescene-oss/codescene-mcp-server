use crate::cli::CliRunner;

use super::{
    configured_credential, credential_from_response, fetch_token, run_login, run_logout, state,
    AuthCredential, CliTokenResponse,
};

/// Outcome of [`AuthManager::switch_account`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SwitchAccountStatus {
    /// Already authenticated for the requested account (possibly after backfill).
    AlreadyOnAccount,
    /// Reused an existing CLI credential slot without interactive login.
    ReusedSession,
    /// Completed interactive OAuth login into the requested account slot.
    SignedIn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SwitchAccountResult {
    pub status: SwitchAccountStatus,
    pub account_id: i64,
}

impl SwitchAccountResult {
    pub(crate) fn status_str(&self) -> &'static str {
        match self.status {
            SwitchAccountStatus::AlreadyOnAccount => "already_on_account",
            SwitchAccountStatus::ReusedSession => "reused_session",
            SwitchAccountStatus::SignedIn => "signed_in",
        }
    }
}

/// Manages OAuth token lifecycle using the config env RwLock for all state.
///
/// All OAuth state lives in process env vars (backed by config file):
/// - `CS_OAUTH_TOKEN` — the access token
/// - `CS_OAUTH_EXPIRES_AT` — expiry as epoch seconds, or "0" for signed-out
/// - `CS_OAUTH_REFRESH_EXPIRES_AT` — refresh token expiry
/// - `CS_OAUTH_ACCOUNT_ID` — Cloud account for the cached OAuth session
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
            tracing::info!(
                source = "configured",
                "resolved auth credential from configured token"
            );
            return Ok(Some(credential));
        }
        // Check persisted OAuth token.
        if let Some(cred) = state::fresh_credential() {
            let has_onprem_url = cred.api_root().ok().is_some();
            tracing::info!(
                source = "oauth_cache",
                has_onprem_url,
                "resolved auth credential from cached OAuth token"
            );
            return Ok(Some(cred));
        }
        // Token missing or expired — try to refresh via CLI.
        let has_oauth_token = crate::config::try_read_env("CS_OAUTH_TOKEN").is_some();
        let oauth_expires_at = crate::config::try_read_env("CS_OAUTH_EXPIRES_AT");
        let signed_out_sentinel = state::is_signed_out();
        tracing::info!(
            has_oauth_token,
            oauth_expires_at,
            signed_out_sentinel,
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
        if state::has_fresh_token() {
            // Build a synthetic CliTokenResponse from env for compatibility.
            return Ok(state::response_from_env());
        }
        // If signed out (sentinel), don't retry automatically.
        if state::is_signed_out() {
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
        if state::guard_has_fresh_token(&guard) {
            let cached = state::response_from_guard(&guard);
            let has_oauth_token = cached
                .access_token
                .as_deref()
                .is_some_and(|token| !token.trim().is_empty());
            tracing::info!(
                oauth_expires_at = cached.expires_at,
                has_oauth_token,
                "skipping interactive login because a fresh cached OAuth token already exists"
            );
            return Ok(cached);
        }

        let resp = run_login(cli_runner).await?;
        if resp.is_signed_in() {
            let persisted = if response_has_access_token(&resp) {
                state::persist_response(&guard, &resp);
                resp.clone()
            } else {
                tracing::info!(
                    status = %resp.status,
                    "login response did not include an access token; fetching token export from CLI"
                );
                let token_resp = fetch_token(cli_runner).await?.ok_or_else(|| {
                    "CLI login succeeded but token export remained unavailable".to_string()
                })?;
                state::persist_response(&guard, &token_resp);
                token_resp
            };
            let has_access_token = response_has_access_token(&persisted);
            tracing::info!(
                status = %persisted.status,
                has_access_token,
                expires_at = persisted.expires_at,
                refresh_expires_at = persisted.refresh_token_expires_at,
                "persisted OAuth login response"
            );
            return Ok(persisted);
        } else {
            state::persist_signed_out(&guard);
            tracing::info!(status = %resp.status, "login response was not signed in; persisted signed-out sentinel");
        }
        Ok(resp)
    }

    /// Refresh token via CLI, acquiring the config write lock.
    /// Returns the resolved credential or None.
    async fn refresh_token(
        &self,
        cli_runner: &dyn CliRunner,
    ) -> Result<Option<AuthCredential>, String> {
        let resp = self.refresh_token_raw(cli_runner).await?;
        let credential = resp.and_then(|r| credential_from_response(&r));
        let resolved = credential.is_some();
        tracing::info!(resolved, "completed CLI auth token refresh");
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
        if state::guard_has_fresh_token(&guard) {
            let cached = state::response_from_guard(&guard);
            let has_oauth_token = cached
                .access_token
                .as_deref()
                .is_some_and(|token| !token.trim().is_empty());
            tracing::info!(
                oauth_expires_at = cached.expires_at,
                has_oauth_token,
                "reusing OAuth token that appeared while waiting for auth lock"
            );
            return Ok(Some(cached));
        }

        if state::guard_is_signed_out(&guard) {
            tracing::info!("skipping CLI auth token refresh because signed-out sentinel is set");
            return Ok(None);
        }

        tracing::info!("running CLI auth token refresh");
        let token = fetch_token(cli_runner).await?;
        match token {
            Some(resp) if resp.is_signed_in() => {
                state::persist_response(&guard, &resp);
                let has_access_token = resp
                    .access_token
                    .as_deref()
                    .is_some_and(|t| !t.trim().is_empty());
                tracing::info!(
                    status = %resp.status,
                    has_access_token,
                    expires_at = resp.expires_at,
                    refresh_expires_at = resp.refresh_token_expires_at,
                    "CLI auth token refresh succeeded"
                );
                Ok(Some(resp))
            }
            _ => {
                state::persist_signed_out(&guard);
                tracing::info!(
                    "CLI auth token refresh reported signed-out state; persisted sentinel"
                );
                Ok(None)
            }
        }
    }

    /// Sign out: run CLI logout and persist the signed-out sentinel.
    ///
    /// Always clears MCP OAuth config (even if the CLI call fails) so a later
    /// `auth token` refresh cannot rehydrate the session. Returns `Err` when
    /// the CLI logout fails; callers should still treat local state as signed out.
    pub(crate) async fn logout(&self, cli_runner: &dyn CliRunner) -> Result<(), String> {
        let guard = crate::config::acquire_write_lock().await;
        let result = run_logout(cli_runner).await;
        state::persist_signed_out(&guard);
        match &result {
            Ok(resp) => {
                tracing::info!(
                    status = %resp.status,
                    "CLI logout completed; persisted signed-out sentinel"
                );
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "CLI logout failed; still persisted signed-out sentinel"
                );
            }
        }
        result.map(|_| ())
    }

    /// Switch the active Cloud OAuth account.
    ///
    /// Pins `account_id`, clears the MCP OAuth cache (not other CLI slots),
    /// then reuses the target CLI credential slot or runs interactive login.
    pub(crate) async fn switch_account(
        &self,
        cli_runner: &dyn CliRunner,
        account_id: i64,
    ) -> Result<SwitchAccountResult, String> {
        validate_switch_account_request(account_id)?;

        let guard = crate::config::acquire_write_lock().await;
        let target = account_id.to_string();
        let known_account = resolve_session_account(cli_runner, &guard).await?;

        if known_account == Some(account_id) && state::guard_has_fresh_token(&guard) {
            ensure_account_pin(&guard, &target)?;
            tracing::info!(account_id, "switch_account: already on requested account");
            return Ok(SwitchAccountResult {
                status: SwitchAccountStatus::AlreadyOnAccount,
                account_id,
            });
        }

        ensure_account_pin(&guard, &target)?;
        state::clear_oauth_cache(&guard);

        if let Some(result) = try_reuse_credential_slot(cli_runner, &guard, account_id).await? {
            return Ok(result);
        }
        interactive_switch_login(cli_runner, &guard, account_id).await
    }

    /// Synchronously try to read the OAuth token for tracking purposes.
    /// Returns `None` if not available.
    pub(crate) fn try_cached_access_token(&self) -> Option<String> {
        crate::config::try_read_env("CS_OAUTH_TOKEN")
    }

    /// Synchronously try to read the OAuth API root for tracking purposes.
    pub(crate) fn try_cached_api_root(&self) -> Option<String> {
        state::fresh_credential().and_then(|cred| cred.api_root().ok())
    }

    /// Account ID for the cached OAuth session, if known.
    pub(crate) fn try_cached_oauth_account_id(&self) -> Option<i64> {
        state::cached_oauth_account_id()
    }
}

fn validate_switch_account_request(account_id: i64) -> Result<(), String> {
    if account_id <= 0 {
        return Err("account_id must be a positive integer".to_string());
    }
    if configured_credential().is_some() {
        return Err(
            "CS_ACCESS_TOKEN is already configured. OAuth account switching is not available.\n\
             To use OAuth instead, remove CS_ACCESS_TOKEN from your MCP client configuration \
             or unset it from your shell environment."
                .to_string(),
        );
    }
    Ok(())
}

fn ensure_account_pin(
    guard: &crate::config::ConfigEnvWriteGuard,
    account_id: &str,
) -> Result<(), String> {
    guard
        .write_env_multi(&[("account_id", account_id)])
        .map_err(|e| format!("failed to persist account_id: {e}"))
}

async fn resolve_session_account(
    cli_runner: &dyn CliRunner,
    guard: &crate::config::ConfigEnvWriteGuard,
) -> Result<Option<i64>, String> {
    if let Some(id) = state::guard_cached_oauth_account_id(guard) {
        return Ok(Some(id));
    }
    backfill_session_account(cli_runner, guard).await
}

/// Ask the CLI for the current session account without clearing MCP cache.
async fn backfill_session_account(
    cli_runner: &dyn CliRunner,
    guard: &crate::config::ConfigEnvWriteGuard,
) -> Result<Option<i64>, String> {
    if state::guard_is_signed_out(guard) {
        return Ok(None);
    }
    // Soft-fail: a backfill miss must not block switching into another slot.
    let Ok(Some(resp)) = fetch_token(cli_runner).await else {
        return Ok(None);
    };
    if !resp.is_signed_in() {
        return Ok(None);
    }
    let account_id = resp.account_id.filter(|&id| id > 0);
    // Persist metadata/token only. Do not write the account_id config pin —
    // that would retarget the browser-selected CLI slot to a numbered slot.
    persist_backfilled_session(guard, &resp, account_id);
    Ok(account_id)
}

fn persist_backfilled_session(
    guard: &crate::config::ConfigEnvWriteGuard,
    resp: &CliTokenResponse,
    account_id: Option<i64>,
) {
    if response_has_access_token(resp) {
        state::persist_response(guard, resp);
        return;
    }
    if let Some(id) = account_id {
        let id_str = id.to_string();
        let _ = guard.write_env_multi(&[("oauth_account_id", &id_str)]);
    }
}

async fn try_reuse_credential_slot(
    cli_runner: &dyn CliRunner,
    guard: &crate::config::ConfigEnvWriteGuard,
    account_id: i64,
) -> Result<Option<SwitchAccountResult>, String> {
    tracing::info!(account_id, "switch_account: trying CLI credential slot");
    let Some(resp) = fetch_token(cli_runner).await? else {
        return Ok(None);
    };
    if !resp.is_signed_in() {
        return Ok(None);
    }
    let persisted = persist_signed_in_response(cli_runner, guard, resp).await?;
    let active = persisted.account_id.unwrap_or(account_id);
    tracing::info!(account_id = active, "switch_account: reused CLI credential slot");
    Ok(Some(SwitchAccountResult {
        status: SwitchAccountStatus::ReusedSession,
        account_id: active,
    }))
}

async fn interactive_switch_login(
    cli_runner: &dyn CliRunner,
    guard: &crate::config::ConfigEnvWriteGuard,
    account_id: i64,
) -> Result<SwitchAccountResult, String> {
    tracing::info!(account_id, "switch_account: starting interactive login");
    let resp = run_login(cli_runner).await?;
    if !resp.is_signed_in() {
        state::persist_signed_out(guard);
        return Err(format!(
            "Login did not complete while switching to account {account_id}. Status: {}",
            resp.status
        ));
    }
    let persisted = persist_signed_in_response(cli_runner, guard, resp).await?;
    let active = persisted.account_id.unwrap_or(account_id);
    tracing::info!(account_id = active, "switch_account: interactive login succeeded");
    Ok(SwitchAccountResult {
        status: SwitchAccountStatus::SignedIn,
        account_id: active,
    })
}

async fn persist_signed_in_response(
    cli_runner: &dyn CliRunner,
    guard: &crate::config::ConfigEnvWriteGuard,
    resp: CliTokenResponse,
) -> Result<CliTokenResponse, String> {
    if response_has_access_token(&resp) {
        state::persist_response(guard, &resp);
        return Ok(resp);
    }
    tracing::info!(
        status = %resp.status,
        "signed-in response omitted access token; fetching token export from CLI"
    );
    let token_resp = fetch_token(cli_runner).await?.ok_or_else(|| {
        "CLI reported signed in but token export remained unavailable".to_string()
    })?;
    if !token_resp.is_signed_in() || !response_has_access_token(&token_resp) {
        return Err("CLI reported signed in but token export remained unavailable".to_string());
    }
    state::persist_response(guard, &token_resp);
    Ok(token_resp)
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
    use crate::{auth::now_epoch_secs, test_utils::MockCliRunner};
    use std::future::Future;

    const SIGNED_OUT_SENTINEL: &str = "0";

    fn with_env_lock() -> impl Drop {
        crate::config::lock_test_env()
    }

    fn signed_in_json(token: &str, expires_at: i64) -> String {
        format!(
            r#"{{"status":"signed_in","access-token":"{token}","api-url":"https://api.codescene.io/api","expires-at":{expires_at}}}"#
        )
    }

    fn signed_in_json_with_account(token: &str, expires_at: i64, account_id: i64) -> String {
        format!(
            r#"{{"status":"signed_in","access-token":"{token}","api-url":"https://api.codescene.io/api","expires-at":{expires_at},"account-id":{account_id}}}"#
        )
    }

    fn clean_oauth_env() {
        std::env::remove_var("CS_ACCESS_TOKEN");
        std::env::remove_var("CS_OAUTH_TOKEN");
        std::env::remove_var("CS_OAUTH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_REFRESH_EXPIRES_AT");
        std::env::remove_var("CS_OAUTH_ACCOUNT_ID");
        std::env::remove_var("CS_ACCOUNT_ID");
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
        assert_eq!(
            result.as_ref().map(|cred| cred.access_token()),
            expected_token
        );
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
        assert_eq!(
            result
                .as_ref()
                .and_then(|resp| resp.access_token.as_deref()),
            expected_token
        );
        assert_eq!(cli_call_count(&cli), expected_cli_calls);
    }

    #[tokio::test]
    async fn cached_auth_short_circuits_without_cli() {
        with_clean_env(|| async {
            std::env::remove_var("CS_CONFIG_DIR");
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
                    std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
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
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() - 10).to_string());
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
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
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
            let cli =
                MockCliRunner::with_ok(&signed_in_json("recovered-token", now_epoch_secs() + 3600));
            assert_current_token(
                || {
                    std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
                },
                cli,
                Some("recovered-token"),
                1,
            )
            .await;
            assert_eq!(
                std::env::var("CS_OAUTH_TOKEN").ok().as_deref(),
                Some("recovered-token")
            );
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

    #[tokio::test]
    async fn current_token_returns_synthetic_response_from_env_when_fresh() {
        with_clean_env(|| async {
            std::env::set_var("CS_ONPREM_URL", "https://onprem.example.com/");
            assert_current_token(
                || {
                    std::env::set_var("CS_OAUTH_TOKEN", "fresh-tok");
                    std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
                    std::env::set_var("CS_OAUTH_REFRESH_EXPIRES_AT", "9999999999");
                },
                empty_cli(),
                Some("fresh-tok"),
                0,
            )
            .await;
            // The synthetic response should also carry expiry + api_url built
            // from env, proving `token_response_from_env` ran (not just a stub).
            let manager = AuthManager::new();
            let resp = manager.current_token(&empty_cli()).await.unwrap().unwrap();
            assert_eq!(resp.refresh_token_expires_at, Some(9999999999));
            assert_eq!(
                resp.api_url.as_deref(),
                Some("https://onprem.example.com/api")
            );
            assert!(resp.is_signed_in());
            std::env::remove_var("CS_ONPREM_URL");
        })
        .await;
    }

    #[tokio::test]
    async fn login_errors_when_token_export_unavailable_after_login_without_token() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            let login_json = format!(
                r#"{{"status":"signed_in","access-token":null,"api-url":"https://api.codescene.io/api","expires-at":{}}}"#,
                now_epoch_secs() + 3600
            );
            let signed_out_json = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
            let cli = MockCliRunner::with_responses(vec![
                Ok(login_json),
                Ok(signed_out_json.to_string()),
            ]);
            let result = manager.login(&cli).await;
            assert_eq!(
                result.unwrap_err(),
                "CLI login succeeded but token export remained unavailable".to_string()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn fresh_oauth_credential_treats_whitespace_token_as_missing() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "   ");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            assert!(state::fresh_credential().is_none());
        })
        .await;
    }

    #[tokio::test]
    async fn fresh_oauth_credential_accepts_token_with_unparsable_expiry() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "tok");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", "not-a-number");
            assert!(state::fresh_credential().is_some());
        })
        .await;
    }

    #[tokio::test]
    async fn refresh_reuses_token_written_while_waiting_for_lock() {
        with_clean_env(|| async {
            let guard = crate::config::acquire_write_lock().await;
            let manager = AuthManager::new();
            let cli = empty_cli();
            let mut refresh = std::pin::pin!(manager.resolve_credential(&cli));
            std::future::poll_fn(|cx| match refresh.as_mut().poll(cx) {
                std::task::Poll::Ready(_) => panic!("refresh completed while write lock was held"),
                std::task::Poll::Pending => std::task::Poll::Ready(()),
            })
            .await;
            std::env::set_var("CS_OAUTH_TOKEN", "appeared-token");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            drop(guard);

            let credential = refresh.await.unwrap().unwrap();
            assert_eq!(credential.access_token(), "appeared-token");
        })
        .await;
    }

    #[tokio::test]
    async fn guard_has_fresh_oauth_token_covers_all_false_branches() {
        with_clean_env(|| async {
            let guard = crate::config::acquire_write_lock().await;

            // No token at all.
            assert!(!state::guard_has_fresh_token(&guard));

            // Whitespace-only token.
            std::env::set_var("CS_OAUTH_TOKEN", "   ");
            assert!(!state::guard_has_fresh_token(&guard));

            // Token present, no expiry recorded.
            std::env::set_var("CS_OAUTH_TOKEN", "tok");
            std::env::remove_var("CS_OAUTH_EXPIRES_AT");
            assert!(!state::guard_has_fresh_token(&guard));

            // Token present, unparsable expiry.
            std::env::set_var("CS_OAUTH_EXPIRES_AT", "not-a-number");
            assert!(!state::guard_has_fresh_token(&guard));

            // Token present, expired.
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() - 10).to_string());
            assert!(!state::guard_has_fresh_token(&guard));

            // Token present, fresh.
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            assert!(state::guard_has_fresh_token(&guard));
        })
        .await;
    }

    #[tokio::test]
    async fn persist_response_warns_but_does_not_panic_when_save_fails() {
        with_clean_env(|| async {
            let impossible = if cfg!(windows) {
                r"NUL\impossible"
            } else {
                "/dev/null/impossible"
            };
            std::env::set_var("CS_CONFIG_DIR", impossible);
            let manager = AuthManager::new();
            let cli =
                MockCliRunner::with_ok(&signed_in_json("resilient-tok", now_epoch_secs() + 3600));
            // resolve_credential should still surface the token even though
            // persisting it to the config file fails.
            let result = manager.resolve_credential(&cli).await.unwrap();
            assert_eq!(
                result.map(|c| c.access_token().to_string()),
                Some("resilient-tok".to_string())
            );
            std::env::remove_var("CS_CONFIG_DIR");
        })
        .await;
    }

    #[tokio::test]
    async fn persist_signed_out_warns_but_does_not_panic_when_save_fails() {
        with_clean_env(|| async {
            let impossible = if cfg!(windows) {
                r"NUL\impossible"
            } else {
                "/dev/null/impossible"
            };
            std::env::set_var("CS_CONFIG_DIR", impossible);
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_ok(
                r#"{"status":"signed_out","access-token":null,"api-url":null}"#,
            );
            assert!(!manager.login(&cli).await.unwrap().is_signed_in());
            std::env::remove_var("CS_CONFIG_DIR");
        })
        .await;
    }

    #[tokio::test]
    async fn logout_clears_oauth_and_sets_sentinel() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "oau-to-clear");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            std::env::set_var("CS_OAUTH_REFRESH_EXPIRES_AT", "9999999999");
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_ok(
                r#"{"status":"signed_out","access-token":null,"api-url":null}"#,
            );
            manager.logout(&cli).await.unwrap();
            assert!(std::env::var("CS_OAUTH_TOKEN").is_err());
            assert_eq!(
                std::env::var("CS_OAUTH_EXPIRES_AT").ok().as_deref(),
                Some(SIGNED_OUT_SENTINEL)
            );
            assert!(std::env::var("CS_OAUTH_REFRESH_EXPIRES_AT").is_err());
            let calls = cli.calls();
            let args = calls.lock().unwrap()[0].clone();
            assert_eq!(args[0], "auth");
            assert_eq!(args[1], "logout");
        })
        .await;
    }

    #[tokio::test]
    async fn logout_persists_sentinel_even_when_cli_fails() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "oau-stale");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_err(1, "logout failed");
            let err = manager.logout(&cli).await.unwrap_err();
            assert!(err.contains("logout") || err.contains("failed"), "got: {err}");
            assert!(std::env::var("CS_OAUTH_TOKEN").is_err());
            assert_eq!(
                std::env::var("CS_OAUTH_EXPIRES_AT").ok().as_deref(),
                Some(SIGNED_OUT_SENTINEL)
            );
        })
        .await;
    }

    #[tokio::test]
    async fn login_persists_oauth_account_id_from_cli() {
        with_clean_env(|| async {
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_ok(&signed_in_json_with_account(
                "login-tok",
                now_epoch_secs() + 3600,
                42,
            ));
            let resp = manager.login(&cli).await.unwrap();
            assert_eq!(resp.account_id, Some(42));
            assert_eq!(
                std::env::var("CS_OAUTH_ACCOUNT_ID").ok().as_deref(),
                Some("42")
            );
            // Ordinary login must not auto-pin account_id config.
            assert!(std::env::var("CS_ACCOUNT_ID").is_err());
        })
        .await;
    }

    #[tokio::test]
    async fn switch_account_rejects_pat() {
        with_clean_env(|| async {
            std::env::set_var("CS_ACCESS_TOKEN", "pat");
            let manager = AuthManager::new();
            let err = manager
                .switch_account(&empty_cli(), 99)
                .await
                .unwrap_err();
            assert!(err.contains("CS_ACCESS_TOKEN"), "got: {err}");
        })
        .await;
    }

    #[tokio::test]
    async fn switch_account_noops_when_already_on_account() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "oau-fresh");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            std::env::set_var("CS_OAUTH_ACCOUNT_ID", "42");
            let manager = AuthManager::new();
            let result = manager
                .switch_account(&empty_cli(), 42)
                .await
                .unwrap();
            assert_eq!(
                (result.status, result.account_id),
                (SwitchAccountStatus::AlreadyOnAccount, 42)
            );
            assert_eq!(
                (
                    std::env::var("CS_ACCOUNT_ID").ok(),
                    std::env::var("CS_OAUTH_TOKEN").ok()
                ),
                (Some("42".into()), Some("oau-fresh".into()))
            );
        })
        .await;
    }

    #[tokio::test]
    async fn switch_account_reuses_cli_slot() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "old-tok");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            std::env::set_var("CS_OAUTH_ACCOUNT_ID", "1");
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_ok(&signed_in_json_with_account(
                "slot-tok",
                now_epoch_secs() + 3600,
                99,
            ));
            let result = manager.switch_account(&cli, 99).await.unwrap();
            assert_eq!(
                (result.status, result.account_id),
                (SwitchAccountStatus::ReusedSession, 99)
            );
            assert_eq!(
                (
                    std::env::var("CS_ACCOUNT_ID").ok(),
                    std::env::var("CS_OAUTH_TOKEN").ok(),
                    std::env::var("CS_OAUTH_ACCOUNT_ID").ok()
                ),
                (Some("99".into()), Some("slot-tok".into()), Some("99".into()))
            );
            let calls = cli.calls();
            let locked = calls.lock().unwrap();
            assert_eq!((locked.len(), locked[0][1].as_str()), (1, "token"));
        })
        .await;
    }

    #[tokio::test]
    async fn switch_account_falls_back_to_login() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "old-tok");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            std::env::set_var("CS_OAUTH_ACCOUNT_ID", "1");
            let signed_out = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
            let login_json =
                signed_in_json_with_account("new-tok", now_epoch_secs() + 3600, 77);
            let cli = MockCliRunner::with_responses(vec![
                Ok(signed_out.to_string()),
                Ok(login_json),
            ]);
            let manager = AuthManager::new();
            let result = manager.switch_account(&cli, 77).await.unwrap();
            assert_eq!(result.status, SwitchAccountStatus::SignedIn);
            assert_eq!(result.account_id, 77);
            assert_eq!(
                std::env::var("CS_OAUTH_TOKEN").ok().as_deref(),
                Some("new-tok")
            );
            let call_log = cli.calls();
            let calls = call_log.lock().unwrap();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0][1], "token");
            assert_eq!(calls[1][1], "login");
        })
        .await;
    }

    #[tokio::test]
    async fn switch_account_backfills_legacy_session_then_noops() {
        with_clean_env(|| async {
            // Fresh token, no oauth_account_id, unset account pin (browser-selected).
            std::env::set_var("CS_OAUTH_TOKEN", "legacy-tok");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            let backfill =
                signed_in_json_with_account("legacy-tok", now_epoch_secs() + 3600, 55);
            let cli = MockCliRunner::with_ok(&backfill);
            let manager = AuthManager::new();
            let result = manager.switch_account(&cli, 55).await.unwrap();
            assert_eq!(result.status, SwitchAccountStatus::AlreadyOnAccount);
            // Token cache preserved; pin + session metadata filled from CLI.
            assert_eq!(
                (
                    std::env::var("CS_OAUTH_ACCOUNT_ID").ok(),
                    std::env::var("CS_ACCOUNT_ID").ok(),
                    std::env::var("CS_OAUTH_TOKEN").ok()
                ),
                (
                    Some("55".into()),
                    Some("55".into()),
                    Some("legacy-tok".into())
                )
            );
        })
        .await;
    }

    #[tokio::test]
    async fn logout_clears_oauth_account_id() {
        with_clean_env(|| async {
            std::env::set_var("CS_OAUTH_TOKEN", "oau-to-clear");
            std::env::set_var("CS_OAUTH_EXPIRES_AT", (now_epoch_secs() + 3600).to_string());
            std::env::set_var("CS_OAUTH_ACCOUNT_ID", "42");
            let manager = AuthManager::new();
            let cli = MockCliRunner::with_ok(
                r#"{"status":"signed_out","access-token":null,"api-url":null}"#,
            );
            manager.logout(&cli).await.unwrap();
            assert!(std::env::var("CS_OAUTH_ACCOUNT_ID").is_err());
        })
        .await;
    }
}

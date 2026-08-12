use super::{now_epoch_secs, AuthCredential, CliTokenResponse};

/// Tokens are considered stale this many seconds before their actual `expires-at`.
const TOKEN_EXPIRY_MARGIN_SECS: i64 = 60;

/// Sentinel value for `CS_OAUTH_EXPIRES_AT` indicating the user is signed out.
const SIGNED_OUT_SENTINEL: &str = "0";

pub(super) fn fresh_credential() -> Option<AuthCredential> {
    let vals = crate::config::try_read_env_multi(&[
        "CS_OAUTH_TOKEN",
        "CS_OAUTH_EXPIRES_AT",
        "CS_ONPREM_URL",
    ])?;
    let token = vals[0].as_deref()?.trim().to_string();
    if token.is_empty() {
        let oauth_expires_at = vals[1].clone();
        tracing::info!(oauth_expires_at, "cached OAuth state has no access token");
        return None;
    }
    if let Some(expires_str) = vals[1].as_deref() {
        if let Ok(expires_at) = expires_str.parse::<i64>() {
            if expires_at <= now_epoch_secs() + TOKEN_EXPIRY_MARGIN_SECS {
                tracing::info!(
                    oauth_expires_at = expires_at,
                    "cached OAuth token is expired or within refresh margin"
                );
                return None;
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

pub(super) fn has_fresh_token() -> bool {
    fresh_credential().is_some()
}

pub(super) fn guard_has_fresh_token(guard: &crate::config::ConfigEnvWriteGuard) -> bool {
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

pub(super) fn is_signed_out() -> bool {
    crate::config::try_read_env("CS_OAUTH_EXPIRES_AT").as_deref() == Some(SIGNED_OUT_SENTINEL)
}

pub(super) fn guard_is_signed_out(guard: &crate::config::ConfigEnvWriteGuard) -> bool {
    guard.read_env("CS_OAUTH_EXPIRES_AT").as_deref() == Some(SIGNED_OUT_SENTINEL)
}

/// Account ID belonging to the cached MCP OAuth session, if known.
pub(super) fn cached_oauth_account_id() -> Option<i64> {
    crate::config::try_read_env("CS_OAUTH_ACCOUNT_ID")
        .as_deref()
        .and_then(|s| s.trim().parse().ok())
        .filter(|&id| id > 0)
}

pub(super) fn guard_cached_oauth_account_id(
    guard: &crate::config::ConfigEnvWriteGuard,
) -> Option<i64> {
    guard
        .read_env("CS_OAUTH_ACCOUNT_ID")
        .as_deref()
        .and_then(|s| s.trim().parse().ok())
        .filter(|&id| id > 0)
}

pub(super) fn response_from_env() -> Option<CliTokenResponse> {
    let vals = crate::config::try_read_env_multi(&[
        "CS_OAUTH_TOKEN",
        "CS_OAUTH_EXPIRES_AT",
        "CS_OAUTH_REFRESH_EXPIRES_AT",
        "CS_ONPREM_URL",
        "CS_OAUTH_ACCOUNT_ID",
    ])?;
    let token = vals[0].clone()?;
    let expires_at = vals[1].as_deref().and_then(|s| s.parse::<i64>().ok());
    let refresh_expires_at = vals[2].as_deref().and_then(|s| s.parse::<i64>().ok());
    let api_url = vals[3]
        .as_deref()
        .map(|u| format!("{}/api", u.trim_end_matches('/')));
    let account_id = vals[4]
        .as_deref()
        .and_then(|s| s.trim().parse().ok())
        .filter(|&id| id > 0);
    Some(CliTokenResponse {
        status: "signed_in".into(),
        access_token: Some(token),
        api_url,
        expires_at,
        refresh_token_expires_at: refresh_expires_at,
        account_id,
    })
}

pub(super) fn response_from_guard(guard: &crate::config::ConfigEnvWriteGuard) -> CliTokenResponse {
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
    let account_id = guard_cached_oauth_account_id(guard);
    CliTokenResponse {
        status: "signed_in".into(),
        access_token: token,
        api_url,
        expires_at,
        refresh_token_expires_at: refresh_expires_at,
        account_id,
    }
}

pub(super) fn persist_response(
    guard: &crate::config::ConfigEnvWriteGuard,
    response: &CliTokenResponse,
) {
    let token = response.access_token.as_deref().unwrap_or("").trim();
    let expires_at = response
        .expires_at
        .map(|v| v.to_string())
        .unwrap_or_default();
    let refresh_expires_at = response
        .refresh_token_expires_at
        .map(|v| v.to_string())
        .unwrap_or_default();
    let account_id = response
        .account_id
        .filter(|&id| id > 0)
        .map(|id| id.to_string())
        .unwrap_or_default();
    let entries: &[(&str, &str)] = &[
        ("oauth_token", token),
        ("oauth_expires_at", &expires_at),
        ("oauth_refresh_expires_at", &refresh_expires_at),
        ("oauth_account_id", &account_id),
    ];
    if let Err(e) = guard.write_env_multi(entries) {
        tracing::warn!(error = %e, "failed to persist OAuth state to config file");
    }
    let has_access_token = !token.is_empty();
    tracing::info!(
        has_access_token,
        expires_at = %expires_at,
        refresh_expires_at = %refresh_expires_at,
        oauth_account_id = %account_id,
        "persisted OAuth state to config-backed environment"
    );
}

/// Clear MCP OAuth cache without setting the signed-out sentinel.
///
/// Used by account switching so a subsequent `cs auth token` can still
/// rehydrate from another CLI credential slot.
pub(super) fn clear_oauth_cache(guard: &crate::config::ConfigEnvWriteGuard) {
    write_cleared_oauth_state(guard, "", "cleared MCP OAuth cache without signed-out sentinel");
}

pub(super) fn persist_signed_out(guard: &crate::config::ConfigEnvWriteGuard) {
    write_cleared_oauth_state(
        guard,
        SIGNED_OUT_SENTINEL,
        "persisted signed-out sentinel to config-backed environment",
    );
}

fn write_cleared_oauth_state(
    guard: &crate::config::ConfigEnvWriteGuard,
    expires_at: &str,
    success_message: &str,
) {
    let entries: &[(&str, &str)] = &[
        ("oauth_token", ""),
        ("oauth_expires_at", expires_at),
        ("oauth_refresh_expires_at", ""),
        ("oauth_account_id", ""),
    ];
    if let Err(e) = guard.write_env_multi(entries) {
        tracing::warn!(error = %e, "failed to clear OAuth state in config file");
    }
    tracing::info!("{success_message}");
}

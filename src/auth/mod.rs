mod manager;

pub(crate) use manager::AuthManager;

use serde::Deserialize;

use crate::cli::CliRunner;

const MCP_OAUTH_CLIENT: &str = "mcp";

/// Parsed response from `cs auth token --client mcp --output-format json`
/// and `cs auth login --client mcp --output-format json`.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CliTokenResponse {
    #[serde(rename = "status")]
    pub(crate) status: String,
    #[serde(rename = "access-token")]
    pub(crate) access_token: Option<String>,
    /// Full API URL, e.g. "https://host/api". Used to derive CS_ONPREM_URL.
    #[serde(rename = "api-url")]
    pub(crate) api_url: Option<String>,
    /// Access token expiry in epoch seconds.
    #[serde(rename = "expires-at")]
    pub(crate) expires_at: Option<i64>,
    /// Refresh token expiry in epoch seconds.
    #[allow(dead_code)]
    #[serde(rename = "refresh-token-expires-at")]
    pub(crate) refresh_token_expires_at: Option<i64>,
}

impl CliTokenResponse {
    pub(crate) fn is_signed_in(&self) -> bool {
        self.status == "signed_in"
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AuthCredential {
    /// Static credential from `CS_ACCESS_TOKEN` + optional `CS_ONPREM_URL`.
    /// PATs do not expire in the short-lived sense; this variant is
    /// reconstructed from env vars on each `resolve_credential()` call.
    Configured {
        access_token: String,
        onprem_url: Option<String>,
    },
    /// OAuth credential produced by `AuthManager` from a CLI token response.
    /// Contains the access token and the on-prem base URL (or `None` for cloud).
    /// Expiry is managed by `AuthManager` which guarantees freshness before
    /// producing this value.
    OAuth {
        access_token: String,
        /// On-prem base URL (e.g. `https://host`), or `None` for cloud.
        /// Derived from the CLI's `api-url` field at construction time.
        onprem_url: Option<String>,
    },
}

impl AuthCredential {
    pub(crate) fn access_token(&self) -> &str {
        match self {
            Self::Configured { access_token, .. } | Self::OAuth { access_token, .. } => {
                access_token
            }
        }
    }

    /// Returns the API root URL (e.g. `https://api.codescene.io` for cloud,
    /// or `https://host/api` for on-prem).  Used for HTTP API calls.
    pub(crate) fn api_root(&self) -> Result<String, crate::errors::ApiError> {
        match self {
            Self::Configured { onprem_url, .. } | Self::OAuth { onprem_url, .. } => {
                api_base_from_onprem(onprem_url.as_deref())
            }
        }
    }

    /// Returns the browser-facing root URL for on-prem instances.
    ///
    /// - `Some("https://host")` — on-prem instance.
    /// - `None` — CodeScene cloud (SaaS).
    ///
    /// The `None` vs `Some` distinction carries meaning beyond URL defaulting:
    /// callers such as `codescene_links` use it to select structurally different
    /// URL path schemas (on-prem uses `/{id}/analyses/`, cloud uses
    /// `/projects/{id}/jobs/.../results/`).
    pub(crate) fn web_root(&self) -> Option<String> {
        match self {
            Self::Configured { onprem_url, .. } | Self::OAuth { onprem_url, .. } => {
                onprem_url.clone()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_configured(&self) -> bool {
        matches!(self, Self::Configured { .. })
    }
}

fn api_base_from_onprem(onprem_url: Option<&str>) -> Result<String, crate::errors::ApiError> {
    api_base_from_optional_url(onprem_url, "CS_ONPREM_URL", |url| {
        format!("{}/api", url.trim_end_matches('/'))
    })
}

fn api_base_from_optional_url(
    url: Option<&str>,
    label: &str,
    normalize: impl FnOnce(&str) -> String,
) -> Result<String, crate::errors::ApiError> {
    let Some(url) = url.filter(|url| !url.trim().is_empty()) else {
        return Ok("https://api.codescene.io".to_string());
    };
    crate::config::require_https(label, url)
        .map_err(|e| crate::errors::ApiError::Transport(e.into()))?;
    Ok(normalize(url))
}

pub(crate) fn configured_credential() -> Option<AuthCredential> {
    let vals = crate::config::try_read_env_multi(&["CS_ACCESS_TOKEN", "CS_ONPREM_URL"])?;
    let access_token = vals[0]
        .as_deref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())?;
    let onprem_url = vals[1]
        .as_deref()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty());
    Some(AuthCredential::Configured {
        access_token,
        onprem_url,
    })
}

/// Fallback API root when no `AuthCredential` is available.
///
/// Reads `CS_ONPREM_URL` from the environment to derive the API base
/// (e.g. `https://host/api`), defaulting to `https://api.codescene.io`
/// for cloud.  This is the pre-auth path — once a credential exists,
/// use `credential.api_root()` instead.
pub(crate) fn default_api_root() -> Result<String, crate::errors::ApiError> {
    api_base_from_onprem(read_onprem_url().as_deref())
}

/// Fallback web root when no `AuthCredential` is available.
///
/// Reads `CS_ONPREM_URL` from the environment.  Returns `None` for
/// cloud or if the URL fails HTTPS validation.
pub(crate) fn default_web_root() -> Option<String> {
    let url = read_onprem_url()?;
    if let Err(e) = crate::config::require_https("CS_ONPREM_URL", &url) {
        tracing::warn!("{e}");
        return None;
    }
    Some(url)
}

/// Resolve the web root from an optional credential, falling back to
/// the environment.  Returns `None` for cloud.
///
/// This is the standard pattern for obtaining a browser-facing base URL:
/// use the credential if available, otherwise read `CS_ONPREM_URL`.
/// The `None` return signals cloud — callers may use it to choose between
/// cloud and on-prem URL path structures (see `codescene_links`).
pub(crate) fn resolve_web_root(credential: Option<&AuthCredential>) -> Option<String> {
    credential
        .and_then(AuthCredential::web_root)
        .or_else(default_web_root)
}

/// Read `CS_ONPREM_URL` from the guarded config env, trimmed and normalized.
/// Returns `None` if unset or empty.
fn read_onprem_url() -> Option<String> {
    crate::config::try_read_env("CS_ONPREM_URL")
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn now_epoch_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn sanitized_output_preview(output: &str) -> String {
    let compact = output.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview = ["access-token", "refresh-token"]
        .into_iter()
        .fold(compact, |preview, key| redact_json_string_value(preview, key));
    const MAX_PREVIEW_CHARS: usize = 200;
    if preview.chars().count() <= MAX_PREVIEW_CHARS {
        preview
    } else {
        let truncated = preview.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
        format!("{truncated}...")
    }
}

fn redact_json_string_value(mut text: String, key: &str) -> String {
    let pattern = format!("\"{key}\":\"");
    let mut search_from = 0;
    while let Some(relative_start) = text[search_from..].find(&pattern) {
        let value_start = search_from + relative_start + pattern.len();
        let Some(relative_end) = text[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + relative_end;
        text.replace_range(value_start..value_end, "[redacted]");
        search_from = value_start + "[redacted]".len();
    }
    text
}

/// Fetch the current OAuth token via `cs auth token --client mcp --output-format json`.
/// Returns `None` if the CLI reports not signed in.
/// Returns `Err` on CLI failure or JSON parse error.
pub(crate) async fn fetch_token(
    cli_runner: &dyn CliRunner,
) -> Result<Option<CliTokenResponse>, String> {
    let parsed = run_and_parse_auth(cli_runner, "token").await?;
    if !parsed.is_signed_in() {
        return Ok(None);
    }
    Ok(Some(parsed))
}

/// Run `cs auth login --client mcp --output-format json` (blocking until the
/// browser flow completes or the CLI's built-in 2-minute timeout fires).
/// Returns the parsed response on success.
pub(crate) async fn run_login(cli_runner: &dyn CliRunner) -> Result<CliTokenResponse, String> {
    run_and_parse_auth(cli_runner, "login").await
}

async fn run_and_parse_auth(
    cli_runner: &dyn CliRunner,
    command: &str,
) -> Result<CliTokenResponse, String> {
    let output = run_auth_command(cli_runner, command).await?;
    serde_json::from_str(output.trim()).map_err(|e| {
        tracing::warn!(
            command,
            error = %e,
            output_preview = %sanitized_output_preview(&output),
            "failed to parse auth response"
        );
        format!("Failed to parse {command} response from CLI")
    })
}

async fn run_auth_command(cli_runner: &dyn CliRunner, command: &str) -> Result<String, String> {
    cli_runner
        .run(
            &[
                "auth",
                command,
                "--client",
                MCP_OAUTH_CLIENT,
                "--output-format",
                "json",
            ],
            None,
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, command, error_kind = e.kind(), "auth command failed");
            match &e {
                crate::errors::CliError::NotFound(_) => {
                    "CodeScene CLI not found. Ensure it is installed.".to_string()
                }
                crate::errors::CliError::Io(_) => {
                    format!("Failed to run auth {command}: I/O error")
                }
                _ => format!("Auth {command} failed (CLI exited with an error)"),
            }
        })
}

/// Build an `AuthCredential::OAuth` from a CLI token response.
///
/// Returns `None` if the response has no access token or only whitespace.
/// Derives the on-prem base URL from the `api-url` field at construction
/// time — `None` signals CodeScene cloud.
/// Does NOT mutate environment variables — OAuth tokens live exclusively in
/// the `AuthManager`'s in-memory cache and are threaded explicitly to API calls.
pub(crate) fn credential_from_response(resp: &CliTokenResponse) -> Option<AuthCredential> {
    let access_token = resp.access_token.as_deref()?.trim();
    if access_token.is_empty() {
        return None;
    }
    let onprem_url = resp
        .api_url
        .as_deref()
        .and_then(extract_onprem_from_api_url);
    Some(AuthCredential::OAuth {
        access_token: access_token.to_string(),
        onprem_url,
    })
}

/// Extract the on-prem base URL (`https://host`) from a full API URL
/// (`https://host/api`).  Returns `None` for cloud URLs, signaling that
/// the default `https://api.codescene.io` / `https://codescene.io` should
/// be used.
fn extract_onprem_from_api_url(api_url: &str) -> Option<String> {
    let trimmed = api_url.trim_end_matches('/');
    // Recognize cloud by hostname.
    let host_start = trimmed.find("://")? + 3;
    let host_end = trimmed[host_start..]
        .find('/')
        .map(|i| host_start + i)
        .unwrap_or(trimmed.len());
    let host = &trimmed[host_start..host_end];
    if host == "api.codescene.io" {
        return None;
    }
    // Strip path to get scheme://host
    Some(trimmed[..host_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockCliRunner;

    fn with_env_lock() -> impl Drop {
        crate::config::lock_test_env()
    }

    // -- CliTokenResponse parsing -----------------------------------------

    #[test]
    fn parses_signed_in_response() {
        let json = r#"{"status":"signed_in","access-token":"oau_abc","api-url":"https://test.example.com/api"}"#;
        let resp: CliTokenResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_signed_in());
        assert_eq!(resp.access_token.as_deref(), Some("oau_abc"));
        assert_eq!(
            resp.api_url.as_deref(),
            Some("https://test.example.com/api")
        );
    }

    #[test]
    fn parses_signed_out_response() {
        let json = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
        let resp: CliTokenResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.is_signed_in());
    }

    #[test]
    fn parses_expired_status() {
        let json = r#"{"status":"expired","access-token":null,"api-url":null}"#;
        let resp: CliTokenResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.is_signed_in());
        assert_eq!(resp.status, "expired");
    }

    #[test]
    fn parses_real_token_response() {
        let json = r#"{"refresh-token":"eyJ...","scopes":"cli.access","expires-at":1784044090,"client-id":"cs-cli","api-url":"https://test-env.enterprise.codescene.io/api","user-id":8,"source":"oauth","account-id":null,"status":"signed_in","client-url":"https://test-env.enterprise.codescene.io/oauth2/token","refresh-token-expires-at":1815576430,"storage-file":"/home/user/.codescene/creds.json","access-token":"oau_AAAA"}"#;
        let resp: CliTokenResponse = serde_json::from_str(json).unwrap();
        assert!(resp.is_signed_in());
        assert_eq!(resp.access_token.as_deref(), Some("oau_AAAA"));
        assert_eq!(
            resp.api_url.as_deref(),
            Some("https://test-env.enterprise.codescene.io/api")
        );
        assert_eq!(resp.expires_at, Some(1784044090));
        assert_eq!(resp.refresh_token_expires_at, Some(1815576430));
    }

    #[test]
    fn sanitized_output_preview_redacts_tokens_and_truncates() {
        let raw = format!(
            "{{\"access-token\":\"secret-access\",\"refresh-token\":\"secret-refresh\",\"message\":\"{}\"}}",
            "x".repeat(300)
        );
        let preview = sanitized_output_preview(&raw);
        assert!(!preview.contains("secret-access"), "preview: {preview}");
        assert!(!preview.contains("secret-refresh"), "preview: {preview}");
        assert!(preview.contains("[redacted]"), "preview: {preview}");
        assert!(preview.ends_with("..."), "preview: {preview}");
    }

    #[test]
    fn expiry_fields_optional() {
        let json = r#"{"status":"signed_in","access-token":"oau_x","api-url":null}"#;
        let resp: CliTokenResponse = serde_json::from_str(json).unwrap();
        assert!(resp.expires_at.is_none());
        assert!(resp.refresh_token_expires_at.is_none());
    }

    // -- extract_onprem_from_api_url ----------------------------------------

    #[test]
    fn extract_onprem_strips_path() {
        assert_eq!(
            extract_onprem_from_api_url("https://my.company.com/api"),
            Some("https://my.company.com".to_string())
        );
    }

    #[test]
    fn extract_onprem_returns_none_for_cloud() {
        assert_eq!(
            extract_onprem_from_api_url("https://api.codescene.io/api"),
            None
        );
    }

    #[test]
    fn extract_onprem_no_path_returns_host() {
        // A URL without /api still returns the host (on-prem without path)
        assert_eq!(
            extract_onprem_from_api_url("https://my.company.com"),
            Some("https://my.company.com".to_string())
        );
    }

    // -- credential helpers -----------------------------------------------

    fn make_response(access_token: Option<&str>, api_url: Option<&str>) -> CliTokenResponse {
        CliTokenResponse {
            status: "signed_in".into(),
            access_token: access_token.map(Into::into),
            api_url: api_url.map(Into::into),
            expires_at: None,
            refresh_token_expires_at: None,
        }
    }

    #[test]
    fn credential_from_response_trims_and_preserves_api_url() {
        let resp = make_response(Some("  oau_trimmed  "), Some("https://my.company.com/api"));
        let credential = credential_from_response(&resp).unwrap();
        assert_eq!(credential.access_token(), "oau_trimmed");
        assert_eq!(credential.api_root().unwrap(), "https://my.company.com/api");
        assert_eq!(
            credential.web_root().as_deref(),
            Some("https://my.company.com")
        );
    }

    #[test]
    fn oauth_cloud_api_url_drops_api_suffix() {
        let resp = make_response(Some("oau_cloud"), Some("https://api.codescene.io/api"));
        let credential = credential_from_response(&resp).unwrap();
        assert_eq!(credential.api_root().unwrap(), "https://api.codescene.io");
        assert_eq!(credential.web_root(), None);
    }

    #[test]
    fn credential_from_response_requires_token() {
        assert!(credential_from_response(&make_response(None, None)).is_none());
        assert!(credential_from_response(&make_response(Some("   "), None)).is_none());
    }

    #[test]
    fn configured_credential_reads_user_env_only() {
        let _lock = with_env_lock();
        std::env::set_var("CS_ACCESS_TOKEN", "  pat-token  ");
        std::env::set_var("CS_ONPREM_URL", "https://my.company.com/");
        let credential = configured_credential().unwrap();
        assert!(credential.is_configured());
        assert_eq!(credential.access_token(), "pat-token");
        assert_eq!(credential.api_root().unwrap(), "https://my.company.com/api");
        std::env::remove_var("CS_ACCESS_TOKEN");
        std::env::remove_var("CS_ONPREM_URL");
    }

    // -- fetch_token / run_login ------------------------------------------

    #[tokio::test]
    async fn fetch_token_returns_some_when_signed_in() {
        let json = r#"{"status":"signed_in","access-token":"oau_tok","api-url":"https://api.codescene.io/api"}"#;
        let cli = MockCliRunner::with_ok(json);
        let result = fetch_token(&cli).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().access_token.as_deref(), Some("oau_tok"));
    }

    #[tokio::test]
    async fn fetch_token_returns_none_when_signed_out() {
        let json = r#"{"status":"signed_out","access-token":null,"api-url":null}"#;
        let cli = MockCliRunner::with_ok(json);
        assert!(fetch_token(&cli).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fetch_token_returns_none_when_expired() {
        let json = r#"{"status":"expired","access-token":null,"api-url":null}"#;
        let cli = MockCliRunner::with_ok(json);
        assert!(fetch_token(&cli).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fetch_token_returns_err_on_cli_failure() {
        let cli = MockCliRunner::with_err(1, "connection refused");
        assert!(fetch_token(&cli).await.is_err());
    }

    #[tokio::test]
    async fn fetch_token_returns_err_on_invalid_json() {
        let cli = MockCliRunner::with_ok("not json");
        assert!(fetch_token(&cli).await.is_err());
    }

    #[tokio::test]
    async fn run_login_returns_response_on_success() {
        let json = r#"{"status":"signed_in","access-token":"oau_new","api-url":"https://api.codescene.io/api","expires-at":9999999999,"refresh-token-expires-at":9999999999}"#;
        let cli = MockCliRunner::with_ok(json);
        let result = run_login(&cli).await.unwrap();
        assert!(result.is_signed_in());
        assert_eq!(result.access_token.as_deref(), Some("oau_new"));
    }

    #[tokio::test]
    async fn run_login_returns_err_on_cli_failure() {
        let cli = MockCliRunner::with_err(1, "timeout waiting for browser");
        assert!(run_login(&cli).await.is_err());
    }

    #[tokio::test]
    async fn run_login_returns_err_on_invalid_json() {
        let cli = MockCliRunner::with_ok("Browser opened but no json returned");
        let err = run_login(&cli).await.unwrap_err();
        assert!(err.contains("Failed to parse"));
    }

    #[tokio::test]
    async fn run_login_parse_error_does_not_echo_token() {
        let cli = MockCliRunner::with_ok(
            r#"{"status":"signed_in","access-token":"secret-token"} trailing"#,
        );
        let err = run_login(&cli).await.unwrap_err();
        assert!(!err.contains("secret-token"));
    }
}

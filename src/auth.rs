use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Deserialize;

use crate::cli::CliRunner;
use crate::config::{self, ConfigData};

const REFRESH_BUFFER_MS: i64 = 60_000;
const MCP_OAUTH_CLIENT: &str = "mcp";

#[derive(Debug, Clone)]
pub(crate) struct CachedCredential {
    pub(crate) access_token: String,
    pub(crate) expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CliTokenResponse {
    state: String,
    #[serde(rename = "access-token")]
    access_token: Option<String>,
    #[serde(rename = "expires-at")]
    expires_at: Option<i64>,
    #[serde(rename = "onprem-url")]
    onprem_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AuthError {
    #[error("CLI auth command failed: {0}")]
    Cli(#[from] crate::errors::CliError),

    #[error("Failed to parse CLI auth token JSON: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Not signed in ({state})")]
    NotSignedIn { state: String },
}

/// Resolves OAuth credentials via the embedded CLI (`cs auth token`).
/// Static tokens from env or config file are left untouched.
#[derive(Clone)]
pub(crate) struct CredentialResolver {
    cli_runner: Arc<dyn CliRunner>,
    oauth_managed: Arc<AtomicBool>,
    cache: Arc<Mutex<Option<CachedCredential>>>,
}

pub(crate) fn access_token_from_env() -> Option<String> {
    std::env::var("CS_ACCESS_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

pub(crate) fn access_token_configured(config: &ConfigData) -> bool {
    if access_token_from_env().is_some() {
        return true;
    }
    config::find_option("access_token")
        .and_then(|opt| config::get_effective(opt, config))
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false)
}

impl CredentialResolver {
    pub(crate) fn new(cli_runner: Arc<dyn CliRunner>, oauth_managed: bool) -> Self {
        Self {
            cli_runner,
            oauth_managed: Arc::new(AtomicBool::new(oauth_managed)),
            cache: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn oauth_managed(&self) -> bool {
        self.oauth_managed.load(Ordering::Relaxed)
    }

    pub(crate) async fn bootstrap_with_config(&self, config: &ConfigData) -> Result<(), AuthError> {
        if access_token_configured(config) {
            self.oauth_managed.store(false, Ordering::Relaxed);
            return Ok(());
        }
        self.oauth_managed.store(true, Ordering::Relaxed);
        self.refresh_from_cli().await
    }

    pub(crate) async fn ensure_fresh(&self) -> Result<(), AuthError> {
        if access_token_from_env().is_some() {
            self.oauth_managed.store(false, Ordering::Relaxed);
            return Ok(());
        }
        if !self.oauth_managed() {
            return Ok(());
        }
        let needs_refresh = self
            .cache
            .lock()
            .map(|guard| {
                guard.as_ref().map_or(true, |cred| token_near_expiry(cred.expires_at))
            })
            .unwrap_or(true);
        if needs_refresh {
            self.refresh_from_cli().await?;
        }
        Ok(())
    }

    /// Re-resolve OAuth after HTTP 401. Returns true when a new token was applied.
    pub(crate) async fn on_unauthorized(&self) -> Result<bool, AuthError> {
        if !self.oauth_managed() {
            return Ok(false);
        }
        {
            let mut guard = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            *guard = None;
        }
        self.refresh_from_cli().await?;
        Ok(true)
    }

    pub(crate) async fn auth_status_json(&self) -> Result<String, AuthError> {
        Ok(self
            .cli_runner
            .run(
                &[
                    "auth",
                    "status",
                    "--client",
                    MCP_OAUTH_CLIENT,
                    "--output-format",
                    "json",
                ],
                None,
            )
            .await?)
    }

    async fn refresh_from_cli(&self) -> Result<(), AuthError> {
        let output = self
            .cli_runner
            .run(
                &[
                    "auth",
                    "token",
                    "--client",
                    MCP_OAUTH_CLIENT,
                    "--output-format",
                    "json",
                ],
                None,
            )
            .await?;
        let parsed: CliTokenResponse = serde_json::from_str(output.trim())?;
        if parsed.state != "signed_in" {
            return Err(AuthError::NotSignedIn {
                state: parsed.state,
            });
        }
        let access_token = parsed
            .access_token
            .filter(|t| !t.trim().is_empty())
            .ok_or(AuthError::NotSignedIn {
                state: "signed_out".to_string(),
            })?;
        apply_token_to_env(&access_token, parsed.onprem_url.as_deref());
        let cached = CachedCredential {
            access_token,
            expires_at: parsed.expires_at,
        };
        if let Ok(mut guard) = self.cache.lock() {
            *guard = Some(cached);
        }
        Ok(())
    }
}

fn token_near_expiry(expires_at: Option<i64>) -> bool {
    expires_at.map_or(false, |exp| exp <= now_ms() + REFRESH_BUFFER_MS)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn apply_token_to_env(access_token: &str, onprem_url: Option<&str>) {
    std::env::set_var("CS_ACCESS_TOKEN", access_token.trim());
    if std::env::var("CS_ONPREM_URL").is_err() {
        if let Some(url) = onprem_url.filter(|u| !u.trim().is_empty()) {
            std::env::set_var("CS_ONPREM_URL", url.trim());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::Path;
    use std::sync::Mutex;

    struct MockCliRunner {
        responses: Mutex<Vec<Result<String, crate::errors::CliError>>>,
    }

    impl MockCliRunner {
        fn with_responses(responses: Vec<Result<String, crate::errors::CliError>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
            })
        }
    }

    #[async_trait]
    impl CliRunner for MockCliRunner {
        async fn run(
            &self,
            _args: &[&str],
            _working_dir: Option<&Path>,
        ) -> Result<String, crate::errors::CliError> {
            self.responses
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_else(|| Ok(String::new()))
        }
    }

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        config::lock_test_env()
    }

    fn clear_token_env() {
        std::env::remove_var("CS_ACCESS_TOKEN");
        std::env::remove_var("CS_ONPREM_URL");
    }

    #[test]
    fn token_near_expiry_within_buffer() {
        let soon = now_ms() + 30_000;
        assert!(token_near_expiry(Some(soon)));
        let later = now_ms() + 120_000;
        assert!(!token_near_expiry(Some(later)));
    }

    #[tokio::test]
    async fn refresh_from_cli_applies_env_and_cache() {
        let _lock = lock_env();
        clear_token_env();

        let json = r#"{"state":"signed_in","access-token":"oat_test","expires-at":9999999999999,"onprem-url":"https://onprem.example.com"}"#;
        let cli = MockCliRunner::with_responses(vec![Ok(json.to_string())]);
        let resolver = CredentialResolver::new(cli, true);
        resolver.refresh_from_cli().await.unwrap();

        assert_eq!(
            access_token_from_env().as_deref(),
            Some("oat_test")
        );
        assert_eq!(
            std::env::var("CS_ONPREM_URL").ok().as_deref(),
            Some("https://onprem.example.com")
        );
        let cached = resolver.cache.lock().unwrap();
        assert_eq!(cached.as_ref().unwrap().access_token, "oat_test");
        clear_token_env();
    }

    #[tokio::test]
    async fn bootstrap_skips_when_env_token_present() {
        let _lock = lock_env();
        std::env::set_var("CS_ACCESS_TOKEN", "static-token");
        let cli = MockCliRunner::with_responses(vec![]);
        let resolver = CredentialResolver::new(cli, true);
        let config = ConfigData::default();
        resolver.bootstrap_with_config(&config).await.unwrap();
        assert!(!resolver.oauth_managed());
        clear_token_env();
    }

    #[tokio::test]
    async fn on_unauthorized_refreshes_oauth_token() {
        let _lock = lock_env();
        clear_token_env();

        let json = r#"{"state":"signed_in","access-token":"oat_new","expires-at":9999999999999}"#;
        let cli = MockCliRunner::with_responses(vec![Ok(json.to_string())]);
        let resolver = CredentialResolver::new(cli, true);
        let refreshed = resolver.on_unauthorized().await.unwrap();
        assert!(refreshed);
        assert_eq!(access_token_from_env().as_deref(), Some("oat_new"));
        clear_token_env();
    }
}

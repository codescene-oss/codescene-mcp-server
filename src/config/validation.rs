use super::options::{ConfigOption, OPTIONS};

/// Keys that represent URL values and must use HTTPS.
const URL_KEYS: &[&str] = &["onprem_url"];

/// Localhost hosts that are exempt from the HTTPS requirement.
/// `host.docker.internal` is included because it is the standard way for a
/// process running inside a Docker container to reach services on its host
/// machine (e.g. our e2e tests' fake servers) — it is not a publicly
/// routable address, so it carries the same trust profile as `localhost`.
const LOCALHOST_HOSTS: &[&str] = &["localhost", "127.0.0.1", "0.0.0.0", "host.docker.internal"];

/// Check whether a URL (after stripping the scheme) points to a localhost address.
fn is_localhost_url(url: &str) -> bool {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);

    LOCALHOST_HOSTS.iter().any(|host| {
        without_scheme == *host
            || without_scheme.starts_with(&format!("{host}:"))
            || without_scheme.starts_with(&format!("{host}/"))
    })
}

/// Returns true if the URL is acceptable: empty, HTTPS, or localhost.
fn is_allowed_url(url: &str) -> bool {
    url.is_empty() || url.starts_with("https://") || is_localhost_url(url)
}

/// Validate that a URL value uses the HTTPS scheme.
/// Returns `Ok(())` for valid HTTPS URLs, or `Err` with a user-facing message.
/// Localhost addresses (localhost, 127.0.0.1, 0.0.0.0, host.docker.internal) are exempt.
pub fn validate_https_url(key: &str, url: &str) -> Result<(), String> {
    if !URL_KEYS.contains(&key) {
        return Ok(());
    }
    let trimmed = url.trim();
    if is_allowed_url(trimmed) {
        return Ok(());
    }
    Err(format!(
        "The URL for '{key}' must use HTTPS (got: {trimmed}). \
         Please provide a URL starting with https://"
    ))
}

/// Validate that a raw URL string uses HTTPS. Used for env vars not managed
/// through the config system (e.g. CS_TRACKING_URL).
/// Localhost addresses (localhost, 127.0.0.1, 0.0.0.0, host.docker.internal) are exempt.
pub fn require_https(env_var: &str, url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if is_allowed_url(trimmed) {
        return Ok(());
    }
    Err(format!(
        "{env_var} must use HTTPS (got: {trimmed}). \
         Please provide a URL starting with https://"
    ))
}

/// Mask sensitive values for display.
/// Shows a short prefix to identify the token kind, without revealing entropy.
const SENSITIVE_PREFIX_LENGTH: usize = 4;

pub fn mask_if_sensitive(option: &ConfigOption, value: &str) -> String {
    if option.sensitive && !value.is_empty() {
        if value.len() <= SENSITIVE_PREFIX_LENGTH {
            return "***".to_string();
        }
        format!("{}...", &value[..SENSITIVE_PREFIX_LENGTH])
    } else {
        value.to_string()
    }
}

/// Returns the env var names of all sensitive config options (tokens, keys, etc.).
/// Used to scrub secrets from child process environments.
pub fn sensitive_env_vars() -> Vec<&'static str> {
    OPTIONS
        .iter()
        .filter(|o| o.sensitive)
        .map(|o| o.env_var)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::find_option;

    #[test]
    fn validate_https_url_accepts_https() {
        assert!(validate_https_url("onprem_url", "https://example.com").is_ok());
    }

    #[test]
    fn validate_https_url_rejects_http() {
        let result = validate_https_url("onprem_url", "http://example.com");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTPS"));
    }

    #[test]
    fn validate_https_url_accepts_empty() {
        assert!(validate_https_url("onprem_url", "").is_ok());
        assert!(validate_https_url("onprem_url", "  ").is_ok());
    }

    #[test]
    fn validate_https_url_ignores_non_url_keys() {
        assert!(validate_https_url("access_token", "http://not-a-url").is_ok());
    }

    #[test]
    fn validate_https_url_rejects_bare_domain() {
        let result = validate_https_url("onprem_url", "example.com");
        assert!(result.is_err());
    }

    #[test]
    fn require_https_accepts_https() {
        assert!(require_https("CS_TRACKING_URL", "https://track.example.com").is_ok());
    }

    #[test]
    fn require_https_rejects_http() {
        let result = require_https("CS_TRACKING_URL", "http://track.example.com");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HTTPS"));
    }

    #[test]
    fn require_https_accepts_empty() {
        assert!(require_https("CS_TRACKING_URL", "").is_ok());
        assert!(require_https("CS_TRACKING_URL", "  ").is_ok());
    }

    #[test]
    fn require_https_includes_env_var_in_error() {
        let result = require_https("CS_TRACKING_URL", "http://bad.com");
        assert!(result.unwrap_err().contains("CS_TRACKING_URL"));
    }

    // --- Localhost exemption tests ---

    #[test]
    fn validate_https_url_accepts_http_localhost() {
        assert!(validate_https_url("onprem_url", "http://localhost").is_ok());
        assert!(validate_https_url("onprem_url", "http://localhost:3000").is_ok());
        assert!(validate_https_url("onprem_url", "http://localhost/api").is_ok());
    }

    #[test]
    fn validate_https_url_accepts_http_127_0_0_1() {
        assert!(validate_https_url("onprem_url", "http://127.0.0.1").is_ok());
        assert!(validate_https_url("onprem_url", "http://127.0.0.1:8080").is_ok());
        assert!(validate_https_url("onprem_url", "http://127.0.0.1/path").is_ok());
    }

    #[test]
    fn validate_https_url_accepts_http_0_0_0_0() {
        assert!(validate_https_url("onprem_url", "http://0.0.0.0").is_ok());
        assert!(validate_https_url("onprem_url", "http://0.0.0.0:9090").is_ok());
    }

    #[test]
    fn require_https_accepts_http_localhost() {
        assert!(require_https("CS_ONPREM_URL", "http://localhost").is_ok());
        assert!(require_https("CS_ONPREM_URL", "http://localhost:3000").is_ok());
        assert!(require_https("CS_TRACKING_URL", "http://localhost/api").is_ok());
    }

    #[test]
    fn require_https_accepts_http_127_0_0_1() {
        assert!(require_https("CS_ONPREM_URL", "http://127.0.0.1").is_ok());
        assert!(require_https("CS_ONPREM_URL", "http://127.0.0.1:8080").is_ok());
    }

    #[test]
    fn require_https_accepts_http_0_0_0_0() {
        assert!(require_https("CS_TRACKING_URL", "http://0.0.0.0").is_ok());
        assert!(require_https("CS_TRACKING_URL", "http://0.0.0.0:9090").is_ok());
    }

    #[test]
    fn validate_https_url_accepts_http_host_docker_internal() {
        assert!(validate_https_url("onprem_url", "http://host.docker.internal").is_ok());
        assert!(validate_https_url("onprem_url", "http://host.docker.internal:8080").is_ok());
    }

    #[test]
    fn require_https_accepts_http_host_docker_internal() {
        assert!(require_https("CS_TRACKING_URL", "http://host.docker.internal").is_ok());
        assert!(require_https("CS_TRACKING_URL", "http://host.docker.internal:9090").is_ok());
    }

    #[test]
    fn validate_https_url_still_rejects_http_non_localhost() {
        assert!(validate_https_url("onprem_url", "http://example.com").is_err());
        assert!(validate_https_url("onprem_url", "http://not-localhost.com").is_err());
    }

    #[test]
    fn require_https_still_rejects_http_non_localhost() {
        assert!(require_https("CS_ONPREM_URL", "http://example.com").is_err());
        assert!(require_https("CS_TRACKING_URL", "http://not-localhost.com").is_err());
    }

    #[test]
    fn mask_sensitive_long_value() {
        let opt = find_option("access_token").unwrap();
        let masked = mask_if_sensitive(opt, "my-super-secret-token-value");
        assert_eq!(masked, "my-s...");
    }

    #[test]
    fn mask_sensitive_short_value() {
        let opt = find_option("access_token").unwrap();
        let masked = mask_if_sensitive(opt, "abcd");
        assert_eq!(masked, "***");
    }

    #[test]
    fn mask_sensitive_empty_value() {
        let opt = find_option("access_token").unwrap();
        let masked = mask_if_sensitive(opt, "");
        assert_eq!(masked, "");
    }

    #[test]
    fn mask_non_sensitive_passes_through() {
        let opt = find_option("onprem_url").unwrap();
        let masked = mask_if_sensitive(opt, "https://example.com");
        assert_eq!(masked, "https://example.com");
    }

    #[test]
    fn sensitive_env_vars_includes_tokens() {
        let vars = sensitive_env_vars();
        assert!(
            vars.contains(&"CS_ACCESS_TOKEN"),
            "should include CS_ACCESS_TOKEN"
        );
    }

    #[test]
    fn sensitive_env_vars_excludes_non_sensitive() {
        let vars = sensitive_env_vars();
        assert!(
            !vars.contains(&"CS_ONPREM_URL"),
            "should not include non-sensitive CS_ONPREM_URL"
        );
    }
}

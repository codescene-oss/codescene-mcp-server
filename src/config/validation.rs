use super::options::{ConfigOption, OPTIONS};

/// Keys that represent URL values and must use HTTPS.
const URL_KEYS: &[&str] = &["onprem_url"];

/// Validate that a URL value uses the HTTPS scheme.
/// Returns `Ok(())` for valid HTTPS URLs, or `Err` with a user-facing message.
pub fn validate_https_url(key: &str, url: &str) -> Result<(), String> {
    if !URL_KEYS.contains(&key) {
        return Ok(());
    }
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if trimmed.starts_with("https://") {
        return Ok(());
    }
    Err(format!(
        "The URL for '{key}' must use HTTPS (got: {trimmed}). \
         Please provide a URL starting with https://"
    ))
}

/// Validate that a raw URL string uses HTTPS. Used for env vars not managed
/// through the config system (e.g. CS_TRACKING_URL).
pub fn require_https(env_var: &str, url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.starts_with("https://") {
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

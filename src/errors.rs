#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("CS CLI exited with code {code}: {stderr}")]
    NonZeroExit { code: i32, stderr: String },

    #[error("CS CLI not found: {0}")]
    NotFound(String),

    #[error("Failed to run CS CLI: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error(
        "Access token is invalid or expired.\n\n\
         Sign in with OAuth: cs auth login --client mcp\n\n\
         Or update your access token using one of these methods:\n\
         1. Use the `set_config` tool: set_config(key=\"access_token\", value=\"your-token\")\n\
         2. Set the CS_ACCESS_TOKEN environment variable in your MCP client configuration\n\n\
         To get a new Personal Access Token, see:\n\
         https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/getting-a-personal-access-token.md"
    )]
    LicenseCheckFailed { stderr: String },
}

impl CliError {
    /// Returns a safe, fixed label for telemetry (no sensitive data).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NonZeroExit { .. } => "non_zero_exit",
            Self::NotFound(_) => "not_found",
            Self::Io(_) => "io",
            Self::InvalidInput(_) => "invalid_input",
            Self::LicenseCheckFailed { .. } => "license_check_failed",
        }
    }
}

impl ApiError {
    /// Returns a safe, fixed label for telemetry (no sensitive data).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Http(_) => "http",
            Self::Transport(_) => "transport",
            Self::Status { .. } => "status",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Config I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("Invalid license format")]
    InvalidFormat,

    #[error("Invalid base64 encoding: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("Invalid signature")]
    InvalidSignature,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP transport error: {0}")]
    Transport(String),

    #[error("API error {status}: {body}")]
    Status { status: u16, body: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_error_non_zero_exit_display() {
        let err = CliError::NonZeroExit {
            code: 1,
            stderr: "something broke".into(),
        };
        assert_eq!(
            err.to_string(),
            "CS CLI exited with code 1: something broke"
        );
    }

    #[test]
    fn cli_error_not_found_display() {
        let err = CliError::NotFound("/usr/bin/cs".into());
        assert_eq!(err.to_string(), "CS CLI not found: /usr/bin/cs");
    }

    #[test]
    fn cli_error_io_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let err = CliError::Io(io_err);
        assert!(err.to_string().starts_with("Failed to run CS CLI:"));
    }

    #[test]
    fn cli_error_license_check_failed_display() {
        let err = CliError::LicenseCheckFailed { stderr: "License check failed".into() };
        assert!(err
            .to_string()
            .contains("Access token is invalid or expired"));
        assert!(err.to_string().contains("set_config"));
    }

    #[test]
    fn cli_error_invalid_input_display() {
        let err = CliError::InvalidInput("bad value".into());
        assert_eq!(err.to_string(), "Invalid input: bad value");
    }

    #[test]
    fn config_error_io_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = ConfigError::Io(io_err);
        assert!(err.to_string().starts_with("Config I/O error:"));
    }

    #[test]
    fn config_error_parse_display() {
        let json_err =
            serde_json::from_str::<serde_json::Value>("{{bad}}").expect_err("should fail");
        let err = ConfigError::Parse(json_err);
        assert!(err.to_string().starts_with("Config parse error:"));
    }

    #[test]
    fn license_error_invalid_format_display() {
        assert_eq!(
            LicenseError::InvalidFormat.to_string(),
            "Invalid license format"
        );
    }

    #[test]
    fn license_error_invalid_signature_display() {
        assert_eq!(
            LicenseError::InvalidSignature.to_string(),
            "Invalid signature"
        );
    }

    #[test]
    fn api_error_transport_display() {
        let err = ApiError::Transport("connection refused".into());
        assert_eq!(err.to_string(), "HTTP transport error: connection refused");
    }

    #[test]
    fn api_error_status_display() {
        let err = ApiError::Status {
            status: 404,
            body: "not found".into(),
        };
        assert_eq!(err.to_string(), "API error 404: not found");
    }

    #[test]
    fn cli_error_kind_returns_safe_labels() {
        assert_eq!(
            CliError::NonZeroExit {
                code: 1,
                stderr: "secret token=abc123 /home/user/.ssh/id_rsa".into()
            }
            .kind(),
            "non_zero_exit"
        );
        assert_eq!(CliError::NotFound("/usr/bin/cs".into()).kind(), "not_found");
        assert_eq!(
            CliError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "x")).kind(),
            "io"
        );
        assert_eq!(
            CliError::InvalidInput("bad".into()).kind(),
            "invalid_input"
        );
        assert_eq!(CliError::LicenseCheckFailed { stderr: String::new() }.kind(), "license_check_failed");
    }

    #[test]
    fn cli_error_kind_never_contains_sensitive_data() {
        let err = CliError::NonZeroExit {
            code: 1,
            stderr: "Bearer eyJhbGciOi password=secret /Users/me/.config/token".into(),
        };
        let kind = err.kind();
        assert!(!kind.contains("Bearer"));
        assert!(!kind.contains("password"));
        assert!(!kind.contains("/Users"));
        assert!(!kind.contains("secret"));
    }

    #[test]
    fn api_error_kind_returns_safe_labels() {
        assert_eq!(
            ApiError::Transport("connection refused".into()).kind(),
            "transport"
        );
        assert_eq!(
            ApiError::Status {
                status: 401,
                body: "Bearer token invalid".into()
            }
            .kind(),
            "status"
        );
    }
}

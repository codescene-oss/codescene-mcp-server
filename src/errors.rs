#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("CS CLI exited with code {code}: {stderr}")]
    NonZeroExit { code: i32, stderr: String },

    #[error("CS CLI not found: {0}")]
    NotFound(String),

    #[error("Failed to run CS CLI: {0}")]
    Io(#[from] std::io::Error),

    #[error(
        "Access token is invalid or expired.\n\n\
         Please update your access token using one of these methods:\n\
         1. Use the `set_config` tool: set_config(key=\"access_token\", value=\"your-token\")\n\
         2. Set the CS_ACCESS_TOKEN environment variable in your MCP client configuration\n\n\
         To get a new Access Token, see:\n\
         https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/getting-a-personal-access-token.md"
    )]
    LicenseCheckFailed,
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
        let err = CliError::LicenseCheckFailed;
        assert!(err
            .to_string()
            .contains("Access token is invalid or expired"));
        assert!(err.to_string().contains("set_config"));
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
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("CS CLI exited with code {code}: {stderr}")]
    NonZeroExit { code: i32, stderr: String },

    #[error("CS CLI not found: {0}")]
    NotFound(String),

    #[error("Failed to run CS CLI: {0}")]
    Io(#[from] std::io::Error),
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

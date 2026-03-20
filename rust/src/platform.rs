/// Platform detection — mirrors Python's `platform_details.py`.
///
/// Simplified for Rust: no Windows Git PATH hacks, no PEM-to-PKCS12
/// truststore creation (handled differently in Rust via reqwest/rustls).

/// Returns the name of the CS CLI binary for the current platform.
#[allow(dead_code)]
pub fn cli_binary_name() -> &'static str {
    if cfg!(windows) { "cs.exe" } else { "cs" }
}

/// Returns `true` if running on Windows.
#[allow(dead_code)]
pub fn is_windows() -> bool {
    cfg!(windows)
}

/// Returns the current platform name for analytics.
#[allow(dead_code)]
pub fn platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

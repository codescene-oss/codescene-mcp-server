/// Environment detection — mirrors Python's `environment.py`.
///
/// Returns `"docker"` when `CS_MOUNT_PATH` is set, `"binary"` otherwise.
/// The Python version also has `"source"` and `"nuitka"`, but the Rust
/// binary is always distributed as a compiled binary.

use std::sync::OnceLock;

static ENVIRONMENT: OnceLock<&'static str> = OnceLock::new();

/// Detect and cache the current runtime environment.
pub fn detect() -> &'static str {
    ENVIRONMENT.get_or_init(|| {
        if std::env::var("CS_MOUNT_PATH").is_ok() {
            "docker"
        } else {
            "binary"
        }
    })
}

/// Returns `true` when running inside a Docker container.
pub fn is_docker() -> bool {
    detect() == "docker"
}

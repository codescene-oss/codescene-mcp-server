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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_known_value() {
        let env = detect();
        assert!(env == "binary" || env == "docker");
    }

    #[test]
    fn is_docker_consistent_with_detect() {
        assert_eq!(is_docker(), detect() == "docker");
    }

    #[test]
    fn detect_is_stable() {
        // OnceLock means repeated calls return the same value
        let first = detect();
        let second = detect();
        assert_eq!(first, second);
    }
}

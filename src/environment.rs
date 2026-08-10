use std::sync::OnceLock;

static ENVIRONMENT: OnceLock<&'static str> = OnceLock::new();

pub fn detect() -> &'static str {
    ENVIRONMENT.get_or_init(|| {
        if std::env::var("CS_MOUNT_PATH").is_ok() {
            "docker"
        } else {
            "binary"
        }
    })
}

pub fn is_docker() -> bool {
    #[cfg(test)]
    if let Some(forced) = test_override::docker_override() {
        return forced;
    }
    detect() == "docker"
}

#[cfg(test)]
mod test_override {
    use std::cell::Cell;

    thread_local! {
        static DOCKER_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
    }

    pub(super) fn docker_override() -> Option<bool> {
        DOCKER_OVERRIDE.with(|c| c.get())
    }

    /// Force [`super::is_docker`] for the duration of the returned guard.
    pub struct DockerOverrideGuard;

    impl Drop for DockerOverrideGuard {
        fn drop(&mut self) {
            DOCKER_OVERRIDE.with(|c| c.set(None));
        }
    }

    pub fn force_docker(is_docker: bool) -> DockerOverrideGuard {
        DOCKER_OVERRIDE.with(|c| c.set(Some(is_docker)));
        DockerOverrideGuard
    }
}

#[cfg(test)]
pub use test_override::force_docker;

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

    #[test]
    fn force_docker_overrides_detect() {
        let _guard = force_docker(true);
        assert!(is_docker());
    }

    #[test]
    fn force_docker_false_overrides_detect() {
        let _guard = force_docker(false);
        assert!(!is_docker());
    }
}

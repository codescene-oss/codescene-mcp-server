#[allow(dead_code)]
pub fn cli_binary_name() -> &'static str {
    if cfg!(windows) { "cs.exe" } else { "cs" }
}

#[allow(dead_code)]
pub fn is_windows() -> bool {
    cfg!(windows)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_binary_name_on_current_platform() {
        let name = cli_binary_name();
        if cfg!(windows) {
            assert_eq!(name, "cs.exe");
        } else {
            assert_eq!(name, "cs");
        }
    }

    #[test]
    fn is_windows_matches_cfg() {
        assert_eq!(is_windows(), cfg!(windows));
    }

    #[test]
    fn platform_name_is_known() {
        let name = platform_name();
        assert!(
            ["macos", "linux", "windows", "unknown"].contains(&name),
            "unexpected platform: {name}"
        );
        // On macOS (where this test runs):
        if cfg!(target_os = "macos") {
            assert_eq!(name, "macos");
        }
    }
}

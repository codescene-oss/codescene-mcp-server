use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config;

/// Initialise tracing with stderr output and optional file logging.
///
/// Returns the non-blocking file-appender guard when file logging is
/// active.  The guard must be held for the lifetime of the program so
/// that buffered log entries are flushed on shutdown.
pub(crate) fn init_tracing(
    config_data: &config::ConfigData,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let env_filter = EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into());
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false);

    let retention_days = config::log_retention_days(config_data);
    if let Some(log_dir) = prepare_log_dir(&config::log_dir(), retention_days) {
        let file_appender = tracing_appender::rolling::daily(&log_dir, "mcp.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false);
        tracing_subscriber::registry()
            .with(env_filter)
            .with(stderr_layer)
            .with(file_layer)
            .init();
        return Some(guard);
    }

    // Stderr-only: file logging disabled or log directory not writable
    // (e.g. non-root container with a read-only config path).
    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .init();
    None
}

/// Create the log directory and prune old logs when file logging is enabled.
///
/// Returns `None` when file logging is disabled (`retention_days == 0`) or the
/// directory cannot be created.
fn prepare_log_dir(log_dir: &Path, retention_days: u32) -> Option<PathBuf> {
    if retention_days == 0 {
        return None;
    }
    std::fs::create_dir_all(log_dir).ok()?;
    cleanup_old_logs(log_dir, retention_days);
    Some(log_dir.to_path_buf())
}

/// Remove log files older than `retention_days` from the given directory.
fn cleanup_old_logs(log_dir: &Path, retention_days: u32) {
    let cutoff = SystemTime::now() - Duration::from_secs(u64::from(retention_days) * 24 * 60 * 60);
    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified < cutoff {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn write_log(dir: &Path, name: &str, age: Duration) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
        path
    }

    #[test]
    fn cleanup_removes_only_logs_older_than_retention() {
        let dir = tempfile::tempdir().unwrap();
        let old = write_log(dir.path(), "old.log", 10 * DAY);
        let recent = write_log(dir.path(), "recent.log", DAY);

        cleanup_old_logs(dir.path(), 7);

        assert!(!old.exists());
        assert!(recent.exists());
    }

    #[test]
    fn cleanup_ignores_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        cleanup_old_logs(&dir.path().join("missing"), 7);
    }

    #[test]
    fn prepare_log_dir_disabled_when_retention_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let log_dir = dir.path().join("logs");

        assert_eq!(prepare_log_dir(&log_dir, 0), None);
        assert!(!log_dir.exists());
    }

    #[test]
    fn prepare_log_dir_creates_directory_and_prunes_old_logs() {
        let dir = tempfile::tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        let old = write_log(&log_dir, "old.log", 10 * DAY);

        assert_eq!(prepare_log_dir(&log_dir, 7), Some(log_dir.clone()));
        assert!(!old.exists());
    }

    #[test]
    fn prepare_log_dir_returns_none_when_directory_cannot_be_created() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").unwrap();

        assert_eq!(prepare_log_dir(&blocker.join("logs"), 7), None);
    }
}

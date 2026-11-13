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
    if retention_days > 0 {
        let log_dir = config::log_dir();
        if let Ok(()) = std::fs::create_dir_all(&log_dir) {
            cleanup_old_logs(&log_dir, retention_days);
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
    }

    // Stderr-only: file logging disabled or log directory not writable
    // (e.g. non-root container with a read-only config path).
    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .init();
    None
}

/// Remove log files older than `retention_days` from the given directory.
pub(crate) fn cleanup_old_logs(log_dir: &std::path::Path, retention_days: u32) {
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(u64::from(retention_days) * 24 * 60 * 60);
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

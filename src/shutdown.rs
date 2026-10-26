use std::time::Duration;

use rmcp::transport::IntoTransport;
use tokio_util::task::TaskTracker;

use crate::{serve_or_handle_disconnect, transport, CodeSceneServer};

const SERVICE_CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
const TELEMETRY_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct Termination {
    #[cfg(unix)]
    signal: tokio::signal::unix::Signal,
}

impl Termination {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            #[cfg(unix)]
            signal: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?,
        })
    }

    async fn recv(&mut self) {
        #[cfg(unix)]
        self.signal.recv().await;
        #[cfg(not(unix))]
        std::future::pending::<()>().await;
        tracing::info!("SIGTERM received; draining telemetry");
    }
}

pub(crate) async fn serve(
    server: CodeSceneServer,
    mut termination: Termination,
) -> anyhow::Result<()> {
    let tasks = server.tracking_tasks.clone();
    let result = serve_until_shutdown(server, &mut termination).await;
    if drain_telemetry(&tasks, TELEMETRY_DRAIN_TIMEOUT)
        .await
        .is_err()
    {
        tracing::warn!(
            pending_events = tasks.len(),
            "Telemetry shutdown drain timed out; pending events may be lost"
        );
    }
    result
}

async fn serve_until_shutdown(
    server: CodeSceneServer,
    termination: &mut Termination,
) -> anyhow::Result<()> {
    let stdio = rmcp::transport::stdio().into_transport();
    let transport = transport::DiscoveryFallbackTransport::new(stdio);
    let service = tokio::select! {
        result = serve_or_handle_disconnect(server, transport) => result?,
        () = termination.recv() => return Ok(()),
    };
    let Some(service) = service else {
        return Ok(());
    };
    tracing::info!("CodeScene MCP server ready");
    let cancellation = service.cancellation_token();
    let waiting = service.waiting();
    tokio::pin!(waiting);
    tokio::select! {
        result = &mut waiting => { result?; }
        () = termination.recv() => {
            cancellation.cancel();
            match tokio::time::timeout(SERVICE_CLOSE_TIMEOUT, waiting).await {
                Ok(result) => { result?; }
                Err(_) => tracing::warn!("MCP service shutdown timed out"),
            }
        }
    }
    Ok(())
}

async fn drain_telemetry(
    tasks: &TaskTracker,
    timeout: Duration,
) -> Result<(), tokio::time::error::Elapsed> {
    tasks.close();
    tokio::time::timeout(timeout, tasks.wait()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::Notify;

    #[tokio::test]
    async fn empty_tracker_drains_immediately() {
        let tasks = TaskTracker::new();
        drain_telemetry(&tasks, Duration::from_secs(1))
            .await
            .unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn drain_waits_for_all_pending_events() {
        let tasks = TaskTracker::new();
        let release = Arc::new(Notify::new());
        let completed = Arc::new(AtomicUsize::new(0));
        for _ in 0..20 {
            let release = release.clone();
            let completed = completed.clone();
            tasks.spawn(async move {
                release.notified().await;
                completed.fetch_add(1, Ordering::SeqCst);
            });
        }
        let draining = drain_telemetry(&tasks, Duration::from_secs(1));
        tokio::pin!(draining);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut draining)
                .await
                .is_err()
        );
        assert_eq!(completed.load(Ordering::SeqCst), 0);
        release.notify_waiters();
        draining.await.unwrap();
        assert_eq!(completed.load(Ordering::SeqCst), 20);
    }

    #[tokio::test]
    async fn stalled_event_does_not_block_shutdown_indefinitely() {
        let tasks = TaskTracker::new();
        let handle = tasks.spawn(std::future::pending::<()>());
        assert!(drain_telemetry(&tasks, Duration::from_millis(20))
            .await
            .is_err());
        assert_eq!(tasks.len(), 1);
        handle.abort();
        assert!(handle.await.unwrap_err().is_cancelled());
    }
}

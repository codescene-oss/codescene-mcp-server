//! Graceful shutdown on Unix SIGINT/SIGTERM or transport closure.
//!
//! Both signal handlers remain installed while telemetry drains. A client can
//! still enforce a shorter deadline with SIGKILL; pending events are not durable.

use std::future::Future;
use std::time::Duration;

use rmcp::service::RoleServer;
use tokio_util::task::TaskTracker;

use crate::{serve_or_handle_disconnect, CodeSceneServer};

const SERVICE_CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
const TELEMETRY_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct Termination {
    #[cfg(unix)]
    signal: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Termination {
    pub(crate) fn new() -> std::io::Result<Self> {
        Ok(Self {
            #[cfg(unix)]
            signal: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?,
            #[cfg(unix)]
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?,
        })
    }

    pub(crate) async fn recv(&mut self) {
        #[cfg(unix)]
        {
            let signal = tokio::select! {
                _ = self.signal.recv() => "SIGTERM",
                _ = self.interrupt.recv() => "SIGINT",
            };
            tracing::info!("{signal} received; draining telemetry");
        }
        #[cfg(not(unix))]
        std::future::pending::<()>().await;
    }
}

pub(crate) async fn serve<T, E, A>(
    server: CodeSceneServer,
    transport: T,
    termination: impl Future<Output = ()>,
) -> anyhow::Result<()>
where
    T: rmcp::transport::IntoTransport<RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    let tasks = server.tracking_tasks.clone();
    let result = serve_until_shutdown(server, transport, termination).await;
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

async fn serve_until_shutdown<T, E, A>(
    server: CodeSceneServer,
    transport: T,
    termination: impl Future<Output = ()>,
) -> anyhow::Result<()>
where
    T: rmcp::transport::IntoTransport<RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    tokio::pin!(termination);
    let service = tokio::select! {
        result = serve_or_handle_disconnect(server, transport) => result?,
        () = &mut termination => return Ok(()),
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
        () = termination => {
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
    use crate::analytics_attribution::tests::capture_logs;
    use crate::tests::make_server;
    use rmcp::transport::{IntoTransport, Transport};
    use rmcp::ServiceExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::oneshot;
    use tokio::sync::Notify;

    #[test]
    fn runtime_shuts_down_after_help_action() {
        crate::run_with_args(vec!["--help".to_string()]).unwrap();
    }

    #[test]
    fn runtime_propagates_invalid_cli_arguments() {
        let error = crate::run_with_args(vec!["--invalid-option".to_string()]).unwrap_err();
        assert!(error.to_string().contains("invalid command line arguments"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn termination_receives_sigint_and_sigterm() {
        let mut termination = Termination::new().unwrap();
        for signal in [libc::SIGINT, libc::SIGTERM] {
            let result = unsafe { libc::kill(std::process::id() as i32, signal) };
            assert_eq!(result, 0);
            tokio::time::timeout(Duration::from_secs(1), termination.recv())
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn disconnect_before_handshake_closes_telemetry_tracker() {
        let server = make_server(false);
        let tasks = server.tracking_tasks.clone();
        let (server_io, client_io) = tokio::io::duplex(4096);
        drop(client_io);
        serve(server, server_io, std::future::pending())
            .await
            .unwrap();
        assert!(tasks.is_closed());
    }

    #[tokio::test]
    async fn termination_before_handshake_still_drains_telemetry() {
        let server = make_server(false);
        let tasks = server.tracking_tasks.clone();
        let completed = Arc::new(AtomicUsize::new(0));
        let completed_task = completed.clone();
        tasks.spawn(async move {
            tokio::task::yield_now().await;
            completed_task.fetch_add(1, Ordering::SeqCst);
        });
        let (server_io, _client_io) = tokio::io::duplex(4096);
        serve(server, server_io, std::future::ready(()))
            .await
            .unwrap();
        assert!(tasks.is_closed());
        assert_eq!(completed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn invalid_handshake_returns_error_after_closing_tracker() {
        use tokio::io::AsyncWriteExt;
        let server = make_server(false);
        let tasks = server.tracking_tasks.clone();
        let (server_io, mut client_io) = tokio::io::duplex(4096);
        client_io
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n")
            .await
            .unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            serve(server, server_io, std::future::pending()),
        )
        .await
        .unwrap();
        assert!(result.is_err());
        assert!(tasks.is_closed());
    }

    #[tokio::test]
    async fn initialized_client_disconnect_drains_telemetry() {
        assert_initialized_shutdown(false).await;
    }

    #[tokio::test]
    async fn termination_after_handshake_cancels_service_and_drains() {
        assert_initialized_shutdown(true).await;
    }

    async fn assert_initialized_shutdown(terminate: bool) {
        let server = make_server(false);
        let tasks = server.tracking_tasks.clone();
        let (server_io, client_io) = tokio::io::duplex(4096);
        let (signal, received) = oneshot::channel();
        let server = serve(server, server_io, async {
            if terminate {
                received.await.unwrap()
            } else {
                std::future::pending().await
            }
        });
        let client = async {
            let client = ().serve(client_io).await.unwrap();
            if terminate {
                signal.send(()).unwrap();
                client.waiting().await.unwrap();
            } else {
                client.cancel().await.unwrap();
            }
        };
        let (result, ()) = tokio::join!(server, client);
        result.unwrap();
        assert!(tasks.is_closed());
    }

    struct StalledClose<T>(T);

    impl<T: Transport<RoleServer>> Transport<RoleServer> for StalledClose<T> {
        type Error = T::Error;

        fn send(
            &mut self,
            item: rmcp::model::ServerJsonRpcMessage,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
            self.0.send(item)
        }

        fn receive(
            &mut self,
        ) -> impl Future<Output = Option<rmcp::model::ClientJsonRpcMessage>> + Send {
            self.0.receive()
        }

        fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            std::future::pending()
        }
    }

    fn capture_shutdown_logs(future: impl Future<Output = ()>) -> String {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        capture_logs(|| runtime.block_on(future))
    }

    #[test]
    fn stalled_service_close_is_bounded_and_logged() {
        let logs = capture_shutdown_logs(async {
            let server = make_server(false);
            let (server_io, client_io) = tokio::io::duplex(4096);
            let transport = StalledClose(server_io.into_transport());
            let (signal, received) = oneshot::channel();
            let running = tokio::spawn(serve(server, transport, async { received.await.unwrap() }));
            let client = ().serve(client_io).await.unwrap();
            signal.send(()).unwrap();
            tokio::time::timeout(Duration::from_secs(4), running)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            drop(client);
        });
        assert!(logs.contains("MCP service shutdown timed out"));
    }

    #[test]
    fn stalled_telemetry_is_bounded_and_logged_by_server() {
        let logs = capture_shutdown_logs(async {
            let server = make_server(false);
            let tasks = server.tracking_tasks.clone();
            let handle = tasks.spawn(std::future::pending::<()>());
            let (server_io, _client_io) = tokio::io::duplex(4096);
            tokio::time::timeout(
                Duration::from_secs(6),
                serve(server, server_io, std::future::ready(())),
            )
            .await
            .unwrap()
            .unwrap();
            handle.abort();
            assert!(handle.await.unwrap_err().is_cancelled());
        });
        assert!(logs.contains("Telemetry shutdown drain timed out"));
        assert!(logs.contains("pending_events=1"));
    }

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

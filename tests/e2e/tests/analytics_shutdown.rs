//! Telemetry delivery on graceful signals and loss characterization on SIGKILL.
//!
//! A control batch must arrive in full. A second batch is held in project
//! attribution until shutdown, then measured at the telemetry receiver.
//! SIGINT and SIGTERM must deliver the queued batch before exiting successfully.
//! Claude's SIGINT -> SIGTERM (100ms) -> SIGKILL (500ms) sequence is also tested:
//! responsive attribution can drain; stalled attribution still loses events.
//! An unavailable endpoint must instead trigger a bounded drain and warning.
//! SIGKILL still loses queued events because it cannot execute cleanup.
//! Control delivery has its own budget; it is not bound by the shutdown timeout.
//! Attribution readiness uses that budget too, since enrichment precedes it.
//! Gate assertions run outside their mutex so failed checks can still clean up.
//!
//! Run with `cargo test --test e2e test_analytics_shutdown -- --nocapture`.
//! Only local fake endpoints and a dummy token are used. Unix backends signal
//! an isolated process group (including npm's child); Docker signals the named
//! container, not the host-side Docker client. Windows skips these diagnostics.

use super::fake_http_server::FakeHttpServer;
use super::*;

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Command;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Instant;

    const BATCH_SIZE: usize = 20;
    const TOOL_TIMEOUT: Duration = Duration::from_secs(60);
    const CONTROL_DELIVERY_TIMEOUT: Duration = Duration::from_secs(60);
    const GATE_HOLD_TIMEOUT: Duration = TOOL_TIMEOUT.saturating_add(CONTROL_DELIVERY_TIMEOUT);
    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
    const OBSERVATION_TIME: Duration = Duration::from_secs(1);
    const MAPPING_PATH: &str = "/api/v2/mcp/repository-projects";

    pub(super) enum Shutdown {
        Signal(i32),
        Claude,
    }

    impl Shutdown {
        fn initial_signal(&self) -> i32 {
            match self {
                Self::Signal(signal) => *signal,
                Self::Claude => libc::SIGINT,
            }
        }

        fn supported(&self) -> bool {
            if matches!(self, Self::Claude) && is_docker() {
                skip_if_docker(
                    "Claude's local process deadlines cannot be modeled by Docker CLI round trips",
                );
                return false;
            }
            true
        }
    }

    #[derive(Default)]
    struct Gate {
        state: Mutex<(bool, bool)>,
        changed: Condvar,
    }

    impl Gate {
        fn block(&self) {
            let released = {
                let mut state = self.state.lock().unwrap();
                state.0 = true;
                self.changed.notify_all();
                let (state, _) = self
                    .changed
                    .wait_timeout_while(state, GATE_HOLD_TIMEOUT, |state| !state.1)
                    .unwrap();
                state.1
            };
            assert!(released, "Attribution gate timed out");
        }

        fn wait_until_blocked(&self, timeout: Duration) -> bool {
            let (state, _) = self
                .changed
                .wait_timeout_while(self.state.lock().unwrap(), timeout, |state| !state.0)
                .unwrap();
            state.0
        }

        fn release(&self) {
            self.state.lock().unwrap().1 = true;
            self.changed.notify_all();
        }
    }

    struct BlockedAttribution {
        gate: Arc<Gate>,
        server: FakeHttpServer,
    }

    impl BlockedAttribution {
        fn new() -> Self {
            let gate = Arc::new(Gate::default());
            let handler_gate = Arc::clone(&gate);
            let server = FakeHttpServer::start(move |request| {
                assert_eq!(request.method, "GET");
                assert_eq!(request.path, MAPPING_PATH);
                handler_gate.block();
                (200, r#"{"repositories":[]}"#.to_string())
            });
            Self { gate, server }
        }
    }

    impl Drop for BlockedAttribution {
        fn drop(&mut self) {
            self.gate.release();
            self.server.shutdown();
        }
    }

    #[test]
    fn test_analytics_shutdown_gate_timeout_allows_cleanup() {
        let api = BlockedAttribution::new();
        let gate = Arc::clone(&api.gate);
        let failure = std::panic::catch_unwind(move || {
            assert!(
                api.gate.wait_until_blocked(Duration::ZERO),
                "Simulated readiness timeout"
            );
        });
        assert!(failure.is_err(), "The readiness assertion should fail");
        assert!(
            !gate.state.is_poisoned(),
            "Cleanup must not encounter a poisoned gate"
        );
        gate.block();
        assert!(gate.wait_until_blocked(Duration::ZERO));
    }

    #[test]
    fn test_analytics_shutdown_waits_for_delayed_attribution_readiness() {
        let api = BlockedAttribution::new();
        let gate = Arc::clone(&api.gate);
        let started = Instant::now();
        let delayed_request = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(6));
            gate.block();
        });
        let blocked = api.gate.wait_until_blocked(CONTROL_DELIVERY_TIMEOUT);
        drop(api);
        delayed_request
            .join()
            .expect("Delayed attribution should finish");
        assert!(
            blocked,
            "Readiness must allow enrichment to exceed five seconds"
        );
        assert!(started.elapsed() >= Duration::from_secs(6));
    }

    struct SignalClient {
        client: MCPClient,
        container_name: Option<String>,
    }

    impl SignalClient {
        fn new(mut command: Vec<String>, env: &[(String, String)], repo: &Path) -> Self {
            let container_name = is_docker().then(|| format!("cs-mcp-{}", uuid::Uuid::new_v4()));
            if let Some(name) = &container_name {
                command.splice(2..2, ["--name".to_string(), name.clone()]);
            }
            let mut client = make_client(&command, env, repo);
            assert!(client.start_in_process_group(), "Server should start");
            let mut process = Self {
                client,
                container_name,
            };
            process
                .client
                .initialize()
                .expect("Initialize should succeed");
            process
        }

        fn signal(&mut self, signal: i32) {
            if let Some(name) = &self.container_name {
                let output = Command::new("docker")
                    .args(["kill", "--signal", &signal.to_string(), name])
                    .output()
                    .expect("docker kill should execute");
                assert!(
                    output.status.success(),
                    "docker kill failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            } else {
                let pid = i32::try_from(self.client.process_id()).expect("PID should fit i32");
                // The test owns this isolated group, including the npm wrapper's child.
                let result = unsafe { libc::kill(-pid, signal) };
                assert_eq!(
                    result,
                    0,
                    "Signal failed: {}",
                    std::io::Error::last_os_error()
                );
            }
        }
    }

    impl Drop for SignalClient {
        fn drop(&mut self) {
            if let Some(name) = &self.container_name {
                let output = Command::new("docker")
                    .args(["rm", "--force", name])
                    .output();
                if let Err(error) = output {
                    eprintln!("Failed to clean up test container {name}: {error}");
                }
            } else {
                let pid = i32::try_from(self.client.process_id()).expect("PID should fit i32");
                let result = unsafe { libc::kill(-pid, libc::SIGKILL) };
                if result != 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
                {
                    eprintln!(
                        "Failed to clean up test process group: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
        }
    }

    fn generate_events(client: &mut MCPClient) {
        for _ in 0..BATCH_SIZE / 2 {
            for (tool, args) in [
                ("get_config", json!({"key": "config_dir"})),
                ("list_skills", json!({})),
            ] {
                let response = client
                    .call_tool(tool, args, TOOL_TIMEOUT)
                    .expect("Tool call should succeed");
                assert!(response.get("error").is_none(), "{tool}: {response}");
                assert_ne!(response["result"]["isError"], true, "{tool}: {response}");
                assert!(
                    !extract_result_text(&response).is_empty(),
                    "{tool} should return content"
                );
            }
        }
    }

    fn event_counts(server: &FakeHttpServer) -> [usize; 2] {
        let payloads = server.get_payloads();
        ["mcp-get-config", "mcp-list-skills"].map(|event| {
            payloads
                .iter()
                .filter(|payload| payload["event-type"] == event)
                .count()
        })
    }

    fn wait_for_control(server: &FakeHttpServer, client: &MCPClient) {
        let expected = [BATCH_SIZE / 2; 2];
        let deadline = Instant::now() + CONTROL_DELIVERY_TIMEOUT;
        while event_counts(server) != expected && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            event_counts(server),
            expected,
            "Control events must all arrive before testing shutdown. Server stderr:\n{}",
            client.get_stderr()
        );
    }

    #[test]
    fn test_analytics_shutdown_control_waits_for_delayed_delivery() {
        let gate = Arc::new(Gate::default());
        let handler_gate = Arc::clone(&gate);
        let tracking = FakeHttpServer::start(move |_| {
            handler_gate.block();
            (200, "{}".to_string())
        });
        let api = BlockedAttribution::new();
        let (command, mut env, repo_dir, _tmp) = setup();
        configure_env(&mut env, &repo_dir, &api.server, &tracking);
        let mut process = SignalClient::new(command, &env, &repo_dir);
        generate_events(&mut process.client);
        assert!(
            gate.wait_until_blocked(CONTROL_DELIVERY_TIMEOUT),
            "Telemetry must reach the receiver before delaying delivery. Server stderr:\n{}",
            process.client.get_stderr()
        );
        let delayed_release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(6));
            gate.release();
        });
        wait_for_control(&tracking, &process.client);
        delayed_release
            .join()
            .expect("Delayed receiver should finish");
        tracking.shutdown();
    }

    fn add_remote(repo: &Path) {
        let output = Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/acme/telemetry-test.git",
            ])
            .current_dir(repo)
            .output()
            .expect("git remote add should execute");
        assert!(
            output.status.success(),
            "git remote add: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn wait_for_drain_start(client: &MCPClient, signal: i32) {
        let name = if signal == libc::SIGINT {
            "SIGINT"
        } else {
            "SIGTERM"
        };
        let message = format!("{name} received; draining telemetry");
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while !client.get_stderr().contains(&message) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            client.get_stderr().contains(&message),
            "Server must handle {name} before attribution is released: {}",
            client.get_stderr()
        );
    }

    fn configure_env(
        env: &mut Vec<(String, String)>,
        repo_dir: &Path,
        api: &FakeHttpServer,
        tracking: &FakeHttpServer,
    ) {
        use_isolated_config_dir(env, repo_dir, ".cs_config_shutdown");
        for (key, value) in [
            ("CS_ACCESS_TOKEN", "shutdown-test-token".to_string()),
            ("CS_ONPREM_URL", api.url()),
            ("CS_TRACKING_URL", tracking.url()),
            ("CS_DISABLE_TRACKING", "0".to_string()),
            ("CS_DISABLE_VERSION_CHECK", "1".to_string()),
            ("CS_DISABLE_SETUP_HINT", "1".to_string()),
        ] {
            env.retain(|(name, _)| name != key);
            env.push((key.to_string(), value));
        }
    }

    fn assert_exit_status(status: std::process::ExitStatus, signal: i32) {
        if signal != libc::SIGKILL {
            assert!(
                status.success(),
                "Graceful signal {signal} must exit cleanly: {status}"
            );
        } else if !is_docker() {
            assert_eq!(
                status.signal(),
                Some(signal),
                "Server must exit from the requested signal"
            );
        } else {
            assert_eq!(
                status.code(),
                Some(128 + signal),
                "Container must exit from the requested signal"
            );
        }
    }

    fn wait_until_exit(
        process: &mut SignalClient,
        deadline: Instant,
    ) -> Option<std::process::ExitStatus> {
        loop {
            if let Some(status) = process
                .client
                .try_wait_for_exit()
                .expect("Check server exit")
            {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn claude_shutdown(
        process: &mut SignalClient,
        gate: &Gate,
        release_attribution: bool,
    ) -> std::process::ExitStatus {
        let started = Instant::now();
        process.signal(libc::SIGINT);
        if let Some(status) = wait_until_exit(process, started + Duration::from_millis(100)) {
            return status;
        }
        process.signal(libc::SIGTERM);
        if release_attribution {
            if let Some(status) = wait_until_exit(process, started + Duration::from_millis(150)) {
                return status;
            }
            // Hold attribution through SIGTERM to exercise escalation during draining.
            gate.release();
        }
        if let Some(status) = wait_until_exit(process, started + Duration::from_millis(500)) {
            return status;
        }
        process.signal(libc::SIGKILL);
        process
            .client
            .wait_for_exit(WAIT_TIMEOUT)
            .expect("SIGKILL must stop server")
    }

    fn finish_shutdown(
        shutdown: &Shutdown,
        process: &mut SignalClient,
        api: &BlockedAttribution,
        release: bool,
    ) -> std::process::ExitStatus {
        if matches!(shutdown, Shutdown::Claude) {
            return claude_shutdown(process, &api.gate, release);
        }
        let signal = shutdown.initial_signal();
        process.signal(signal);
        if signal != libc::SIGKILL {
            wait_for_drain_start(&process.client, signal);
        }
        if release {
            api.gate.release();
        }
        process
            .client
            .wait_for_exit(Duration::from_secs(9))
            .unwrap_or_else(|error| panic!("{error}\nstderr: {}", process.client.get_stderr()))
    }

    fn assert_shutdown_result(
        shutdown: &Shutdown,
        status: std::process::ExitStatus,
        release_attribution: bool,
        client: &MCPClient,
    ) {
        let forced = matches!(shutdown, Shutdown::Claude) && !release_attribution;
        let signal = if forced {
            libc::SIGKILL
        } else {
            shutdown.initial_signal()
        };
        assert_exit_status(status, signal);
        if !release_attribution && !forced {
            assert!(
                client
                    .get_stderr()
                    .contains("Telemetry shutdown drain timed out"),
                "Stalled telemetry must log a drain timeout: {}",
                client.get_stderr()
            );
        }
    }

    pub(super) fn run(shutdown: Shutdown, release_attribution: bool) {
        if !shutdown.supported() {
            return;
        }
        let signal = shutdown.initial_signal();
        let api = BlockedAttribution::new();
        let tracking = FakeHttpServer::always_ok();
        let (command, mut env, repo_dir, _tmp) = setup();
        configure_env(&mut env, &repo_dir, &api.server, &tracking);
        let mut process = SignalClient::new(command, &env, &repo_dir);
        generate_events(&mut process.client);
        wait_for_control(&tracking, &process.client);

        add_remote(&repo_dir);
        generate_events(&mut process.client);
        assert!(
            api.gate.wait_until_blocked(CONTROL_DELIVERY_TIMEOUT),
            "Telemetry must reach attribution before shutdown. Server stderr:\n{}",
            process.client.get_stderr()
        );
        assert_eq!(
            event_counts(&tracking),
            [BATCH_SIZE / 2; 2],
            "Second batch must still be queued"
        );

        let status = finish_shutdown(&shutdown, &mut process, &api, release_attribution);
        assert_shutdown_result(&shutdown, status, release_attribution, &process.client);
        std::thread::sleep(OBSERVATION_TIME);
        let counts = event_counts(&tracking);
        let delivered = counts.iter().sum::<usize>() - BATCH_SIZE;
        eprintln!("signal={signal}, exit={status}, control={BATCH_SIZE}/{BATCH_SIZE}, queued delivered={delivered}/{BATCH_SIZE}, lost={}", BATCH_SIZE - delivered);
        let expected = if signal != libc::SIGKILL && release_attribution {
            [BATCH_SIZE; 2]
        } else {
            [BATCH_SIZE / 2; 2]
        };
        assert_eq!(
            counts, expected,
            "Graceful signals must drain queued events; timeouts and forced kills can lose them. Stderr:\n{}",
            process.client.get_stderr()
        );
        tracking.shutdown();
    }
}

#[test]
pub fn test_analytics_shutdown_hard_kill() {
    #[cfg(unix)]
    unix::run(unix::Shutdown::Signal(libc::SIGKILL), true);
    #[cfg(not(unix))]
    eprintln!("SKIP: SIGKILL diagnostics require Unix signals");
}

#[test]
pub fn test_analytics_shutdown_soft_kill() {
    #[cfg(unix)]
    unix::run(unix::Shutdown::Signal(libc::SIGTERM), true);
    #[cfg(not(unix))]
    eprintln!("SKIP: SIGTERM diagnostics require Unix signals");
}

#[test]
pub fn test_analytics_shutdown_soft_kill_timeout() {
    #[cfg(unix)]
    unix::run(unix::Shutdown::Signal(libc::SIGTERM), false);
    #[cfg(not(unix))]
    eprintln!("SKIP: SIGTERM diagnostics require Unix signals");
}

#[test]
pub fn test_analytics_shutdown_sigint() {
    #[cfg(unix)]
    unix::run(unix::Shutdown::Signal(libc::SIGINT), true);
    #[cfg(not(unix))]
    eprintln!("SKIP: SIGINT diagnostics require Unix signals");
}

#[test]
pub fn test_analytics_shutdown_sigint_timeout() {
    #[cfg(unix)]
    unix::run(unix::Shutdown::Signal(libc::SIGINT), false);
    #[cfg(not(unix))]
    eprintln!("SKIP: SIGINT diagnostics require Unix signals");
}

#[test]
pub fn test_analytics_shutdown_claude_sequence() {
    #[cfg(unix)]
    unix::run(unix::Shutdown::Claude, true);
    #[cfg(not(unix))]
    eprintln!("SKIP: Claude signal sequence requires Unix signals");
}

#[test]
pub fn test_analytics_shutdown_claude_sequence_stalled() {
    #[cfg(unix)]
    unix::run(unix::Shutdown::Claude, false);
    #[cfg(not(unix))]
    eprintln!("SKIP: Claude signal sequence requires Unix signals");
}

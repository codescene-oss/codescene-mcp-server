//! Integration tests for clean server shutdown during the MCP handshake.
//!
//! Validates that the server exits cleanly (exit code 0) when stdin is
//! closed at various points during the initialization handshake:
//! before any input, after the initialize request, and after the full
//! initialize / initialized exchange.

use super::*;

use serde_json::json;
use std::io::Write;
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{Duration, Instant};

const EXIT_TIMEOUT_SECS: u64 = 10;

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn shutdown_setup() -> (Vec<String>, Vec<(String, String)>, tempfile::TempDir) {
    let executable = find_or_build_executable();
    let backend = create_backend(executable);
    let temp_dir = create_temp_dir("cs_mcp_shutdown_test_").expect("temp dir");
    let base = base_env();
    let env = backend.get_env(&base, temp_dir.path());
    let env_vec: Vec<(String, String)> = env.into_iter().collect();
    let command = backend.get_command(temp_dir.path());
    (command, env_vec, temp_dir)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn spawn_server(command: &[String], env: &[(String, String)], cwd: &Path) -> std::process::Child {
    let (program, args) = command.split_first().expect("command must not be empty");
    ProcessCommand::new(program)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn server process")
}

fn send_message(child: &mut std::process::Child, message: &serde_json::Value) {
    let stdin = child.stdin.as_mut().expect("stdin should be piped");
    serde_json::to_writer(&mut *stdin, message).expect("failed to write JSON");
    writeln!(stdin).expect("failed to write newline");
    stdin.flush().expect("failed to flush stdin");
}

fn wait_for_exit(mut child: std::process::Child) -> (Option<i32>, String) {
    let timeout = Duration::from_secs(EXIT_TIMEOUT_SECS);
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr_bytes = child
                    .stderr
                    .take()
                    .map(|mut e| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut e, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
                return (status.code(), stderr);
            }
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(100));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return (None, String::new());
            }
        }
    }
}

fn close_stdin_and_wait(mut child: std::process::Child) -> (Option<i32>, String) {
    drop(child.stdin.take());
    wait_for_exit(child)
}

fn initialize_request() -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "shutdown-test-client",
                "version": "1.0.0"
            }
        }
    })
}

fn initialized_notification() -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
}

fn check_clean_exit(exit_code: Option<i32>, stderr: &str, scenario: &str) -> bool {
    let code = exit_code.unwrap_or_else(|| {
        panic!("{scenario}: server did not exit within {EXIT_TIMEOUT_SECS}s");
    });
    assert_eq!(
        code, 0,
        "{scenario}: expected exit code 0, got {code}\nstderr: {stderr}"
    );
    true
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
pub fn test_stdin_closed_before_any_input() {
    let (command, env, temp_dir) = shutdown_setup();
    let child = spawn_server(&command, &env, temp_dir.path());

    std::thread::sleep(Duration::from_millis(300));

    let (exit_code, stderr) = close_stdin_and_wait(child);
    check_clean_exit(exit_code, &stderr, "stdin closed before any input");
}

#[test]
pub fn test_stdin_closed_after_initialize_request() {
    let (command, env, temp_dir) = shutdown_setup();
    let mut child = spawn_server(&command, &env, temp_dir.path());

    send_message(&mut child, &initialize_request());
    std::thread::sleep(Duration::from_millis(500));

    let (exit_code, stderr) = close_stdin_and_wait(child);
    check_clean_exit(exit_code, &stderr, "stdin closed after initialize request");
}

#[test]
pub fn test_stdin_closed_after_full_handshake() {
    let (command, env, temp_dir) = shutdown_setup();
    let mut child = spawn_server(&command, &env, temp_dir.path());

    send_message(&mut child, &initialize_request());
    std::thread::sleep(Duration::from_millis(300));

    send_message(&mut child, &initialized_notification());
    std::thread::sleep(Duration::from_millis(300));

    let (exit_code, stderr) = close_stdin_and_wait(child);
    check_clean_exit(exit_code, &stderr, "stdin closed after full handshake");
}

// ---------------------------------------------------------------------------
// SIGTERM tests (Unix + npm backend only)
// ---------------------------------------------------------------------------

fn is_npm_backend() -> bool {
    std::env::var("CS_MCP_BACKEND").as_deref() == Ok("npm")
}

#[allow(unused_mut)]
fn sigterm_and_wait(mut child: std::process::Child) -> (Option<i32>, String) {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(child.id() as i32, libc::SIGTERM);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }

    wait_for_exit(child)
}

#[test]
pub fn test_sigterm_before_any_input() {
    if cfg!(windows) || !is_npm_backend() {
        eprintln!("  SKIP: SIGTERM tests only run on Unix with npm backend");
        return;
    }

    let (command, env, temp_dir) = shutdown_setup();
    let child = spawn_server(&command, &env, temp_dir.path());
    std::thread::sleep(Duration::from_millis(300));

    let (exit_code, stderr) = sigterm_and_wait(child);
    check_clean_exit(exit_code, &stderr, "SIGTERM before any input");
}

#[test]
pub fn test_sigterm_after_full_handshake() {
    if cfg!(windows) || !is_npm_backend() {
        eprintln!("  SKIP: SIGTERM tests only run on Unix with npm backend");
        return;
    }

    let (command, env, temp_dir) = shutdown_setup();
    let mut child = spawn_server(&command, &env, temp_dir.path());

    send_message(&mut child, &initialize_request());
    std::thread::sleep(Duration::from_millis(300));

    send_message(&mut child, &initialized_notification());
    std::thread::sleep(Duration::from_millis(300));

    let (exit_code, stderr) = sigterm_and_wait(child);
    check_clean_exit(exit_code, &stderr, "SIGTERM after full handshake");
}

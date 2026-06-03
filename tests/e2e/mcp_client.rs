//! MCP Client for e2e integration tests.
//!
//! Communicates with the MCP server via JSON-RPC over stdio.

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

static MSG_ID: AtomicU64 = AtomicU64::new(0);

fn next_msg_id() -> u64 {
    MSG_ID.fetch_add(1, Ordering::SeqCst) + 1
}

type LineBuffer = Arc<Mutex<VecDeque<String>>>;
type LineList = Arc<Mutex<Vec<String>>>;

fn spawn_line_reader<R: std::io::Read + Send + 'static>(
    stream: R,
    mut on_line: impl FnMut(String) + Send + 'static,
) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().flatten() {
            if !line.is_empty() {
                on_line(line);
            }
        }
    });
}

pub struct MCPClient {
    command: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<String>,
    process: Option<Child>,
    responses: LineBuffer,
    stderr_lines: LineList,
}

impl MCPClient {
    pub fn new(command: Vec<String>, env: Vec<(String, String)>, cwd: Option<String>) -> Self {
        Self {
            command,
            env,
            cwd,
            process: None,
            responses: Arc::new(Mutex::new(VecDeque::new())),
            stderr_lines: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start(&mut self) -> bool {
        let mut cmd = self.build_command();
        match cmd.spawn() {
            Ok(child) => self.attach_to_process(child),
            Err(e) => {
                eprintln!("Failed to start MCP server: {e}");
                false
            }
        }
    }

    fn build_command(&self) -> Command {
        let mut cmd = Command::new(&self.command[0]);
        if self.command.len() > 1 {
            cmd.args(&self.command[1..]);
        }
        cmd.env_clear();
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    fn attach_to_process(&mut self, mut child: Child) -> bool {
        self.spawn_stdout_reader(child.stdout.take().expect("stdout"));
        self.spawn_stderr_reader(child.stderr.take().expect("stderr"));
        // npm backend needs extra time: node downloads, extracts, then launches the binary
        let wait_secs = if std::env::var("CS_MCP_BACKEND").as_deref() == Ok("npm") {
            10
        } else {
            1
        };
        thread::sleep(Duration::from_secs(wait_secs));
        let alive = matches!(child.try_wait(), Ok(None));
        if !alive {
            let stderr = self.stderr_lines.lock().unwrap().join("\n");
            eprintln!("MCP server exited immediately. stderr:\n{stderr}");
        }
        self.process = Some(child);
        alive
    }

    fn spawn_stdout_reader(&self, stdout: std::process::ChildStdout) {
        let buf = Arc::clone(&self.responses);
        spawn_line_reader(stdout, move |line| buf.lock().unwrap().push_back(line));
    }

    fn spawn_stderr_reader(&self, stderr: std::process::ChildStderr) {
        let buf = Arc::clone(&self.stderr_lines);
        spawn_line_reader(stderr, move |line| buf.lock().unwrap().push(line));
    }

    pub fn send_request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": next_msg_id(),
            "method": method,
            "params": params,
        }))?;
        self.await_response(timeout)
    }

    fn write_message(&mut self, message: &Value) -> Result<(), String> {
        let child = self.process.as_mut().ok_or("No process")?;
        let stdin = child.stdin.as_mut().ok_or("No stdin")?;
        let mut msg = serde_json::to_string(message).map_err(|e| e.to_string())?;
        msg.push('\n');
        stdin.write_all(msg.as_bytes()).map_err(|e| format!("Write failed: {e}"))?;
        stdin.flush().map_err(|e| format!("Flush failed: {e}"))
    }

    fn await_response(&self, timeout: Duration) -> Result<Value, String> {
        let start = Instant::now();
        loop {
            if let Some(line) = self.responses.lock().unwrap().pop_front() {
                return serde_json::from_str::<Value>(&line)
                    .map_err(|e| format!("Invalid JSON: {e}"));
            }
            if start.elapsed() > timeout {
                let tail: String = self.get_stderr().lines().rev().take(10)
                    .collect::<Vec<_>>().join("\n");
                return Err(format!("Timeout waiting for response. Recent stderr:\n{tail}"));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn send_notification(&mut self, method: &str, params: Option<Value>) {
        let mut notification = json!({"jsonrpc": "2.0", "method": method});
        if let Some(p) = params {
            notification["params"] = p;
        }
        let _ = self.write_message(&notification);
    }

    pub fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        self.send_request(
            "tools/call",
            json!({"name": tool_name, "arguments": arguments}),
            timeout,
        )
    }

    pub fn initialize(&mut self) -> Result<Value, String> {
        let response = self.send_request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "integration-test-client", "version": "1.0.0"},
            }),
            Duration::from_secs(30),
        )?;
        thread::sleep(Duration::from_millis(200));
        self.send_notification("notifications/initialized", None);
        thread::sleep(Duration::from_millis(300));
        Ok(response)
    }

    pub fn stop(&mut self) {
        if let Some(ref mut child) = self.process {
            drop(child.stdin.take());
            let _ = child.kill();
            let _ = child.wait();
        }
        self.process = None;
    }

    pub fn get_stderr(&self) -> String {
        self.stderr_lines.lock().unwrap().join("\n")
    }
}

impl Drop for MCPClient {
    fn drop(&mut self) {
        self.stop();
    }
}

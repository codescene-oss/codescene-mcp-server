//! Minimal CodeScene-shaped OAuth mock for host-only e2e tests.
//!
//! Implements the subset of the on-prem OAuth surface the embedded CLI uses:
//! - empty `POST /oauth2/token` → OAuth error (route discovery)
//! - `GET /oauth2/auth` → 302 to the CLI localhost callback with `code` + `state`
//! - `POST /oauth2/token` with `grant_type=authorization_code` → access/refresh tokens
//! - `POST /oauth2/token` with `grant_type=refresh_token` → refreshed access token
//!
//! Also stubs incidental API paths the CLI may hit after login.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::fake_server_bind_host;
use super::fake_server_url_host;

const ACCESS_TOKEN: &str = "oau_mock_e2e_access";
const REFRESH_TOKEN: &str = "orr_mock_e2e_refresh";
const REFRESHED_ACCESS_TOKEN: &str = "oau_mock_e2e_refreshed";

#[derive(Clone, Debug, Default)]
pub struct OAuthRequestLog {
    pub discovery_posts: usize,
    pub authorize_gets: usize,
    pub auth_code_exchanges: usize,
    pub refresh_grants: usize,
}

pub struct FakeOAuthServer {
    port: u16,
    log: Arc<Mutex<OAuthRequestLog>>,
    shutdown: Arc<Mutex<bool>>,
}

pub struct FakeOAuthServerOptions {
    /// When false, `/oauth2/*` routes return 404 so discovery fails.
    pub oauth_enabled: bool,
}

impl Default for FakeOAuthServerOptions {
    fn default() -> Self {
        Self {
            oauth_enabled: true,
        }
    }
}

struct ServerCtx {
    log: Arc<Mutex<OAuthRequestLog>>,
    shutdown: Arc<Mutex<bool>>,
    base_url: String,
    oauth_enabled: bool,
}

impl FakeOAuthServer {
    pub fn start_with_options(options: FakeOAuthServerOptions) -> Self {
        let bind_addr = format!("{}:0", fake_server_bind_host());
        let listener = TcpListener::bind(&bind_addr).expect("bind OAuth mock");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();

        let log = Arc::new(Mutex::new(OAuthRequestLog::default()));
        let shutdown = Arc::new(Mutex::new(false));
        let ctx = ServerCtx {
            log: Arc::clone(&log),
            shutdown: Arc::clone(&shutdown),
            base_url: format!("http://{}:{}", fake_server_url_host(), port),
            oauth_enabled: options.oauth_enabled,
        };

        thread::spawn(move || accept_loop(listener, ctx));
        thread::sleep(Duration::from_millis(50));

        FakeOAuthServer {
            port,
            log,
            shutdown,
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", fake_server_url_host(), self.port)
    }

    pub fn access_token(&self) -> &'static str {
        ACCESS_TOKEN
    }

    pub fn request_log(&self) -> OAuthRequestLog {
        self.log.lock().unwrap().clone()
    }

    pub fn shutdown(&self) {
        *self.shutdown.lock().unwrap() = true;
    }
}

impl Drop for FakeOAuthServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept_loop(listener: TcpListener, ctx: ServerCtx) {
    while !*ctx.shutdown.lock().unwrap() {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Accepted sockets inherit the listener's nonblocking mode on Windows.
                // Reads must block so a request arriving just after accept is not discarded.
                stream
                    .set_nonblocking(false)
                    .expect("set OAuth connection blocking");
                serve_connection(&mut stream, &ctx);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
}

fn serve_connection(stream: &mut TcpStream, ctx: &ServerCtx) {
    if let Some(req) = read_http_request(stream) {
        let response = dispatch_request(&req, ctx);
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

struct HttpRequest {
    method: String,
    path: String,
    body: String,
}

fn read_http_request(stream: &mut TcpStream) -> Option<HttpRequest> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .ok()?;

    let buf = read_request_bytes(stream);
    if buf.is_empty() {
        return None;
    }
    parse_http_request(&buf)
}

fn read_request_bytes(stream: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    for _ in 0..40 {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if request_body_complete(&buf) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    buf
}

fn request_body_complete(buf: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(buf) else {
        return false;
    };
    let Some(header_end) = text.find("\r\n\r\n") else {
        return false;
    };
    let content_length = content_length_from_headers(&text[..header_end]);
    buf.len() >= header_end + 4 + content_length
}

fn content_length_from_headers(headers: &str) -> usize {
    headers
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

fn parse_http_request(buf: &[u8]) -> Option<HttpRequest> {
    let text = String::from_utf8_lossy(buf);
    let mut lines = text.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let body = text
        .find("\r\n\r\n")
        .map(|i| text[i + 4..].to_string())
        .unwrap_or_default();
    Some(HttpRequest { method, path, body })
}

fn dispatch_request(req: &HttpRequest, ctx: &ServerCtx) -> String {
    let path_only = req.path.split('?').next().unwrap_or(&req.path);

    if let Some(response) = handle_oauth_auth(req, path_only, ctx) {
        return response;
    }
    if let Some(response) = handle_oauth_token(req, path_only, ctx) {
        return response;
    }
    api_stub_response(path_only)
}

fn handle_oauth_auth(req: &HttpRequest, path_only: &str, ctx: &ServerCtx) -> Option<String> {
    if path_only != "/oauth2/auth" || req.method != "GET" {
        return None;
    }
    if !ctx.oauth_enabled {
        return Some(json_response(404, r#"{"error":"not_found"}"#));
    }
    ctx.log.lock().unwrap().authorize_gets += 1;
    Some(authorize_redirect(&req.path))
}

fn handle_oauth_token(req: &HttpRequest, path_only: &str, ctx: &ServerCtx) -> Option<String> {
    if path_only != "/oauth2/token" || req.method != "POST" {
        return None;
    }
    if !ctx.oauth_enabled {
        return Some(json_response(404, r#"{"error":"not_found"}"#));
    }
    if req.body.trim().is_empty() {
        ctx.log.lock().unwrap().discovery_posts += 1;
        return Some(json_response(
            400,
            r#"{"error":"invalid_request","error_description":"missing parameters"}"#,
        ));
    }
    Some(token_grant_response(&req.body, &ctx.log, &ctx.base_url))
}

fn api_stub_response(path_only: &str) -> String {
    if path_only.contains("/api/v2/tool-license/cli") {
        return json_response(200, r#"{"valid":true}"#);
    }
    if path_only.contains("/api/v2/projects") {
        return json_response(200, r#"[{"id":1,"name":"Test Project"}]"#);
    }
    json_response(200, "{}")
}

fn authorize_redirect(full_path: &str) -> String {
    let query = full_path.split('?').nth(1).unwrap_or("");
    let mut state = "missing-state".to_string();
    let mut redirect_uri = "http://127.0.0.1:19876/callback".to_string();

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = percent_decode(parts.next().unwrap_or(""));
        match key {
            "state" => state = value,
            "redirect_uri" => redirect_uri = value,
            _ => {}
        }
    }

    let location = format!("{redirect_uri}?code=e2e-auth-code&state={state}");
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn token_grant_response(
    body: &str,
    log: &Arc<Mutex<OAuthRequestLog>>,
    base_url: &str,
) -> String {
    let grant = form_value(body, "grant_type").unwrap_or_default();
    match grant.as_str() {
        "authorization_code" => {
            log.lock().unwrap().auth_code_exchanges += 1;
            json_response(200, &token_payload(ACCESS_TOKEN, base_url))
        }
        "refresh_token" => {
            log.lock().unwrap().refresh_grants += 1;
            json_response(200, &token_payload(REFRESHED_ACCESS_TOKEN, base_url))
        }
        _ => json_response(400, r#"{"error":"unsupported_grant_type"}"#),
    }
}

fn token_payload(access_token: &str, base_url: &str) -> String {
    format!(
        r#"{{"access_token":"{access_token}","refresh_token":"{REFRESH_TOKEN}","token_type":"Bearer","expires_in":3600,"scope":"cli.access mcpapi.read mcpapi.write mcp.event-tracking","api_url":"{base_url}/api"}}"#
    )
}

fn json_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn form_value(body: &str, key: &str) -> Option<String> {
    for pair in body.split('&') {
        let mut parts = pair.splitn(2, '=');
        let k = parts.next()?;
        if k == key {
            return Some(percent_decode(parts.next().unwrap_or("")));
        }
    }
    None
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some(decoded) = decode_percent_at(bytes, i) {
            out.push(decoded.0);
            i = decoded.1;
            continue;
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn decode_percent_at(bytes: &[u8], i: usize) -> Option<(u8, usize)> {
    if bytes[i] != b'%' || i + 2 >= bytes.len() {
        return None;
    }
    let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
    let byte = u8::from_str_radix(hex, 16).ok()?;
    Some((byte, i + 3))
}

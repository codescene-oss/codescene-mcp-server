//! Local HTTP fixture with blocking request reads, including on platforms where
//! accepted sockets inherit the listener's nonblocking mode.

use serde_json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::fake_server_bind_host;
use super::fake_server_url_host;

pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

type RequestHandler = Arc<dyn Fn(&CapturedRequest) -> (u16, String) + Send + Sync>;

pub struct FakeHttpServer {
    port: u16,
    captured_requests: Arc<Mutex<Vec<CapturedRequest>>>,
    shutdown: Arc<Mutex<bool>>,
}

impl FakeHttpServer {
    pub fn start(
        handler: impl Fn(&CapturedRequest) -> (u16, String) + Send + Sync + 'static,
    ) -> Self {
        let bind_addr = format!("{}:0", fake_server_bind_host());
        let listener = TcpListener::bind(&bind_addr).expect("Failed to bind");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();

        let captured_requests: Arc<Mutex<Vec<CapturedRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(Mutex::new(false));

        let reqs = Arc::clone(&captured_requests);
        let stop = Arc::clone(&shutdown);
        let handler: RequestHandler = Arc::new(handler);

        thread::spawn(move || accept_loop(&listener, &reqs, &stop, &handler));

        FakeHttpServer {
            port,
            captured_requests,
            shutdown,
        }
    }

    pub fn always_ok() -> Self {
        Self::start(|_| (200, "{}".to_string()))
    }

    #[allow(dead_code)]
    pub fn with_responses(responses: Vec<(u16, String)>) -> Self {
        let queue = Arc::new(Mutex::new(responses));
        Self::start(move |_| {
            let mut q = queue.lock().unwrap();
            if q.is_empty() {
                return (200, "[]".to_string());
            }
            q.remove(0)
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", fake_server_url_host(), self.port)
    }

    pub fn get_requests(&self) -> Vec<CapturedRequest> {
        let locked = self.captured_requests.lock().unwrap();
        locked
            .iter()
            .map(|r| CapturedRequest {
                method: r.method.clone(),
                path: r.path.clone(),
                headers: r.headers.clone(),
                body: r.body.clone(),
            })
            .collect()
    }

    pub fn request_count(&self) -> usize {
        self.captured_requests.lock().unwrap().len()
    }

    pub fn get_payloads(&self) -> Vec<serde_json::Value> {
        let locked = self.captured_requests.lock().unwrap();
        locked
            .iter()
            .filter(|r| r.method == "POST")
            .filter_map(|r| serde_json::from_str(&r.body).ok())
            .collect()
    }

    pub fn shutdown(&self) {
        *self.shutdown.lock().unwrap() = true;
    }
}

fn accept_loop(
    listener: &TcpListener,
    captured: &Arc<Mutex<Vec<CapturedRequest>>>,
    shutdown: &Arc<Mutex<bool>>,
    handler: &RequestHandler,
) {
    while !is_shutdown(shutdown) {
        match try_accept(listener) {
            AcceptResult::Connected(mut stream) => {
                handle_connection(&mut stream, captured, handler);
            }
            AcceptResult::WouldBlock => thread::sleep(Duration::from_millis(100)),
            AcceptResult::Error => return,
        }
    }
}

fn is_shutdown(flag: &Arc<Mutex<bool>>) -> bool {
    *flag.lock().unwrap()
}

enum AcceptResult {
    Connected(TcpStream),
    WouldBlock,
    Error,
}

fn try_accept(listener: &TcpListener) -> AcceptResult {
    match listener.accept() {
        Ok((stream, _)) => AcceptResult::Connected(stream),
        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => AcceptResult::WouldBlock,
        Err(_) => AcceptResult::Error,
    }
}

fn handle_connection(
    stream: &mut TcpStream,
    captured: &Arc<Mutex<Vec<CapturedRequest>>>,
    handler: &RequestHandler,
) {
    // macOS inherits the nonblocking listener mode on accepted sockets.
    stream
        .set_nonblocking(false)
        .expect("Failed to make accepted HTTP socket blocking");
    let Some(request) = parse_http_request(stream) else {
        return;
    };
    let (status, body) = handler(&request);
    captured.lock().unwrap().push(request);
    write_http_response(stream, status, &body);
}

fn parse_http_request(stream: &mut TcpStream) -> Option<CapturedRequest> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let method = parts[0].to_string();
    let path = parts[1].to_string();

    let (headers, content_length) = parse_headers(&mut reader);

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok()?;
    }

    Some(CapturedRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn parse_headers(reader: &mut BufReader<TcpStream>) -> (Vec<(String, String)>, usize) {
    let lines = read_header_lines(reader);
    let headers: Vec<(String, String)> =
        lines.iter().filter_map(|l| parse_header_line(l)).collect();
    let content_length = extract_content_length(&headers);
    (headers, content_length)
}

fn read_header_lines(reader: &mut BufReader<TcpStream>) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            break;
        }
        if line.trim().is_empty() {
            break;
        }
        lines.push(line);
    }
    lines
}

fn parse_header_line(line: &str) -> Option<(String, String)> {
    let (k, v) = line.trim().split_once(':')?;
    Some((k.trim().to_string(), v.trim().to_string()))
}

fn extract_content_length(headers: &[(String, String)]) -> usize {
    headers
        .iter()
        .find(|(k, _)| k.to_lowercase() == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0)
}

fn write_http_response(stream: &mut TcpStream, status: u16, body: &str) {
    let response = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        status,
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn test_fake_http_server_reads_fragmented_request_on_nonblocking_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_nonblocking(true).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let body = r#"{"event":true}"#;
        write!(
            client,
            "POST /tracking HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            &body[..6]
        )
        .unwrap();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let requests = Arc::clone(&captured);
        let (done, finished) = mpsc::channel();
        let worker = thread::spawn(move || {
            let handler: RequestHandler = Arc::new(|_| (200, "{}".to_string()));
            handle_connection(&mut stream, &requests, &handler);
            done.send(()).unwrap();
        });
        assert_eq!(
            finished.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout),
            "The server must wait for the remaining request body"
        );
        client.write_all(body[6..].as_bytes()).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        worker.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].body, body);
    }
}

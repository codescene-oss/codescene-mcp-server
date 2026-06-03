//! A minimal HTTPS server for testing TLS certificate handling.
//!
//! Uses `rcgen` for certificate generation and `rustls` for TLS.
//! Provides a self-signed CA that issues server certificates, allowing
//! tests to verify that the MCP server respects `REQUESTS_CA_BUNDLE`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// A captured HTTP request from the fake server.
pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Result of generating a CA and server certificate pair.
pub struct GeneratedCerts {
    pub ca_cert_path: PathBuf,
}

/// A running HTTPS server with a self-signed certificate.
pub struct FakeHttpsServer {
    port: u16,
    #[allow(dead_code)]
    shutdown: Arc<Mutex<bool>>,
    pub certs: GeneratedCerts,
    captured_requests: Arc<Mutex<Vec<CapturedRequest>>>,
}

impl FakeHttpsServer {
    /// Start an HTTPS server that responds to `/api/v2/projects` requests.
    ///
    /// Generates a fresh CA and server certificate, writes the CA cert to
    /// `ca_cert_path` inside `cert_dir`, and listens on a random port.
    pub fn start_projects_api(cert_dir: &Path) -> Self {
        Self::start(cert_dir, |req| {
            let path = &req.path;
            if path.contains("/api/v2/projects") {
                if path.contains("page=1") || !path.contains("page=") {
                    return (200, r#"[{"id":1,"name":"Test Project"}]"#.to_string());
                }
                return (200, "[]".to_string());
            }
            (200, "{}".to_string())
        })
    }

    /// Start an HTTPS server with a custom handler function.
    pub fn start(
        cert_dir: &Path,
        handler: impl Fn(&CapturedRequest) -> (u16, String) + Send + Sync + 'static,
    ) -> Self {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok(); // ignore if already installed
        let (certs, tls_config) = build_tls_config(cert_dir);
        let listener = TcpListener::bind(format!("{}:0", super::fake_server_bind_host()))
            .expect("bind HTTPS");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();

        let acceptor = Arc::new(rustls::ServerConfig::from(tls_config));
        let shutdown = Arc::new(Mutex::new(false));
        let captured_requests: Arc<Mutex<Vec<CapturedRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::clone(&shutdown);
        let reqs = Arc::clone(&captured_requests);
        let handler: Arc<dyn Fn(&CapturedRequest) -> (u16, String) + Send + Sync> =
            Arc::new(handler);

        thread::spawn(move || serve_loop(listener, ServerState {
            tls_config: acceptor,
            shutdown: stop,
            captured: reqs,
            handler,
        }));

        FakeHttpsServer {
            port,
            shutdown,
            certs,
            captured_requests,
        }
    }

    /// Start an HTTPS server that always responds with 200 OK and `{}`.
    pub fn always_ok(cert_dir: &Path) -> Self {
        Self::start(cert_dir, |_| (200, "{}".to_string()))
    }

    pub fn url(&self) -> String {
        format!("https://{}:{}", super::fake_server_url_host(), self.port)
    }

    #[allow(dead_code)]
    pub fn shutdown(&self) {
        *self.shutdown.lock().unwrap() = true;
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
}

// ---------------------------------------------------------------------------
// TLS setup
// ---------------------------------------------------------------------------

fn build_tls_config(cert_dir: &Path) -> (GeneratedCerts, rustls::ServerConfig) {
    let ca_key = rcgen::KeyPair::generate().expect("CA key");
    let ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("CA params");
    let ca = ca_params.self_signed(&ca_key).expect("self-sign CA");

    let ca_pem = ca.pem();
    let ca_cert_path = cert_dir.join("ca.crt");
    std::fs::write(&ca_cert_path, &ca_pem).expect("write CA cert");

    let issuer = rcgen::Issuer::from_params(&ca_params, ca_key);

    let mut server_params = rcgen::CertificateParams::new(vec![
        "localhost".to_string(),
        "host.docker.internal".to_string(),
    ])
        .expect("server params");
    server_params.subject_alt_names.push(rcgen::SanType::IpAddress(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
    ));

    let server_key = rcgen::KeyPair::generate().expect("server key");
    let server_cert = server_params
        .signed_by(&server_key, &issuer)
        .expect("sign server cert");

    let cert_chain = vec![
        rustls::pki_types::CertificateDer::from(server_cert.der().to_vec()),
    ];
    let private_key =
        rustls::pki_types::PrivateKeyDer::Pkcs8(server_key.serialize_der().into());

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .expect("rustls server config");

    let certs = GeneratedCerts {
        ca_cert_path,
    };
    (certs, config)
}

// ---------------------------------------------------------------------------
// Accept loop
// ---------------------------------------------------------------------------

struct ServerState {
    tls_config: Arc<rustls::ServerConfig>,
    shutdown: Arc<Mutex<bool>>,
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    handler: Arc<dyn Fn(&CapturedRequest) -> (u16, String) + Send + Sync>,
}

fn serve_loop(listener: TcpListener, state: ServerState) {
    while !*state.shutdown.lock().unwrap() {
        match listener.accept() {
            Ok((tcp_stream, _)) => {
                let acceptor = rustls::ServerConnection::new(Arc::clone(&state.tls_config));
                if let Ok(acceptor) = acceptor {
                    handle_tls_connection(tcp_stream, acceptor, &state.captured, &state.handler);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

fn handle_tls_connection(
    tcp: std::net::TcpStream,
    server_conn: rustls::ServerConnection,
    captured: &Arc<Mutex<Vec<CapturedRequest>>>,
    handler: &Arc<dyn Fn(&CapturedRequest) -> (u16, String) + Send + Sync>,
) {
    tcp.set_nonblocking(false).ok();
    let mut tls = rustls::StreamOwned::new(server_conn, tcp);

    let Some(request) = parse_request(&mut tls) else {
        return;
    };

    let (status, body) = handler(&request);
    captured.lock().unwrap().push(request);
    write_response(&mut tls, status, &body);
}

fn parse_request(stream: &mut impl Read) -> Option<CapturedRequest> {
    let mut reader = BufReader::new(stream);
    let (method, path) = parse_request_line(&mut reader)?;
    let (headers, content_length) = parse_headers(&mut reader);

    let mut body_buf = vec![0u8; content_length];
    if content_length > 0 {
        let _ = reader.read_exact(&mut body_buf);
    }

    Some(CapturedRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body_buf).to_string(),
    })
}

fn parse_request_line(reader: &mut impl BufRead) -> Option<(String, String)> {
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.len() < 2 { return None; }
    Some((parts[0].to_string(), parts[1].to_string()))
}

fn parse_headers(reader: &mut impl BufRead) -> (Vec<(String, String)>, usize) {
    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
        let Some((k, v)) = line.trim().split_once(':') else { continue };
        let key = k.trim().to_string();
        let val = v.trim().to_string();
        if key.eq_ignore_ascii_case("content-length") {
            content_length = val.parse().unwrap_or(0);
        }
        headers.push((key, val));
    }
    (headers, content_length)
}

fn write_response(stream: &mut impl Write, status: u16, body: &str) {
    let response = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        status,
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

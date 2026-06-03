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
}

impl FakeHttpsServer {
    /// Start an HTTPS server that responds to `/api/v2/projects` requests.
    ///
    /// Generates a fresh CA and server certificate, writes the CA cert to
    /// `ca_cert_path` inside `cert_dir`, and listens on a random port.
    pub fn start_projects_api(cert_dir: &Path) -> Self {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .ok(); // ignore if already installed
        let (certs, tls_config) = build_tls_config(cert_dir);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind HTTPS");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();

        let acceptor = Arc::new(rustls::ServerConfig::from(tls_config));
        let shutdown = Arc::new(Mutex::new(false));
        let stop = Arc::clone(&shutdown);

        thread::spawn(move || serve_loop(listener, acceptor, stop));

        FakeHttpsServer {
            port,
            shutdown,
            certs,
        }
    }

    pub fn url(&self) -> String {
        format!("https://127.0.0.1:{}", self.port)
    }

    #[allow(dead_code)]
    pub fn shutdown(&self) {
        *self.shutdown.lock().unwrap() = true;
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

    let mut server_params = rcgen::CertificateParams::new(vec!["localhost".to_string()])
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

fn serve_loop(
    listener: TcpListener,
    tls_config: Arc<rustls::ServerConfig>,
    shutdown: Arc<Mutex<bool>>,
) {
    while !*shutdown.lock().unwrap() {
        match listener.accept() {
            Ok((tcp_stream, _)) => {
                let acceptor = rustls::ServerConnection::new(Arc::clone(&tls_config));
                if let Ok(acceptor) = acceptor {
                    handle_tls_connection(tcp_stream, acceptor);
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
) {
    tcp.set_nonblocking(false).ok();
    let mut tls = rustls::StreamOwned::new(server_conn, tcp);

    let Some(path) = read_request_path(&mut tls) else {
        return;
    };

    let body = build_response_body(&path);
    write_response(&mut tls, &body);
}

fn read_request_path(stream: &mut impl Read) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;

    let path = request_line
        .split_whitespace()
        .nth(1)?
        .to_string();

    // Consume remaining headers
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
    }
    Some(path)
}

fn build_response_body(path: &str) -> String {
    if path.contains("/api/v2/projects") {
        if path.contains("page=1") || !path.contains("page=") {
            return r#"[{"id":1,"name":"Test Project"}]"#.to_string();
        }
        return "[]".to_string();
    }
    "{}".to_string()
}

fn write_response(stream: &mut impl Write, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

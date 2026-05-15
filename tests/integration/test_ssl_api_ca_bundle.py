#!/usr/bin/env python3
"""
SSL CA bundle integration tests for API tools (reqwest HTTP client).

Tests that the MCP server's Rust HTTP client (reqwest) respects the
REQUESTS_CA_BUNDLE configuration when making API calls, such as
select_project.

This test suite validates:
1. When REQUESTS_CA_BUNDLE points to a custom CA cert, the reqwest
   HTTP client trusts HTTPS servers signed by that CA.
2. Without the CA bundle, connections to such servers correctly fail.

Background:
    The ca_bundle / REQUESTS_CA_BUNDLE config was only wired into the
    Java CLI subprocess (truststore injection). The Rust reqwest client
    used for API tools (select_project, hotspots, goals, ownership)
    ignored it entirely, meaning customers behind corporate proxies with
    custom CAs could not use API-based tools.

Approach:
    We spin up a local HTTPS server with a self-signed certificate, set
    CS_ONPREM_URL to point at it, and call select_project. The test
    expects the call to succeed when REQUESTS_CA_BUNDLE is set to the
    self-signed CA cert, proving reqwest picks it up.
"""

import os
import ssl
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

from test_utils import (
    CargoBackend,
    DockerBackend,
    MCPClient,
    ServerBackend,
    create_git_repo,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)


def _generate_ca_and_server_cert(
    ca_cert_path: Path, ca_key_path: Path,
    server_cert_path: Path, server_key_path: Path,
) -> None:
    """Generate a CA certificate and a server certificate signed by it."""
    # Generate CA key and self-signed CA cert
    subprocess.run(
        [
            "openssl", "req", "-x509", "-newkey", "rsa:2048",
            "-keyout", str(ca_key_path),
            "-out", str(ca_cert_path),
            "-days", "1",
            "-nodes",
            "-subj", "/CN=Test CA",
        ],
        check=True,
        capture_output=True,
    )
    # Generate server key and CSR
    server_csr = server_key_path.parent / "server.csr"
    subprocess.run(
        [
            "openssl", "req", "-newkey", "rsa:2048",
            "-keyout", str(server_key_path),
            "-out", str(server_csr),
            "-nodes",
            "-subj", "/CN=localhost",
        ],
        check=True,
        capture_output=True,
    )
    # Sign server cert with CA, including SAN extension
    ext_file = server_key_path.parent / "ext.cnf"
    ext_file.write_text("subjectAltName=DNS:localhost,IP:127.0.0.1\n")
    subprocess.run(
        [
            "openssl", "x509", "-req",
            "-in", str(server_csr),
            "-CA", str(ca_cert_path),
            "-CAkey", str(ca_key_path),
            "-CAcreateserial",
            "-out", str(server_cert_path),
            "-days", "1",
            "-extfile", str(ext_file),
        ],
        check=True,
        capture_output=True,
    )


class _FakeCodeSceneHandler(BaseHTTPRequestHandler):
    """Minimal handler that responds to the projects API endpoint."""

    def do_GET(self):
        if "/api/v2/projects" in self.path:
            # Return data only on page 1; empty array on subsequent pages
            # to stop the pagination loop in query_api_list_with_client.
            if "page=1" in self.path or "page=" not in self.path:
                body = b'[{"id":1,"name":"Test Project"}]'
            else:
                body = b'[]'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, fmt, *args):
        pass  # suppress noisy request logs


def _start_https_server(cert_path: Path, key_path: Path) -> tuple[HTTPServer, int]:
    """Start an HTTPS server on a random port with the given cert/key."""
    server = HTTPServer(("127.0.0.1", 0), _FakeCodeSceneHandler)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(certfile=str(cert_path), keyfile=str(key_path))
    server.socket = ctx.wrap_socket(server.socket, server_side=True)
    port = server.server_address[1]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, port


def _call_select_project(command: list[str], env: dict[str, str], cwd: str) -> tuple[str, str]:
    """Call select_project via MCP and return (result_text, stderr)."""
    client = MCPClient(command, env=env, cwd=cwd)
    try:
        if not client.start():
            return f"START_ERROR: {client.get_stderr()}", client.get_stderr()
        client.initialize()
        response = client.call_tool("select_project", {}, timeout=30)
        return extract_result_text(response), client.get_stderr()
    finally:
        client.stop()


def test_api_uses_ca_bundle(
    command: list[str], env: dict[str, str], repo_dir: Path,
) -> bool:
    """Test that select_project succeeds when REQUESTS_CA_BUNDLE is set."""
    print_header("Test: API Tool Trusts Custom CA Bundle")

    result, _stderr = _call_select_project(command, env, str(repo_dir))
    print(f"  Response preview: {result[:300]}")

    # If the CA bundle is honoured, the TLS handshake succeeds and we
    # get back our fake project data. We check for "Test Project" which
    # is the project name returned by our fake server.
    has_error = "error" in result.lower()
    has_project_data = "test project" in result.lower()

    ok = has_project_data and not has_error
    print_test("API call succeeded with custom CA", ok)
    if has_error:
        print("  FAIL: reqwest did not use REQUESTS_CA_BUNDLE")
        print(f"  Error: {result[:400]}")
    if not has_project_data:
        print("  FAIL: response did not contain expected 'Test Project' data")
    return ok


def test_api_fails_without_ca_bundle(
    command: list[str], env: dict[str, str], repo_dir: Path,
) -> bool:
    """Test that select_project fails when connecting to self-signed without CA bundle."""
    print_header("Test: API Tool Rejects Unknown CA Without Bundle")

    case_env = env.copy()
    case_env.pop("REQUESTS_CA_BUNDLE", None)
    case_env.pop("SSL_CERT_FILE", None)
    case_env.pop("CURL_CA_BUNDLE", None)

    result, stderr = _call_select_project(command, case_env, str(repo_dir))
    print(f"  Response preview: {result[:300]}")

    # Without the CA bundle, reqwest should reject the self-signed cert.
    # The server must have started (not a START_ERROR) and the response
    # should contain an HTTP/TLS transport error, not project data.
    is_start_error = result.startswith("START_ERROR")
    has_transport_error = "error" in result.lower() and not is_start_error
    has_no_project_data = "test project" not in result.lower()

    ok = has_transport_error and has_no_project_data and not is_start_error
    print_test("API call correctly failed without CA bundle", ok)
    if is_start_error:
        print("  FAIL: server did not start — test is inconclusive")
    return ok


def run_ssl_api_ca_bundle_tests(executable: Path) -> int:
    backend = CargoBackend(executable=executable)
    return run_ssl_api_ca_bundle_tests_with_backend(backend)


def run_ssl_api_ca_bundle_tests_with_backend(backend: ServerBackend) -> int:
    if isinstance(backend, DockerBackend):
        # The HTTPS test server runs on the host. Docker containers cannot
        # reach host 127.0.0.1, and the CA cert file is not mounted into the
        # container, so these tests are not applicable for the Docker backend.
        return print_summary([
            ("API tool trusts custom CA bundle (REQUESTS_CA_BUNDLE)", "SKIPPED"),
            ("API tool rejects unknown CA without bundle", "SKIPPED"),
        ])

    with safe_temp_directory(prefix="cs_mcp_ssl_api_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        # Generate CA cert + server cert signed by it
        ca_cert_path = test_dir / "ca.crt"
        ca_key_path = test_dir / "ca.key"
        server_cert_path = test_dir / "server.crt"
        server_key_path = test_dir / "server.key"
        _generate_ca_and_server_cert(
            ca_cert_path, ca_key_path, server_cert_path, server_key_path,
        )
        print(f"  CA cert: {ca_cert_path}")
        print(f"  Server cert: {server_cert_path}")

        # Start local HTTPS server with the server cert
        server, port = _start_https_server(server_cert_path, server_key_path)
        print(f"  HTTPS server on port: {port}")

        try:
            repo_dir = create_git_repo(test_dir, get_sample_files())
            command = backend.get_command(repo_dir)
            base_env = backend.get_env(os.environ.copy(), repo_dir)
            base_env["CS_ACCESS_TOKEN"] = base_env.get("CS_ACCESS_TOKEN", "test-token")
            base_env["CS_DISABLE_VERSION_CHECK"] = "1"
            base_env["CS_DISABLE_TRACKING"] = "1"
            base_env["CS_ONPREM_URL"] = f"https://127.0.0.1:{port}"
            base_env["REQUESTS_CA_BUNDLE"] = str(ca_cert_path)

            results = [
                (
                    "API tool trusts custom CA bundle (REQUESTS_CA_BUNDLE)",
                    test_api_uses_ca_bundle(command, base_env, repo_dir),
                ),
                (
                    "API tool rejects unknown CA without bundle",
                    test_api_fails_without_ca_bundle(command, base_env, repo_dir),
                ),
            ]

            return print_summary(results)
        finally:
            server.shutdown()


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_ssl_api_ca_bundle.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("SSL API CA Bundle Integration Tests")
    print("\nThese tests verify that the reqwest HTTP client used for API")
    print("tools (select_project, etc.) respects REQUESTS_CA_BUNDLE.")

    return run_ssl_api_ca_bundle_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

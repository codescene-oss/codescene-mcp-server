#!/usr/bin/env python3
"""
CloudFront-compatible HTTP header integration tests.

Verifies that both the API client and ACE client send the HTTP headers
required for CloudFront compatibility:

  - User-Agent: codescene-mcp/<version>
  - Accept: application/json
  - Authorization: Bearer <token>  (only when the token is non-empty)
  - Content-Type: application/json (ACE client only)

This test suite validates:
1. API client headers (exercised via select_project -> api_client.rs)
2. ACE client headers (exercised via code_health_auto_refactor -> ace_client.rs)

These tests prevent regressions like the one where ace_client.rs was missing
the User-Agent header, causing CloudFront to return 403 errors.
"""

import json
import os
import sys
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

from test_utils import (
    DockerBackend,
    MCPClient,
    CargoBackend,
    ServerBackend,
    create_git_repo,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)


# ---------------------------------------------------------------------------
# Fake HTTP server that captures request headers
# ---------------------------------------------------------------------------

class _HeaderCapturingHandler(BaseHTTPRequestHandler):
    """HTTP handler that records request headers from incoming requests."""

    captured_requests: list[dict] = []
    _lock = threading.Lock()
    _response_queue: list[tuple[int, str]] = []

    def _handle_request(self):
        """Record headers and respond with the next queued response."""
        content_length = int(self.headers.get("Content-Length", 0))
        if content_length > 0:
            self.rfile.read(content_length)

        header_dict = {k: v for k, v in self.headers.items()}
        with self._lock:
            _HeaderCapturingHandler.captured_requests.append({
                "method": self.command,
                "path": self.path,
                "headers": header_dict,
            })
            if self._response_queue:
                status, body = self._response_queue.pop(0)
            else:
                status, body = 200, "[]"

        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body.encode())

    do_GET = _handle_request
    do_POST = _handle_request

    def log_message(self, format, *args):
        """Suppress request logging to keep test output clean."""
        pass

    @classmethod
    def reset(cls, responses: list[tuple[int, str]] | None = None):
        """Clear state and optionally queue canned responses."""
        with cls._lock:
            cls.captured_requests = []
            cls._response_queue = list(responses) if responses else []

    @classmethod
    def get_captured_requests(cls) -> list[dict]:
        """Return a copy of all captured requests (thread-safe)."""
        with cls._lock:
            return list(cls.captured_requests)


# ---------------------------------------------------------------------------
# Server and client lifecycle helpers
# ---------------------------------------------------------------------------

def _start_fake_server(
    is_docker: bool, responses: list[tuple[int, str]] | None = None,
) -> tuple[HTTPServer, str]:
    """Start a local HTTP server and return (server, base_url)."""
    bind_host = "0.0.0.0" if is_docker else "127.0.0.1"
    _HeaderCapturingHandler.reset(responses)

    server = HTTPServer((bind_host, 0), _HeaderCapturingHandler)
    port = server.server_address[1]
    threading.Thread(target=server.serve_forever, daemon=True).start()

    url_host = "host.docker.internal" if is_docker else "127.0.0.1"
    return server, f"http://{url_host}:{port}"


def _build_env(backend: ServerBackend, repo_dir: Path, extra_env: dict[str, str]) -> dict:
    """Build the subprocess environment with common overrides."""
    env = backend.get_env(os.environ.copy(), repo_dir)
    env.update(extra_env)
    env["CS_DISABLE_VERSION_CHECK"] = "1"
    env["CS_DISABLE_TRACKING"] = "1"
    return env


def _call_and_capture(
    backend: ServerBackend, repo_dir: Path, extra_env: dict[str, str], tool_call: tuple,
) -> list[dict] | None:
    """Start server, call a tool, return captured requests (or None on failure).

    ``tool_call`` is ``(tool_name, tool_args, timeout)``.
    """
    env = _build_env(backend, repo_dir, extra_env)
    command = backend.get_command(repo_dir)

    client = MCPClient(command, env=env, cwd=str(repo_dir))
    try:
        if not client.start():
            print_test("Server started", False)
            return None
        print_test("Server started", True)
        client.initialize()

        tool_name, tool_args, timeout = tool_call
        client.call_tool(tool_name, tool_args, timeout=timeout)
        return _HeaderCapturingHandler.get_captured_requests()
    finally:
        client.stop()


# ---------------------------------------------------------------------------
# Header assertion helpers
# ---------------------------------------------------------------------------

def _find_header(headers: dict, name: str) -> str | None:
    """Find a header value by case-insensitive key lookup."""
    lower = name.lower()
    for key, value in headers.items():
        if key.lower() == lower:
            return value
    return None


def _assert_header(headers: dict, name: str, check_fn, label: str) -> bool:
    """Assert a header property, print the result, and return success."""
    value = _find_header(headers, name) or "MISSING"
    ok = check_fn(value)
    print_test(label, ok, f"Value: {value}")
    return ok


def _assert_common_headers(headers: dict) -> bool:
    """Assert that User-Agent, Accept, and Authorization headers are correct."""
    ua_ok = _assert_header(
        headers, "User-Agent",
        lambda v: v.startswith("codescene-mcp/"),
        "Has User-Agent: codescene-mcp/*",
    )
    accept_ok = _assert_header(
        headers, "Accept",
        lambda v: "application/json" in v,
        "Has Accept: application/json",
    )
    auth_ok = _assert_header(
        headers, "Authorization",
        lambda v: v.startswith("Bearer ") and len(v) > len("Bearer "),
        "Has Authorization: Bearer <token>",
    )
    return ua_ok and accept_ok and auth_ok


def _extract_first_request_headers(
    requests: list[dict], server_label: str,
) -> dict | None:
    """Check that at least one request was captured and return its headers."""
    has_requests = len(requests) > 0
    print_test(f"Fake {server_label} server received requests", has_requests, f"Count: {len(requests)}")
    return requests[0]["headers"] if has_requests else None


# ---------------------------------------------------------------------------
# Test: API client sends correct headers (via select_project)
# ---------------------------------------------------------------------------

def test_api_client_headers(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that the API client sends User-Agent, Accept, and Authorization headers."""
    print_header("Test: API Client Sends CloudFront-Compatible Headers")

    is_docker = isinstance(backend, DockerBackend)
    api_responses = [
        (200, json.dumps([{"id": 1, "name": "Test Project"}])),
        (200, "[]"),
    ]
    server, base_url = _start_fake_server(is_docker, api_responses)

    try:
        extra_env = {
            "CS_ONPREM_URL": base_url,
            "CS_ACCESS_TOKEN": "test-token-for-header-check",
        }
        requests = _call_and_capture(
            backend, repo_dir, extra_env, ("select_project", {}, 60),
        )
        if requests is None:
            return False

        headers = _extract_first_request_headers(requests, "API")
        if headers is None:
            return False

        return _assert_common_headers(headers)

    except Exception as e:
        print_test("API client headers", False, str(e))
        return False
    finally:
        server.shutdown()


# ---------------------------------------------------------------------------
# Test: ACE client sends correct headers (via code_health_auto_refactor)
# ---------------------------------------------------------------------------

def test_ace_client_headers(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that the ACE client sends User-Agent, Accept, Content-Type, and Authorization headers."""
    print_header("Test: ACE Client Sends CloudFront-Compatible Headers")

    is_docker = isinstance(backend, DockerBackend)
    ace_response = json.dumps({"refactored-code": "def process_order(): pass"})
    server, base_url = _start_fake_server(is_docker, [(200, ace_response)])

    try:
        extra_env = {
            "CS_ACE_API_URL": base_url,
            "CS_ACE_ACCESS_TOKEN": "test-ace-token-for-header-check",
        }
        test_file = str(repo_dir / "src/services/order_processor.py")
        tool_args = {"file_path": test_file, "function_name": "process_order"}
        requests = _call_and_capture(
            backend, repo_dir, extra_env, ("code_health_auto_refactor", tool_args, 120),
        )
        if requests is None:
            return False

        headers = _extract_first_request_headers(requests, "ACE")
        if headers is None:
            return False

        common_ok = _assert_common_headers(headers)
        ct_ok = _assert_header(
            headers, "Content-Type",
            lambda v: "application/json" in v,
            "Has Content-Type: application/json",
        )
        return common_ok and ct_ok

    except Exception as e:
        print_test("ACE client headers", False, str(e))
        return False
    finally:
        server.shutdown()


# ---------------------------------------------------------------------------
# Backend-aware runner
# ---------------------------------------------------------------------------

def run_cloudfront_headers_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all CloudFront header tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_cloudfront_headers_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        results = [
            (
                "CloudFront Headers - API client sends required headers",
                test_api_client_headers(backend, repo_dir),
            ),
            (
                "CloudFront Headers - ACE client sends required headers",
                test_ace_client_headers(backend, repo_dir),
            ),
        ]

        return print_summary(results)


# ---------------------------------------------------------------------------
# CargoBackend convenience wrapper
# ---------------------------------------------------------------------------

def run_cloudfront_headers_tests(executable: Path) -> int:
    """
    Run all CloudFront header tests.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = CargoBackend(executable=executable)
    return run_cloudfront_headers_tests_with_backend(backend)


# ---------------------------------------------------------------------------
# Standalone entry point
# ---------------------------------------------------------------------------

def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_cloudfront_headers.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("CloudFront-Compatible Headers Integration Tests")
    print("\nThese tests verify that both the API client and ACE client")
    print("send the HTTP headers required for CloudFront compatibility.")

    return run_cloudfront_headers_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

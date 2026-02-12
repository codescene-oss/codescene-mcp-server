#!/usr/bin/env python3
"""
Analytics tracking integration tests.

Tests that MCP tool calls are never blocked by analytics tracking, even when
the analytics endpoint is unreachable.  This validates the fix for the
customer-reported issue where tool calls took ~3 minutes because the
synchronous tracking POST hung on an unreachable endpoint.

This validates:
1. Tool calls complete promptly when the analytics endpoint is unreachable
2. Response times are not inflated by analytics timeout penalties
3. Analytics events are still delivered when the endpoint is reachable
"""

import os
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

from test_utils import (
    DockerBackend,
    MCPClient,
    NuitkaBackend,
    ServerBackend,
    create_git_repo,
    extract_code_health_score,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)

# RFC 5737 non-routable address — guaranteed to be unreachable.
UNREACHABLE_ANALYTICS_URL = "http://192.0.2.1:1"


class _FakeAnalyticsHandler(BaseHTTPRequestHandler):
    """Minimal handler that mimics the CodeScene analytics tracking endpoint."""

    request_count = 0
    request_count_lock = threading.Lock()

    def do_POST(self):
        with self.request_count_lock:
            _FakeAnalyticsHandler.request_count += 1
        # Consume the request body to avoid broken-pipe errors on the client.
        content_length = int(self.headers.get("Content-Length", 0))
        self.rfile.read(content_length)
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b"{}")

    def log_message(self, format, *args):
        """Suppress request logging to keep test output clean."""
        pass

    @classmethod
    def reset_request_count(cls):
        """Reset the shared request counter to zero."""
        with cls.request_count_lock:
            cls.request_count = 0

    @classmethod
    def get_request_count(cls) -> int:
        """Return the current request count (thread-safe)."""
        with cls.request_count_lock:
            return cls.request_count


def run_analytics_tracking_tests(executable: Path) -> int:
    """Run all analytics tracking tests using a Nuitka executable."""
    backend = NuitkaBackend(executable=executable)
    return run_analytics_tracking_tests_with_backend(backend)


def run_analytics_tracking_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all analytics tracking tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_analytics_tracking_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        results = [
            (
                "Analytics Tracking - Tool responds when analytics unreachable",
                test_tool_responds_when_analytics_unreachable(backend, repo_dir),
            ),
            (
                "Analytics Tracking - Response time not delayed by analytics",
                test_response_time_not_delayed_by_analytics(backend, repo_dir),
            ),
            (
                "Analytics Tracking - Events are sent when endpoint reachable",
                test_analytics_events_are_sent(backend, repo_dir),
            ),
            (
                "Analytics Tracking - Disabled tracking sends no events",
                test_disabled_tracking_sends_no_events(backend, repo_dir),
            ),
            (
                "Analytics Tracking - Disabled tracking still returns valid results",
                test_disabled_tracking_returns_valid_results(backend, repo_dir),
            ),
        ]

        return print_summary(results)


def _call_tool_and_extract_score(
    backend: ServerBackend, repo_dir: Path, extra_env: dict[str, str] | None = None
) -> tuple[MCPClient, str | None, float | None]:
    """Start MCP server, call code_health_score, and return (client, result_text, score).

    The caller is responsible for stopping the client (use try/finally).
    """
    env = backend.get_env(os.environ.copy(), repo_dir)
    if extra_env:
        env.update(extra_env)
    command = backend.get_command(repo_dir)

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    if not client.start():
        print_test("Server started", False)
        return client, None, None

    print_test("Server started", True)
    client.initialize()

    test_file = repo_dir / "src/utils/calculator.py"
    response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
    result_text = extract_result_text(response)
    score = extract_code_health_score(result_text)
    return client, result_text, score


def _run_with_fake_server_and_count_requests(
    backend: ServerBackend, repo_dir: Path, extra_env: dict[str, str] | None = None
) -> tuple[float | None, int]:
    """Start a fake analytics server, make a tool call, and return (score, request_count).

    Handles server lifecycle and client cleanup internally.
    """
    is_docker = isinstance(backend, DockerBackend)
    bind_host = "0.0.0.0" if is_docker else "127.0.0.1"

    _FakeAnalyticsHandler.reset_request_count()

    server = HTTPServer((bind_host, 0), _FakeAnalyticsHandler)
    port = server.server_address[1]
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    url_host = "host.docker.internal" if is_docker else "127.0.0.1"
    local_url = f"http://{url_host}:{port}"
    print(f"  Local analytics server at {local_url}")

    merged_env = {"CS_TRACKING_URL": local_url}
    if extra_env:
        merged_env.update(extra_env)

    client, _result_text, score = None, None, None
    try:
        client, _result_text, score = _call_tool_and_extract_score(backend, repo_dir, merged_env)

        # Wait briefly for background tracking threads to deliver (or not).
        time.sleep(2)

        return score, _FakeAnalyticsHandler.get_request_count()
    finally:
        if client:
            client.stop()
        server.shutdown()


def test_tool_responds_when_analytics_unreachable(backend: ServerBackend, repo_dir: Path) -> bool:
    """
    Test that code_health_score returns a valid result when the analytics
    endpoint is unreachable.

    This is the primary regression test for the customer-reported issue
    where an unreachable analytics endpoint caused tool calls to hang for
    ~3 minutes (the OS TCP connect timeout).
    """
    print_header("Test: Tool Responds When Analytics Unreachable")

    client = None
    try:
        client, result_text, score = _call_tool_and_extract_score(
            backend, repo_dir, {"CS_TRACKING_URL": UNREACHABLE_ANALYTICS_URL}
        )

        if result_text is None:
            return False

        has_content = len(result_text) > 0
        print_test("Tool returned content", has_content, f"Length: {len(result_text)} chars")

        has_score = score is not None
        print_test("Response contains a valid Code Health score", has_score, f"Score: {score}")

        return has_content and has_score

    except Exception as e:
        print_test("Tool responds when analytics unreachable", False, str(e))
        return False
    finally:
        if client:
            client.stop()


def test_response_time_not_delayed_by_analytics(backend: ServerBackend, repo_dir: Path) -> bool:
    """
    Test that tool responses are not delayed by analytics tracking timeouts.

    With background tracking, tool calls should complete well within a
    reasonable time budget even when the analytics endpoint is unreachable.
    We use a generous 30-second budget (same as the version check test) to
    account for the actual Code Health analysis time in CI. The key
    assertion is that the response does NOT take an *extra* 2-3 minutes due
    to a blocking analytics POST.
    """
    print_header("Test: Response Time Not Delayed By Analytics")

    env = backend.get_env(os.environ.copy(), repo_dir)
    env["CS_TRACKING_URL"] = UNREACHABLE_ANALYTICS_URL
    command = backend.get_command(repo_dir)

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/utils/calculator.py"
        print(f"\n  Analyzing: {test_file}")

        start = time.monotonic()
        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        elapsed = time.monotonic() - start

        result_text = extract_result_text(response)
        has_content = len(result_text) > 0

        within_budget = elapsed < 30
        print_test(
            "Response within time budget (<30s)",
            within_budget,
            f"Elapsed: {elapsed:.2f}s",
        )
        print_test("Response has content", has_content)

        return within_budget and has_content

    except Exception as e:
        print_test("Response time not delayed by analytics", False, str(e))
        return False
    finally:
        client.stop()


def test_analytics_events_are_sent(backend: ServerBackend, repo_dir: Path) -> bool:
    """
    Test that analytics events are delivered when the endpoint is reachable.

    Starts a local HTTP server that mimics the analytics endpoint, then
    makes a tool call and verifies that the server received at least one
    POST request. This confirms that the background tracking mechanism
    still fires events when the endpoint is available.

    For Docker backends, the server binds to 0.0.0.0 and the URL uses
    host.docker.internal so the container can reach back to the host.
    """
    print_header("Test: Analytics Events Are Sent")

    try:
        score, request_count = _run_with_fake_server_and_count_requests(backend, repo_dir)

        has_score = score is not None
        print_test("Valid Code Health score", has_score, f"Score: {score}")
        if not has_score:
            return False

        has_events = request_count > 0
        print_test(
            "Analytics endpoint received events",
            has_events,
            f"Requests received: {request_count}",
        )

        return has_events

    except Exception as e:
        print_test("Analytics events are sent", False, str(e))
        return False


def test_disabled_tracking_sends_no_events(backend: ServerBackend, repo_dir: Path) -> bool:
    """
    Test that no analytics events are sent when CS_DISABLE_TRACKING is set.

    Starts a local HTTP server, sets CS_DISABLE_TRACKING=1 alongside
    CS_TRACKING_URL pointing at the local server, makes a tool call, and
    verifies that the server received zero POST requests.
    """
    print_header("Test: Disabled Tracking Sends No Events")

    try:
        score, request_count = _run_with_fake_server_and_count_requests(
            backend, repo_dir, {"CS_DISABLE_TRACKING": "1"}
        )

        has_score = score is not None
        print_test("Valid Code Health score", has_score, f"Score: {score}")
        if not has_score:
            return False

        no_events = request_count == 0
        print_test(
            "No analytics events sent",
            no_events,
            f"Requests received: {request_count}",
        )

        return no_events

    except Exception as e:
        print_test("Disabled tracking sends no events", False, str(e))
        return False


def test_disabled_tracking_returns_valid_results(backend: ServerBackend, repo_dir: Path) -> bool:
    """
    Test that tool calls still return valid results when tracking is disabled.

    This ensures that CS_DISABLE_TRACKING only suppresses analytics — it
    must not interfere with the actual Code Health analysis.
    """
    print_header("Test: Disabled Tracking Returns Valid Results")

    client = None
    try:
        client, result_text, score = _call_tool_and_extract_score(
            backend, repo_dir, {"CS_DISABLE_TRACKING": "1"}
        )

        if result_text is None:
            return False

        has_content = len(result_text) > 0
        print_test("Tool returned content", has_content, f"Length: {len(result_text)} chars")

        has_score = score is not None
        print_test("Response contains a valid Code Health score", has_score, f"Score: {score}")

        return has_content and has_score

    except Exception as e:
        print_test("Disabled tracking returns valid results", False, str(e))
        return False
    finally:
        if client:
            client.stop()


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_analytics_tracking.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Analytics Tracking Integration Tests")
    print("\nThese tests verify that MCP tool calls are never blocked by")
    print("analytics tracking, even when the endpoint is unreachable.")

    return run_analytics_tracking_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""
Version check integration tests.

Tests that MCP tool calls work correctly both when the GitHub version check
endpoint is unreachable and when it is reachable and reports a newer version.

This validates:
1. Tool calls complete without being blocked by version check timeouts
2. Responses contain valid tool output (no version-check-related errors)
3. No "VERSION UPDATE AVAILABLE" noise when the check cannot reach GitHub
4. Version info becomes available on subsequent tool calls after the background
   fetch completes (happy path with a local HTTP server)
"""

import json
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
UNREACHABLE_VERSION_CHECK_URL = "http://192.0.2.1:1/fake"

# Fake version that will always differ from the server's real version.
FAKE_LATEST_VERSION = "MCP-99.99.99"


class _FakeGitHubHandler(BaseHTTPRequestHandler):
    """Minimal handler that mimics the GitHub releases/latest endpoint."""

    request_count = 0
    request_count_lock = threading.Lock()

    def do_GET(self):
        with self.request_count_lock:
            _FakeGitHubHandler.request_count += 1
        body = json.dumps({"tag_name": FAKE_LATEST_VERSION}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

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


def run_version_check_tests(executable: Path) -> int:
    """Run all version check tests using a Nuitka executable."""
    backend = NuitkaBackend(executable=executable)
    return run_version_check_tests_with_backend(backend)


def run_version_check_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all version check tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_version_check_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        command = backend.get_command(repo_dir)
        env = backend.get_env(os.environ.copy(), repo_dir)

        # These tests need the version checker active, so remove the default
        # disable that get_env() sets for non-version-check tests.
        env.pop("CS_DISABLE_VERSION_CHECK", None)
        # Point version check at an unreachable address
        env["CS_VERSION_CHECK_URL"] = UNREACHABLE_VERSION_CHECK_URL

        results = [
            (
                "Version Check - Tool responds when GitHub unreachable",
                test_tool_responds_when_github_unreachable(command, env, repo_dir),
            ),
            (
                "Version Check - No version update noise",
                test_no_version_update_noise(command, env, repo_dir),
            ),
            (
                "Version Check - Response time acceptable",
                test_response_time_acceptable(command, env, repo_dir),
            ),
            (
                "Version Check - Version info appears after background fetch",
                test_version_info_appears_after_background_fetch(backend, repo_dir),
            ),
            (
                "Version Check - Disabled: no banner despite newer version",
                test_disabled_version_check_no_banner(backend, repo_dir),
            ),
            (
                "Version Check - Disabled: no network traffic",
                test_disabled_version_check_no_network_traffic(backend, repo_dir),
            ),
        ]

        return print_summary(results)


def test_tool_responds_when_github_unreachable(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that code_health_score returns a valid result when the version
    check endpoint is unreachable.
    """
    print_header("Test: Tool Responds When GitHub Unreachable")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/utils/calculator.py"
        print(f"\n  Analyzing: {test_file}")

        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        result_text = extract_result_text(response)

        has_content = len(result_text) > 0
        print_test("Tool returned content", has_content, f"Length: {len(result_text)} chars")

        score = extract_code_health_score(result_text)
        has_score = score is not None
        print_test("Response contains a valid Code Health score", has_score, f"Score: {score}")

        return has_content and has_score

    except Exception as e:
        print_test("Tool responds when GitHub unreachable", False, str(e))
        return False
    finally:
        client.stop()


def test_no_version_update_noise(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that no VERSION UPDATE AVAILABLE banner appears when the version
    check endpoint is unreachable (failed fetches should be cached silently).
    """
    print_header("Test: No Version Update Noise")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/utils/calculator.py"

        # Call the tool twice — the second call exercises the cached-failure path
        for i in range(2):
            print(f"\n  Tool call {i + 1}...")
            response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
            result_text = extract_result_text(response)

            has_version_noise = "VERSION UPDATE AVAILABLE" in result_text
            print_test(f"Call {i + 1}: no VERSION UPDATE AVAILABLE", not has_version_noise)

            if has_version_noise:
                return False

        return True

    except Exception as e:
        print_test("No version update noise", False, str(e))
        return False
    finally:
        client.stop()


def test_response_time_acceptable(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that tool responses are not delayed by version check timeouts.

    With the non-blocking background fetch, even the first tool call should
    return well under the 5-second version check timeout. We allow a generous
    30-second budget to account for the actual Code Health analysis time in CI,
    but the key assertion is that the response does NOT take an *extra* 5+
    seconds due to a blocking version check.
    """
    print_header("Test: Response Time Acceptable")

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

        # The response should arrive within a reasonable window.
        # The actual code health analysis takes time, but it should not be
        # inflated by an extra 5-second version check timeout.
        within_budget = elapsed < 30
        print_test(
            "Response within time budget (<30s)",
            within_budget,
            f"Elapsed: {elapsed:.2f}s",
        )
        print_test("Response has content", has_content)

        return within_budget and has_content

    except Exception as e:
        print_test("Response time acceptable", False, str(e))
        return False
    finally:
        client.stop()


def test_version_info_appears_after_background_fetch(backend: ServerBackend, repo_dir: Path) -> bool:
    """
    Test that version info becomes available on a subsequent tool call.

    Starts a local HTTP server that returns a fake "newer" version, then:
      1. Call 1 — background fetch starts; no version banner expected yet.
      2. Brief pause for the fast local fetch to complete.
      3. Calls 2..N — cached result should now include VERSION UPDATE AVAILABLE.

    This verifies the core async contract: first call triggers, later calls
    benefit from the cached result.

    For Docker backends, the server binds to 0.0.0.0 and the URL uses
    host.docker.internal so the container can reach back to the host.
    """
    print_header("Test: Version Info Appears After Background Fetch")

    is_docker = isinstance(backend, DockerBackend)

    # Bind to 0.0.0.0 when running under Docker so the container can connect
    # back to the host; 127.0.0.1 otherwise for tighter security in native runs.
    bind_host = "0.0.0.0" if is_docker else "127.0.0.1"

    _FakeGitHubHandler.reset_request_count()

    # Start a local HTTP server on an ephemeral port
    server = HTTPServer((bind_host, 0), _FakeGitHubHandler)
    port = server.server_address[1]
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    # Docker containers reach the host via host.docker.internal (added with
    # --add-host in DockerBackend.get_command); native runs use 127.0.0.1.
    url_host = "host.docker.internal" if is_docker else "127.0.0.1"
    local_url = f"http://{url_host}:{port}/releases/latest"
    print(f"  Local version server at {local_url}")

    # Build env with the local URL override
    env = backend.get_env(os.environ.copy(), repo_dir)
    # This test needs the version checker active, so remove the default
    # disable that get_env() sets for non-version-check tests.
    env.pop("CS_DISABLE_VERSION_CHECK", None)
    env["CS_VERSION_CHECK_URL"] = local_url
    command = backend.get_command(repo_dir)

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/utils/calculator.py"

        # --- Call 1: triggers background fetch, version not expected yet ---
        print("\n  Call 1 (triggers background fetch)...")
        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        result_text_1 = extract_result_text(response)

        score = extract_code_health_score(result_text_1)
        has_score = score is not None
        print_test("Call 1: valid Code Health score", has_score, f"Score: {score}")

        if not has_score:
            return False

        # Brief pause — the local server responds instantly, so the background
        # thread should complete well within this window.
        time.sleep(2)

        # --- Calls 2..4: the cached result should now include the version banner ---
        version_appeared = False
        for i in range(2, 5):
            print(f"\n  Call {i} (checking for cached version info)...")
            response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
            result_text = extract_result_text(response)

            if "VERSION UPDATE AVAILABLE" in result_text:
                version_appeared = True
                print_test(f"Call {i}: VERSION UPDATE AVAILABLE present", True)

                has_fake_version = FAKE_LATEST_VERSION in result_text
                print_test(f"Call {i}: mentions fake version {FAKE_LATEST_VERSION}", has_fake_version)
                break
            else:
                print_test(f"Call {i}: VERSION UPDATE AVAILABLE present", False, "not yet")

        print_test("Version info appeared on a subsequent call", version_appeared)
        return version_appeared

    except Exception as e:
        print_test("Version info appears after background fetch", False, str(e))
        return False
    finally:
        client.stop()
        server.shutdown()


def test_disabled_version_check_no_banner(backend: ServerBackend, repo_dir: Path) -> bool:
    """
    Test that CS_DISABLE_VERSION_CHECK suppresses the VERSION UPDATE AVAILABLE
    banner even when a newer version is available.

    Starts a local HTTP server that would report a newer version, sets the
    disable flag, and verifies no version banner ever appears across multiple
    tool calls.
    """
    print_header("Test: Disabled Version Check — No Banner")

    is_docker = isinstance(backend, DockerBackend)
    bind_host = "0.0.0.0" if is_docker else "127.0.0.1"

    server = HTTPServer((bind_host, 0), _FakeGitHubHandler)
    port = server.server_address[1]
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    url_host = "host.docker.internal" if is_docker else "127.0.0.1"
    local_url = f"http://{url_host}:{port}/releases/latest"
    print(f"  Local version server at {local_url}")

    env = backend.get_env(os.environ.copy(), repo_dir)
    env["CS_VERSION_CHECK_URL"] = local_url
    env["CS_DISABLE_VERSION_CHECK"] = "1"
    command = backend.get_command(repo_dir)

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/utils/calculator.py"

        # First call: validate we get a valid score, and no banner
        print("\n  Call 1...")
        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        result_text = extract_result_text(response)
        has_banner = "VERSION UPDATE AVAILABLE" in result_text
        print_test("Call 1: no VERSION UPDATE AVAILABLE", not has_banner)

        score = extract_code_health_score(result_text)
        print_test("Call 1: valid Code Health score", score is not None, f"Score: {score}")
        if score is None or has_banner:
            return False

        time.sleep(1)

        # Subsequent calls: verify banner stays suppressed
        all_clean = True
        for i in range(2, 4):
            print(f"\n  Call {i}...")
            response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
            result_text = extract_result_text(response)

            has_banner = "VERSION UPDATE AVAILABLE" in result_text
            print_test(f"Call {i}: no VERSION UPDATE AVAILABLE", not has_banner)
            all_clean = all_clean and not has_banner

            time.sleep(1)

        print_test("Version banner suppressed on all calls", all_clean)
        return all_clean

    except Exception as e:
        print_test("Disabled version check — no banner", False, str(e))
        return False
    finally:
        client.stop()
        server.shutdown()


def test_disabled_version_check_no_network_traffic(backend: ServerBackend, repo_dir: Path) -> bool:
    """
    Test that CS_DISABLE_VERSION_CHECK prevents all network traffic to the
    version check endpoint.

    Starts a local HTTP server and counts incoming requests. With the version
    check disabled, zero requests should arrive.
    """
    print_header("Test: Disabled Version Check — No Network Traffic")

    is_docker = isinstance(backend, DockerBackend)
    bind_host = "0.0.0.0" if is_docker else "127.0.0.1"

    _FakeGitHubHandler.reset_request_count()

    server = HTTPServer((bind_host, 0), _FakeGitHubHandler)
    port = server.server_address[1]
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    url_host = "host.docker.internal" if is_docker else "127.0.0.1"
    local_url = f"http://{url_host}:{port}/releases/latest"
    print(f"  Local version server at {local_url}")

    env = backend.get_env(os.environ.copy(), repo_dir)
    env["CS_VERSION_CHECK_URL"] = local_url
    env["CS_DISABLE_VERSION_CHECK"] = "1"
    command = backend.get_command(repo_dir)

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/utils/calculator.py"

        # Make several tool calls
        for i in range(1, 4):
            print(f"\n  Call {i}...")
            response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
            result_text = extract_result_text(response)

            score = extract_code_health_score(result_text)
            print_test(f"Call {i}: valid Code Health score", score is not None, f"Score: {score}")

        # Brief pause to allow any stray background requests to arrive
        time.sleep(2)

        request_count = _FakeGitHubHandler.get_request_count()
        no_traffic = request_count == 0
        print_test(
            "Zero requests to version endpoint",
            no_traffic,
            f"Requests received: {request_count}",
        )

        return no_traffic

    except Exception as e:
        print_test("Disabled version check — no network traffic", False, str(e))
        return False
    finally:
        client.stop()
        server.shutdown()


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_version_check.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Version Check Integration Tests")
    print("\nThese tests verify that MCP tool calls work correctly when the")
    print("GitHub version check endpoint is unreachable.")

    return run_version_check_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

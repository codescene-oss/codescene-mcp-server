#!/usr/bin/env python3
"""
Error logging and telemetry redaction integration tests.

Tests that the MCP server:
1. Sends only safe error kind labels (not raw stderr) to telemetry
2. Logs full error details to a file when file logging is enabled
3. Respects the log_retention_days=0 setting to disable file logging

This prevents sensitive information (tokens, file paths, credentials)
from leaking through the analytics/telemetry channel while ensuring
errors are still captured locally for debugging.
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
    CargoBackend,
    ServerBackend,
    create_git_repo,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)

# Known safe error kind values that should appear in telemetry
SAFE_ERROR_KINDS = {
    "non_zero_exit",
    "not_found",
    "io",
    "invalid_input",
    "license_check_failed",
    "http",
    "transport",
    "status",
    "api_error",
}


class _FakeAnalyticsHandler(BaseHTTPRequestHandler):
    """Captures analytics payloads for inspection."""

    request_count = 0
    request_count_lock = threading.Lock()
    captured_payloads: list[dict] = []

    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length)
        with self.request_count_lock:
            _FakeAnalyticsHandler.request_count += 1
            try:
                payload = json.loads(body)
                _FakeAnalyticsHandler.captured_payloads.append(payload)
            except (json.JSONDecodeError, TypeError):
                pass
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b"{}")

    def log_message(self, format, *args):
        pass

    @classmethod
    def reset(cls):
        with cls.request_count_lock:
            cls.request_count = 0
            cls.captured_payloads = []

    @classmethod
    def get_captured_payloads(cls) -> list[dict]:
        with cls.request_count_lock:
            return list(cls.captured_payloads)


def run_error_logging_tests(executable: Path) -> int:
    """Run all error logging tests using a Cargo executable."""
    backend = CargoBackend(executable=executable)
    return run_error_logging_tests_with_backend(backend)


def run_error_logging_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all error logging tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_error_logging_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        results = [
            (
                "Error Telemetry - Only error kind sent, not raw stderr",
                test_error_telemetry_sends_only_kind(backend, repo_dir, test_dir),
            ),
            (
                "Error Telemetry - Invalid token reports license_check_failed",
                test_error_telemetry_invalid_token(backend, repo_dir, test_dir),
            ),
            (
                "Error Logging - Full error logged to file",
                test_error_logged_to_file(backend, repo_dir, test_dir),
            ),
            (
                "Error Logging - File logging disabled when retention is 0",
                test_file_logging_disabled_when_zero(backend, repo_dir, test_dir),
            ),
        ]

        return print_summary(results)


def _trigger_error_with_fake_server(
    backend: ServerBackend,
    repo_dir: Path,
    config_dir: Path,
    extra_env: dict[str, str] | None = None,
) -> tuple[str, list[dict]]:
    """Trigger a tool error with a fake analytics server.

    Calls code_health_score on a non-existent file to reliably produce a
    CliError::NonZeroExit or CliError::InvalidInput error.

    For Docker backends, *config_dir* must be inside *repo_dir* so it is
    accessible via the bind mount.  The env var is translated to the
    container path automatically.

    Returns (result_text, captured_payloads).
    """
    is_docker = isinstance(backend, DockerBackend)
    bind_host = "0.0.0.0" if is_docker else "127.0.0.1"

    _FakeAnalyticsHandler.reset()

    server = HTTPServer((bind_host, 0), _FakeAnalyticsHandler)
    port = server.server_address[1]
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    url_host = "host.docker.internal" if is_docker else "127.0.0.1"
    local_url = f"http://{url_host}:{port}"
    print(f"  Local analytics server at {local_url}")

    env = backend.get_env(os.environ.copy(), repo_dir)
    env["CS_TRACKING_URL"] = local_url

    if is_docker:
        # config_dir must be relative to repo_dir; translate to container path
        relative = config_dir.relative_to(repo_dir)
        env["CS_CONFIG_DIR"] = f"/mount/{relative}"
    else:
        env["CS_CONFIG_DIR"] = str(config_dir)

    if extra_env:
        env.update(extra_env)

    command = backend.get_command(repo_dir)
    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return "", []

        print_test("Server started", True)
        client.initialize()

        # Call with a non-existent file to trigger an error
        nonexistent = str(repo_dir / "does_not_exist_xyz.py")
        print(f"  Calling code_health_score on non-existent file: {nonexistent}")
        response = client.call_tool(
            "code_health_score", {"file_path": nonexistent}, timeout=60
        )
        result_text = extract_result_text(response)

        # Wait for background tracking to deliver
        time.sleep(2)

        return result_text, _FakeAnalyticsHandler.get_captured_payloads()
    finally:
        client.stop()
        server.shutdown()


def _extract_error_payloads(payloads: list[dict]) -> list[dict]:
    """Return only the error tracking payloads."""
    return [p for p in payloads if "error" in p.get("event-type", "")]


def _get_error_kind(payload: dict) -> str:
    """Extract the error kind string from a tracking payload."""
    return payload.get("event-properties", {}).get("error", "")


def _assert_error_event_received(
    result_text: str, payloads: list[dict],
) -> tuple[bool, list[dict]]:
    """Verify a response was received and contains error events.

    Returns (ok, error_payloads).
    """
    has_response = len(result_text) > 0
    print_test("Tool returned error response", has_response)

    error_payloads = _extract_error_payloads(payloads)
    has_error_event = len(error_payloads) > 0
    print_test(
        "Error tracking event sent",
        has_error_event,
        f"Found {len(error_payloads)} error event(s)",
    )
    return has_response and has_error_event, error_payloads


def _assert_payload_is_safe(payload: dict) -> bool:
    """Verify a single error payload contains only a safe kind label."""
    error_value = _get_error_kind(payload)
    is_safe = error_value in SAFE_ERROR_KINDS
    print_test("Error value is safe kind label", is_safe, f"Got: '{error_value}'")

    no_stderr_leak = "exited with code" not in error_value
    no_path_leak = "/" not in error_value and "\\" not in error_value
    print_test("No raw stderr in telemetry", no_stderr_leak)
    print_test("No file paths in telemetry", no_path_leak)

    return is_safe and no_stderr_leak and no_path_leak


def _read_log_content(log_dir: Path) -> str:
    """Read and concatenate all log files in a directory."""
    return "".join(f.read_text(errors="replace") for f in log_dir.iterdir())


def test_error_telemetry_sends_only_kind(
    backend: ServerBackend, repo_dir: Path, test_dir: Path
) -> bool:
    """
    Test that error telemetry contains only the error kind label.

    Triggers a tool error and inspects the analytics payload to verify
    that no raw stderr, file paths, or sensitive data is transmitted.
    """
    print_header("Test: Error Telemetry Sends Only Kind")

    config_dir = repo_dir / ".config_telemetry"
    config_dir.mkdir(parents=True, exist_ok=True)

    try:
        result_text, payloads = _trigger_error_with_fake_server(
            backend, repo_dir, config_dir
        )

        ok, error_payloads = _assert_error_event_received(result_text, payloads)
        if not ok:
            return False

        return all(_assert_payload_is_safe(p) for p in error_payloads)

    except Exception as e:
        print_test("Error telemetry sends only kind", False, str(e))
        return False


def test_error_telemetry_invalid_token(
    backend: ServerBackend, repo_dir: Path, test_dir: Path
) -> bool:
    """
    Test that an invalid access token produces a license_check_failed error kind.
    """
    print_header("Test: Invalid Token Reports license_check_failed")

    config_dir = repo_dir / ".config_invalid_token"
    config_dir.mkdir(parents=True, exist_ok=True)

    try:
        result_text, payloads = _trigger_error_with_fake_server(
            backend,
            repo_dir,
            config_dir,
            extra_env={"CS_ACCESS_TOKEN": "invalid-garbage-token-xyz"},
        )

        ok, error_payloads = _assert_error_event_received(result_text, payloads)
        if not ok:
            return False

        error_value = _get_error_kind(error_payloads[0])
        is_license_error = error_value == "license_check_failed"
        print_test("Error kind is license_check_failed", is_license_error, f"Got: '{error_value}'")

        no_token_leak = "invalid-garbage-token-xyz" not in error_value
        print_test("Token value not in telemetry", no_token_leak)

        return is_license_error and no_token_leak

    except Exception as e:
        print_test("Invalid token error telemetry", False, str(e))
        return False


def test_error_logged_to_file(
    backend: ServerBackend, repo_dir: Path, test_dir: Path
) -> bool:
    """
    Test that full error details are logged to a file.
    """
    print_header("Test: Full Error Logged to File")

    config_dir = repo_dir / ".config_logging"
    config_dir.mkdir(parents=True, exist_ok=True)

    try:
        result_text, _payloads = _trigger_error_with_fake_server(
            backend, repo_dir, config_dir, extra_env={"CS_LOG_RETENTION_DAYS": "7"}
        )

        has_response = len(result_text) > 0
        print_test("Tool returned error response", has_response)

        log_dir = config_dir / "logs"
        log_dir_exists = log_dir.exists() and log_dir.is_dir()
        print_test("Log directory created", log_dir_exists, f"Path: {log_dir}")
        if not log_dir_exists:
            return False

        log_files = list(log_dir.iterdir())
        has_log_files = len(log_files) > 0
        print_test("Log files created", has_log_files, f"Found {len(log_files)} file(s)")
        if not has_log_files:
            return False

        log_content = _read_log_content(log_dir)
        has_error_logged = "error" in log_content.lower()
        print_test("Error details in log file", has_error_logged, f"Log size: {len(log_content)} chars")

        detail_markers = ["does_not_exist_xyz", "no such file", "not a supported", "non_zero_exit", "invalid_input"]
        has_detail = any(m in log_content.lower() for m in detail_markers)
        print_test("Log contains error detail", has_detail)

        return has_response and has_log_files and has_error_logged

    except Exception as e:
        print_test("Full error logged to file", False, str(e))
        return False


def test_file_logging_disabled_when_zero(
    backend: ServerBackend, repo_dir: Path, test_dir: Path
) -> bool:
    """
    Test that file logging is disabled when log_retention_days=0.

    Sets CS_LOG_RETENTION_DAYS=0 and verifies that no log directory
    or log files are created.
    """
    print_header("Test: File Logging Disabled When Retention Is 0")

    config_dir = repo_dir / ".config_no_logging"
    config_dir.mkdir(parents=True, exist_ok=True)

    try:
        result_text, _payloads = _trigger_error_with_fake_server(
            backend, repo_dir, config_dir, extra_env={"CS_LOG_RETENTION_DAYS": "0"}
        )

        has_response = len(result_text) > 0
        print_test("Tool returned error response", has_response)

        # Log directory should NOT be created
        log_dir = config_dir / "logs"
        no_log_dir = not log_dir.exists()
        print_test(
            "No log directory created",
            no_log_dir,
            f"Path: {log_dir} (exists={log_dir.exists()})",
        )

        return has_response and no_log_dir

    except Exception as e:
        print_test("File logging disabled when zero", False, str(e))
        return False


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_error_logging.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Error Logging Integration Tests")
    print("\nThese tests verify error telemetry redaction and file logging.")

    return run_error_logging_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

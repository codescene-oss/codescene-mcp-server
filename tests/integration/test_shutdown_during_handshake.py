#!/usr/bin/env python3
"""
Shutdown-during-handshake integration tests.

Tests that the MCP server exits cleanly (exit code 0) when the client
disconnects during the MCP initialization handshake. Some MCP clients
(notably VS Code and Zed) tear down agents by closing the server's
stdin before the handshake completes — for example when the client
shuts down quickly after launch, or when the user closes the agent
before any tool is invoked.

When that happens, the server must NOT report a fatal error: the
client surfaces a non-zero exit as a fatal crash dialog, e.g.:

    .../cs-mcp has encountered a fatal error and was closed.

This test suite validates:
1. Closing stdin before any input results in exit code 0.
2. Closing stdin after the initialize request but before the
   notifications/initialized notification results in exit code 0.
3. A full handshake followed by stdin close still results in exit
   code 0 (sanity check that the happy path also stays clean).
4. (Unix only) SIGTERM before any input results in exit code 0.
5. (Unix only) SIGTERM after a full handshake results in exit code 0.
"""

import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from test_utils import (
    CargoBackend,
    NpmBackend,
    ServerBackend,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)

# Hard upper bound for how long the server may take to exit after
# stdin is closed. If exceeded, the test fails — a stuck server
# process is just as bad as a crashing one.
_EXIT_TIMEOUT_SECONDS = 10


def _spawn_server(command: list[str], env: dict, cwd: str) -> subprocess.Popen:
    """Start the MCP server with stdio pipes captured."""
    return subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        cwd=cwd,
        text=True,
        encoding="utf-8",
        bufsize=1,
    )


def _send_message(process: subprocess.Popen, message: dict) -> None:
    """Write a single JSON-RPC message and flush."""
    assert process.stdin is not None
    process.stdin.write(json.dumps(message) + "\n")
    process.stdin.flush()


def _stop_and_wait(
    process: subprocess.Popen, stop: callable
) -> tuple[int | None, str]:
    """Execute *stop* action, wait for the process to exit, return (exit_code, stderr)."""
    stop(process)
    try:
        process.wait(timeout=_EXIT_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
        return None, _drain_stderr(process)
    return process.returncode, _drain_stderr(process)


def _close_stdin(process: subprocess.Popen) -> None:
    assert process.stdin is not None
    process.stdin.close()


def _close_stdin_and_wait(process: subprocess.Popen) -> tuple[int | None, str]:
    """Close stdin, wait for the process to exit, return (exit_code, stderr)."""
    return _stop_and_wait(process, _close_stdin)


def _drain_stderr(process: subprocess.Popen) -> str:
    """Read any remaining stderr output without blocking forever."""
    if process.stderr is None:
        return ""
    try:
        return process.stderr.read() or ""
    except Exception:
        return ""


def _initialize_request() -> dict:
    return {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "shutdown-test", "version": "1.0.0"},
        },
    }


def _initialized_notification() -> dict:
    return {"jsonrpc": "2.0", "method": "notifications/initialized"}


def _check_clean_exit(exit_code: int | None, stderr: str, scenario: str) -> bool:
    """Assert the server exited cleanly within the timeout."""
    if exit_code is None:
        print_test(
            f"Server exited within {_EXIT_TIMEOUT_SECONDS}s ({scenario})",
            False,
            "Server hung after stdin close and had to be killed",
        )
        return False
    print_test(
        f"Server exited within {_EXIT_TIMEOUT_SECONDS}s ({scenario})",
        True,
    )

    is_clean = exit_code == 0
    print_test(
        f"Exit code is 0 ({scenario})",
        is_clean,
        (
            f"Got exit code {exit_code}. Recent stderr:\n"
            f"{_tail(stderr)}"
        ),
    )
    return is_clean


def _tail(text: str, max_lines: int = 10) -> str:
    lines = text.strip().splitlines()
    return "\n".join(lines[-max_lines:]) if lines else "<empty>"


def _sigterm_and_wait(process: subprocess.Popen) -> tuple[int | None, str]:
    """Send SIGTERM to the process, wait for exit, return (exit_code, stderr)."""
    return _stop_and_wait(
        process, lambda p: p.send_signal(signal.SIGTERM)
    )


# --- Scenarios ---


def test_stdin_closed_before_any_input(
    command: list[str], env: dict, cwd: str
) -> bool:
    """
    Closing stdin before sending any JSON-RPC message must exit cleanly.

    This simulates a client that launches the server and immediately
    disconnects (e.g. crashed, or shut down between launch and first
    request).
    """
    print_header("Test: Stdin closed before any input")

    process = _spawn_server(command, env, cwd)
    # Give the server a moment to boot before we close stdin.
    time.sleep(0.3)
    exit_code, stderr = _close_stdin_and_wait(process)
    return _check_clean_exit(exit_code, stderr, "no input")


def test_stdin_closed_after_initialize_request(
    command: list[str], env: dict, cwd: str
) -> bool:
    """
    Closing stdin after the initialize request but before the
    notifications/initialized notification must exit cleanly.

    This is the exact scenario reported by VS Code / Zed users:
    closing the agent during the handshake produced a "fatal error"
    dialog because the server exited with code 1.
    """
    print_header("Test: Stdin closed mid-handshake")

    process = _spawn_server(command, env, cwd)
    _send_message(process, _initialize_request())
    # Wait for the server to read the initialize request before we
    # close stdin, so the server is in the "waiting for initialized
    # notification" state when stdin closes.
    time.sleep(0.5)
    exit_code, stderr = _close_stdin_and_wait(process)
    return _check_clean_exit(exit_code, stderr, "mid-handshake")


def test_stdin_closed_after_full_handshake(
    command: list[str], env: dict, cwd: str
) -> bool:
    """
    Closing stdin after a complete handshake must still exit cleanly.

    Sanity check: the bug fix must not regress the already-working
    happy path where the client completes initialization and then
    disconnects.
    """
    print_header("Test: Stdin closed after full handshake")

    process = _spawn_server(command, env, cwd)
    _send_message(process, _initialize_request())
    time.sleep(0.3)
    _send_message(process, _initialized_notification())
    time.sleep(0.3)
    exit_code, stderr = _close_stdin_and_wait(process)
    return _check_clean_exit(exit_code, stderr, "post-handshake")


def test_sigterm_before_any_input(
    command: list[str], env: dict, cwd: str
) -> bool:
    """
    Sending SIGTERM before any JSON-RPC message must exit cleanly.

    MCP clients like Zed may terminate the server process with SIGTERM
    instead of (or in addition to) closing stdin. The npm wrapper must
    translate that into exit code 0 so the client does not surface a
    "fatal error" dialog.
    """
    print_header("Test: SIGTERM before any input")

    process = _spawn_server(command, env, cwd)
    time.sleep(0.3)
    exit_code, stderr = _sigterm_and_wait(process)
    return _check_clean_exit(exit_code, stderr, "SIGTERM, no input")


def test_sigterm_after_full_handshake(
    command: list[str], env: dict, cwd: str
) -> bool:
    """
    Sending SIGTERM after a complete handshake must exit cleanly.

    This is the most common real-world scenario: the user closes the
    agent after it has been fully initialized, and the client sends
    SIGTERM to the server process group.
    """
    print_header("Test: SIGTERM after full handshake")

    process = _spawn_server(command, env, cwd)
    _send_message(process, _initialize_request())
    time.sleep(0.3)
    _send_message(process, _initialized_notification())
    time.sleep(0.3)
    exit_code, stderr = _sigterm_and_wait(process)
    return _check_clean_exit(exit_code, stderr, "SIGTERM, post-handshake")


# --- Runner ---


def run_shutdown_during_handshake_tests_with_backend(backend: ServerBackend) -> int:
    """Run the shutdown-during-handshake tests using a backend."""
    with safe_temp_directory(prefix="cs_mcp_shutdown_handshake_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        command = backend.get_command(test_dir)
        env = backend.get_env(os.environ.copy(), test_dir)
        cwd = str(test_dir)

        results = [
            (
                "Stdin Closed Before Any Input",
                test_stdin_closed_before_any_input(command, env, cwd),
            ),
            (
                "Stdin Closed After Initialize Request",
                test_stdin_closed_after_initialize_request(command, env, cwd),
            ),
            (
                "Stdin Closed After Full Handshake",
                test_stdin_closed_after_full_handshake(command, env, cwd),
            ),
        ]

        # SIGTERM tests only apply on Unix and only when running through the
        # npm wrapper, which is responsible for translating signals into a
        # clean exit code. The bare Rust binary does not trap SIGTERM.
        if sys.platform != "win32" and isinstance(backend, NpmBackend):
            results.extend([
                (
                    "SIGTERM Before Any Input",
                    test_sigterm_before_any_input(command, env, cwd),
                ),
                (
                    "SIGTERM After Full Handshake",
                    test_sigterm_after_full_handshake(command, env, cwd),
                ),
            ])

        return print_summary(results)


def run_shutdown_during_handshake_tests(executable: Path) -> int:
    """Run all shutdown-during-handshake tests with a Cargo executable."""
    backend = CargoBackend(executable=executable)
    return run_shutdown_during_handshake_tests_with_backend(backend)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_shutdown_during_handshake.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Shutdown-During-Handshake Integration Tests")
    print(
        "\nThese tests verify that the MCP server exits with code 0 when\n"
        "the client closes stdin during the MCP initialization handshake.\n"
        "Non-zero exit codes are surfaced by VS Code and Zed as a fatal\n"
        "error dialog, even though the client itself triggered the close."
    )

    return run_shutdown_during_handshake_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

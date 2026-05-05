#!/usr/bin/env python3
"""
Analytics environment override integration tests.

Validates analytics payload environment behavior for:
1. Default environment when CS_ENVIRONMENT is not set.
2. Overridden environment when CS_ENVIRONMENT is set.
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
    extract_code_health_score,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)


class _FakeAnalyticsHandler(BaseHTTPRequestHandler):
    """Captures analytics payloads sent by the MCP server."""

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


def _run_with_fake_server(
    backend: ServerBackend, repo_dir: Path, extra_env: dict[str, str] | None = None
) -> tuple[float | None, list[dict]]:
    """Run code_health_score with fake analytics server and return (score, payloads)."""
    is_docker = isinstance(backend, DockerBackend)
    bind_host = "0.0.0.0" if is_docker else "127.0.0.1"

    _FakeAnalyticsHandler.reset()

    server = HTTPServer((bind_host, 0), _FakeAnalyticsHandler)
    port = server.server_address[1]
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    url_host = "host.docker.internal" if is_docker else "127.0.0.1"
    local_url = f"http://{url_host}:{port}"

    env = backend.get_env(os.environ.copy(), repo_dir)
    env.pop("CS_DISABLE_TRACKING", None)
    env["CS_TRACKING_URL"] = local_url
    if extra_env:
        env.update(extra_env)

    command = backend.get_command(repo_dir)
    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return None, []

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/utils/calculator.py"
        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        score = extract_code_health_score(extract_result_text(response))

        time.sleep(2)
        return score, _FakeAnalyticsHandler.get_captured_payloads()
    finally:
        client.stop()
        server.shutdown()


def _first_score_event_properties(payloads: list[dict]) -> dict | None:
    for payload in payloads:
        if payload.get("event-type") == "mcp-code-health-score":
            return payload.get("event-properties", {})
    return None


def test_default_environment_is_sent(backend: ServerBackend, repo_dir: Path) -> bool:
    """When CS_ENVIRONMENT is unset, analytics should send detected runtime environment."""
    print_header("Test: Default Environment Is Sent")

    score, payloads = _run_with_fake_server(backend, repo_dir)

    has_score = score is not None
    print_test("Valid Code Health score", has_score, f"Score: {score}")
    if not has_score:
        return False

    props = _first_score_event_properties(payloads)
    has_event = props is not None
    print_test("Found mcp-code-health-score event", has_event)
    if not has_event:
        return False

    value = props.get("environment")
    is_default = value in ("binary", "docker")
    print_test("Environment is default runtime value", is_default, f"Value: {value}")
    return is_default


def test_overridden_environment_is_sent(backend: ServerBackend, repo_dir: Path) -> bool:
    """When CS_ENVIRONMENT is set, analytics should send the override value."""
    print_header("Test: Overridden Environment Is Sent")

    override = "my-agent-name"
    score, payloads = _run_with_fake_server(backend, repo_dir, {"CS_ENVIRONMENT": override})

    has_score = score is not None
    print_test("Valid Code Health score", has_score, f"Score: {score}")
    if not has_score:
        return False

    props = _first_score_event_properties(payloads)
    has_event = props is not None
    print_test("Found mcp-code-health-score event", has_event)
    if not has_event:
        return False

    value = props.get("environment")
    is_override = value == override
    print_test("Environment override propagated", is_override, f"Value: {value}")
    return is_override


def run_analytics_environment_override_tests(executable: Path) -> int:
    backend = CargoBackend(executable=executable)
    return run_analytics_environment_override_tests_with_backend(backend)


def run_analytics_environment_override_tests_with_backend(backend: ServerBackend) -> int:
    with safe_temp_directory(prefix="cs_mcp_analytics_env_override_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        results = [
            (
                "Analytics Environment - Default environment is sent",
                test_default_environment_is_sent(backend, repo_dir),
            ),
            (
                "Analytics Environment - Overridden environment is sent",
                test_overridden_environment_is_sent(backend, repo_dir),
            ),
        ]

        return print_summary(results)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_analytics_environment_override.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Analytics Environment Override Integration Tests")
    print("\nThese tests verify analytics environment defaults and overrides.")

    return run_analytics_environment_override_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

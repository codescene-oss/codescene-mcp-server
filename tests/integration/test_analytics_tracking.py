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

import hashlib
import json
import os
import subprocess
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

# RFC 5737 non-routable address — guaranteed to be unreachable.
UNREACHABLE_ANALYTICS_URL = "http://192.0.2.1:1"

# Re-use the degrading code fixture from analyze_change_set tests — a Complex
# Conditional smell (3+ logical operators per conditional) that reliably
# triggers delta-analysis findings.
from test_analyze_change_set import CLEAN_ADDITION, DEGRADING_ADDITION


class _FakeAnalyticsHandler(BaseHTTPRequestHandler):
    """Minimal handler that mimics the CodeScene analytics tracking endpoint."""

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
        """Suppress request logging to keep test output clean."""
        pass

    @classmethod
    def reset(cls):
        """Reset the shared request counter and captured payloads."""
        with cls.request_count_lock:
            cls.request_count = 0
            cls.captured_payloads = []

    @classmethod
    def reset_request_count(cls):
        """Reset the shared request counter to zero (legacy helper)."""
        cls.reset()

    @classmethod
    def get_request_count(cls) -> int:
        """Return the current request count (thread-safe)."""
        with cls.request_count_lock:
            return cls.request_count

    @classmethod
    def get_captured_payloads(cls) -> list[dict]:
        """Return a copy of all captured request payloads (thread-safe)."""
        with cls.request_count_lock:
            return list(cls.captured_payloads)


def run_analytics_tracking_tests(executable: Path) -> int:
    """Run all analytics tracking tests using a Cargo executable."""
    backend = CargoBackend(executable=executable)
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

        # Tests that use the shared repo (read-only / score-only).
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
            (
                "Analytics Tracking - Enriched event payloads contain common properties",
                test_enriched_event_contains_common_properties(backend, repo_dir),
            ),
            (
                "Analytics Tracking - Enriched event payloads contain tool-specific properties",
                test_enriched_event_contains_tool_specific_properties(backend, repo_dir),
            ),
            (
                "Analytics Tracking - Enriched review event contains tool-specific properties",
                test_enriched_review_event(backend, repo_dir),
            ),
        ]

        # Tests that mutate the repo — each gets its own fresh copy.
        results.extend(_run_mutating_tests(backend, test_dir))

        return print_summary(results)


def _run_mutating_tests(backend: ServerBackend, test_dir: Path) -> list[tuple[str, bool]]:
    """Run tests that mutate their repo (pre-commit, analyze-change-set, degrading variants)."""
    results: list[tuple[str, bool]] = []

    for subdir, label, test_fn in [
        ("pre_commit", "Enriched pre-commit event", test_enriched_pre_commit_event),
        ("changeset", "Enriched analyze-change-set event", test_enriched_analyze_change_set_event),
        ("pre_commit_dirty", "Enriched pre-commit event with findings", test_enriched_pre_commit_event_with_findings),
        ("changeset_dirty", "Enriched analyze-change-set event with findings", test_enriched_analyze_change_set_event_with_findings),
    ]:
        print(f"\nCreating repository for {subdir} test...")
        repo = create_git_repo(test_dir / subdir, get_sample_files())
        results.append((f"Analytics Tracking - {label}", test_fn(backend, repo)))

    return results


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


def _run_with_fake_server(
    backend: ServerBackend, repo_dir: Path, extra_env: dict[str, str] | None = None
) -> tuple[float | None, int, list[dict]]:
    """Start a fake analytics server, make a tool call, and return (score, request_count, payloads).

    Delegates to ``_run_tool_with_fake_server`` with a score-extracting caller.
    """

    def _score_caller(client: MCPClient, rd: Path) -> str:
        test_file = rd / "src/utils/calculator.py"
        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        return extract_result_text(response)

    result_text, count, payloads = _run_tool_with_fake_server(
        backend, repo_dir, _score_caller, extra_env
    )
    score = extract_code_health_score(result_text) if result_text else None
    return score, count, payloads


def _run_tool_with_fake_server(
    backend: ServerBackend,
    repo_dir: Path,
    tool_caller,
    extra_env: dict[str, str] | None = None,
) -> tuple[str, int, list[dict]]:
    """Start a fake analytics server, call an arbitrary tool, return (result_text, request_count, payloads).

    *tool_caller* receives ``(client, repo_dir)`` and must return the raw
    ``result_text`` string from the tool response.
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

    merged_env = {"CS_TRACKING_URL": local_url}
    if extra_env:
        merged_env.update(extra_env)

    env = backend.get_env(os.environ.copy(), repo_dir)
    env.update(merged_env)
    command = backend.get_command(repo_dir)
    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return "", 0, []

        print_test("Server started", True)
        client.initialize()

        result_text = tool_caller(client, repo_dir)

        # Wait briefly for background tracking threads to deliver.
        time.sleep(2)

        return result_text, _FakeAnalyticsHandler.get_request_count(), _FakeAnalyticsHandler.get_captured_payloads()
    finally:
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
        score, request_count, _payloads = _run_with_fake_server(backend, repo_dir)

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
        score, request_count, _payloads = _run_with_fake_server(
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


def _hash_value(value: str) -> str:
    """Reproduce the 16-char SHA-256 hex prefix used by the server."""
    return hashlib.sha256(value.encode()).hexdigest()[:16]


def _get_score_event_properties(
    backend: ServerBackend, repo_dir: Path
) -> tuple[float, dict] | None:
    """Run a tool call, capture payloads, and return (score, event-properties) for the score event.

    Returns ``None`` (and prints diagnostic test lines) when any prerequisite
    check fails, so callers can simply ``return False``.
    """
    score, _count, payloads = _run_with_fake_server(backend, repo_dir)

    if score is None:
        print_test("Valid Code Health score", False, "Score: None")
        return None
    print_test("Valid Code Health score", True, f"Score: {score}")

    if not payloads:
        print_test("Analytics payloads captured", False, "Count: 0")
        return None
    print_test("Analytics payloads captured", True, f"Count: {len(payloads)}")

    score_payloads = [p for p in payloads if p.get("event-type") == "mcp-code-health-score"]
    if not score_payloads:
        print_test("Found mcp-code-health-score event", False)
        return None
    print_test("Found mcp-code-health-score event", True)

    return score, score_payloads[0].get("event-properties", {})


def _check_common_properties(props: dict) -> bool:
    """Validate that the common enrichment fields are present in *props*."""
    has_instance_id = bool(props.get("instance-id"))
    print_test("Has instance-id", has_instance_id, f"Value: {props.get('instance-id', 'MISSING')}")

    has_environment = props.get("environment") in ("docker", "source", "binary")
    print_test("Has valid environment", has_environment, f"Value: {props.get('environment', 'MISSING')}")

    has_version = bool(props.get("version"))
    print_test("Has version", has_version, f"Value: {props.get('version', 'MISSING')}")

    return has_instance_id and has_environment and has_version


def _check_tool_specific_properties(props: dict, score: float, repo_dir: Path) -> bool:
    """Validate that the tool-specific enrichment fields are present in *props*."""
    test_file = str(repo_dir / "src/utils/calculator.py")
    expected_hash = _hash_value(test_file)
    has_file_hash = props.get("file-hash") == expected_hash
    print_test(
        "Has correct file-hash",
        has_file_hash,
        f"Expected: {expected_hash}, Got: {props.get('file-hash', 'MISSING')}",
    )

    has_score_prop = "score" in props
    print_test("Has score property", has_score_prop, f"Value: {props.get('score', 'MISSING')}")

    score_matches = _score_matches(props, score)
    print_test("Score property matches actual score", score_matches)

    return has_file_hash and has_score_prop and score_matches


def _score_matches(props: dict, expected: float) -> bool:
    """Return True when the ``score`` property in *props* matches *expected*."""
    try:
        return abs(float(props["score"]) - expected) < 0.01
    except (KeyError, ValueError, TypeError):
        return False


def test_enriched_event_contains_common_properties(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that every analytics event includes instance-id, environment, and version."""
    print_header("Test: Enriched Events Contain Common Properties")
    try:
        result = _get_score_event_properties(backend, repo_dir)
        if result is None:
            return False
        _score, props = result
        return _check_common_properties(props)
    except Exception as e:
        print_test("Enriched events contain common properties", False, str(e))
        return False


def test_enriched_event_contains_tool_specific_properties(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that the code_health_score event includes file-hash and the numeric score."""
    print_header("Test: Enriched Events Contain Tool-Specific Properties")
    try:
        result = _get_score_event_properties(backend, repo_dir)
        if result is None:
            return False
        score, props = result
        return _check_tool_specific_properties(props, score, repo_dir)
    except Exception as e:
        print_test("Enriched events contain tool-specific properties", False, str(e))
        return False


# -- Tool-specific callers for _run_tool_with_fake_server --


def _call_review(client: MCPClient, repo_dir: Path) -> str:
    """Call code_health_review and return the result text."""
    test_file = str(repo_dir / "src/services/order_processor.py")
    response = client.call_tool("code_health_review", {"file_path": test_file}, timeout=60)
    return extract_result_text(response)


def _call_pre_commit(client: MCPClient, repo_dir: Path) -> str:
    """Stage a clean modification and call pre_commit_code_health_safeguard."""
    return _stage_and_call_pre_commit(client, repo_dir, "\n# Analytics tracking integration test modification\n")


def _call_pre_commit_dirty(client: MCPClient, repo_dir: Path) -> str:
    """Stage a degrading change and call pre_commit_code_health_safeguard."""
    return _stage_and_call_pre_commit(client, repo_dir, DEGRADING_ADDITION)


def _stage_and_call_pre_commit(client: MCPClient, repo_dir: Path, addition: str) -> str:
    """Append *addition* to calculator.py, stage it, and run the pre-commit safeguard."""
    test_file = repo_dir / "src/utils/calculator.py"
    test_file.write_text(test_file.read_text() + addition)

    subprocess.run(["git", "add", str(test_file)], cwd=repo_dir, check=True, capture_output=True)

    response = client.call_tool(
        "pre_commit_code_health_safeguard",
        {"git_repository_path": str(repo_dir)},
        timeout=60,
    )
    return extract_result_text(response)


def _call_analyze_change_set(client: MCPClient, repo_dir: Path) -> str:
    """Call analyze_change_set against 'master'."""
    response = client.call_tool(
        "analyze_change_set",
        {"base_ref": "master", "git_repository_path": str(repo_dir)},
        timeout=60,
    )
    return extract_result_text(response)


def _find_event(payloads: list[dict], event_type: str) -> dict | None:
    """Find the first payload matching *event_type* and return its event-properties."""
    for p in payloads:
        if p.get("event-type") == event_type:
            return p.get("event-properties", {})
    return None


def _get_event_props_for_tool(
    backend: ServerBackend,
    repo_dir: Path,
    tool_caller,
    event_type: str,
) -> dict | None:
    """Run *tool_caller* via a fake analytics server and return event properties.

    Performs the common preamble checks (content, events received, event found)
    and returns the event-properties dict on success, or ``None`` on failure.
    """
    result_text, count, payloads = _run_tool_with_fake_server(backend, repo_dir, tool_caller)

    has_content = len(result_text) > 0
    print_test("Tool returned content", has_content, f"Length: {len(result_text)} chars")
    if not has_content:
        return None

    has_events = count > 0
    print_test("Analytics endpoint received events", has_events, f"Requests: {count}")
    if not has_events:
        return None

    props = _find_event(payloads, event_type)
    has_event = props is not None
    print_test(f"Found {event_type} event", has_event)
    if not has_event:
        return None

    return props


def _check_prop(props: dict, key: str, label: str, predicate) -> bool:
    """Check a single property in *props* using *predicate*, printing the result."""
    value = props.get(key, "MISSING")
    ok = predicate(value)
    print_test(label, ok, f"Value: {value}")
    return ok


# -- Enriched event tests for code_health_review --


def test_enriched_review_event(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that code_health_review events include file-hash, score, and categories."""
    print_header("Test: Enriched Review Event Contains Tool-Specific Properties")
    try:
        props = _get_event_props_for_tool(
            backend, repo_dir, _call_review, "mcp-code-health-review"
        )
        if props is None:
            return False

        expected_hash = _hash_value(str(repo_dir / "src/services/order_processor.py"))
        checks = [
            _check_prop(props, "file-hash", "Has correct file-hash", lambda v: v == expected_hash),
            _check_prop(props, "score", "Has score property", lambda v: v != "MISSING"),
            _check_prop(props, "categories", "Has categories", lambda v: isinstance(v, list) and len(v) > 0),
            _check_prop(props, "category-count", "Has category-count", lambda v: isinstance(v, int) and v > 0),
            _check_common_properties(props),
        ]
        return all(checks)

    except Exception as e:
        print_test("Enriched review event", False, str(e))
        return False


# -- Enriched event tests for pre_commit_code_health_safeguard --


def test_enriched_pre_commit_event(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that pre_commit_code_health_safeguard events include repo-hash, quality-gates, file-count."""
    print_header("Test: Enriched Pre-Commit Event Contains Tool-Specific Properties")
    try:
        props = _get_event_props_for_tool(
            backend, repo_dir, _call_pre_commit,
            "mcp-pre-commit-code-health-safeguard",
        )
        if props is None:
            return False

        expected_hash = _hash_value(str(repo_dir))
        checks = [
            _check_prop(props, "repo-hash", "Has correct repo-hash", lambda v: v == expected_hash),
            _check_prop(props, "quality-gates", "Has quality-gates", lambda v: v in ("passed", "failed")),
            _check_prop(props, "file-count", "Has file-count", lambda v: isinstance(v, int)),
            _check_common_properties(props),
        ]
        return all(checks)

    except Exception as e:
        print_test("Enriched pre-commit event", False, str(e))
        return False


# -- Enriched event tests for analyze_change_set --


def _create_branch_with_change(repo_dir: Path, addition: str, message: str = "Feature branch change") -> None:
    """Create a feature branch that appends *addition* to calculator.py and commits."""
    subprocess.run(["git", "checkout", "-b", "feature"], cwd=repo_dir, check=True, capture_output=True)

    test_file = repo_dir / "src/utils/calculator.py"
    test_file.write_text(test_file.read_text() + addition)

    subprocess.run(["git", "add", "."], cwd=repo_dir, check=True, capture_output=True)
    subprocess.run(["git", "commit", "-m", message], cwd=repo_dir, check=True, capture_output=True)


def test_enriched_analyze_change_set_event(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that analyze_change_set events include repo-hash, base-ref-hash, quality-gates, file-count."""
    print_header("Test: Enriched Analyze-Change-Set Event Contains Tool-Specific Properties")
    try:
        _create_branch_with_change(repo_dir, CLEAN_ADDITION)

        props = _get_event_props_for_tool(
            backend, repo_dir, _call_analyze_change_set,
            "mcp-analyze-change-set",
        )
        if props is None:
            return False

        expected_repo_hash = _hash_value(str(repo_dir))
        expected_ref_hash = _hash_value("master")
        checks = [
            _check_prop(props, "repo-hash", "Has correct repo-hash", lambda v: v == expected_repo_hash),
            _check_prop(props, "base-ref-hash", "Has correct base-ref-hash", lambda v: v == expected_ref_hash),
            _check_prop(props, "quality-gates", "Has quality-gates", lambda v: v in ("passed", "failed")),
            _check_prop(props, "file-count", "Has file-count", lambda v: isinstance(v, int) and v >= 0),
            _check_common_properties(props),
        ]
        return all(checks)

    except Exception as e:
        print_test("Enriched analyze-change-set event", False, str(e))
        return False


# -- Degrading-change tests (verify categories, verdicts, file-count > 0) --


def _check_delta_findings_props(props: dict, repo_dir: Path) -> bool:
    """Validate that delta-analysis props contain findings metadata (file-count, verdicts, categories)."""
    expected_hash = _hash_value(str(repo_dir))
    checks = [
        _check_prop(props, "repo-hash", "Has correct repo-hash", lambda v: v == expected_hash),
        _check_prop(props, "quality-gates", "Has quality-gates (failed)", lambda v: v == "failed"),
        _check_prop(props, "file-count", "Has file-count > 0", lambda v: isinstance(v, int) and v > 0),
        _check_prop(props, "verdicts", "Has verdicts dict", lambda v: isinstance(v, dict) and len(v) > 0),
        _check_prop(props, "categories", "Has categories list", lambda v: isinstance(v, list) and len(v) > 0),
        _check_prop(
            props, "category-count", "Has category-count > 0", lambda v: isinstance(v, int) and v > 0
        ),
        _check_common_properties(props),
    ]
    return all(checks)


def test_enriched_pre_commit_event_with_findings(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that pre-commit events include verdicts, categories, and category-count when code degrades."""
    print_header("Test: Enriched Pre-Commit Event With Findings")
    try:
        props = _get_event_props_for_tool(
            backend, repo_dir, _call_pre_commit_dirty,
            "mcp-pre-commit-code-health-safeguard",
        )
        if props is None:
            return False

        return _check_delta_findings_props(props, repo_dir)

    except Exception as e:
        print_test("Enriched pre-commit event with findings", False, str(e))
        return False


def test_enriched_analyze_change_set_event_with_findings(backend: ServerBackend, repo_dir: Path) -> bool:
    """Test that analyze-change-set events include verdicts, categories, and category-count when code degrades."""
    print_header("Test: Enriched Analyze-Change-Set Event With Findings")
    try:
        _create_branch_with_change(repo_dir, DEGRADING_ADDITION, "Add degrading code")

        props = _get_event_props_for_tool(
            backend, repo_dir, _call_analyze_change_set,
            "mcp-analyze-change-set",
        )
        if props is None:
            return False

        expected_ref_hash = _hash_value("master")
        has_ref = _check_prop(
            props, "base-ref-hash", "Has correct base-ref-hash", lambda v: v == expected_ref_hash
        )
        return has_ref and _check_delta_findings_props(props, repo_dir)

    except Exception as e:
        print_test("Enriched analyze-change-set event with findings", False, str(e))
        return False


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

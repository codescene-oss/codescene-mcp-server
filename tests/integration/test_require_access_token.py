#!/usr/bin/env python3
"""
Access token guard integration tests.

Tests that the MCP server correctly blocks tool execution when no
CS_ACCESS_TOKEN is configured, returning a helpful error message
instead of crashing or running the tool.

This test suite validates:
1. Without a token, guarded tools return the token-missing message.
2. Without a token, configure tools (get_config, set_config) still work.
3. With a token, guarded tools work normally (sanity check).
"""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from test_utils import (
    MCPClient,
    NuitkaBackend,
    ServerBackend,
    create_git_repo,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)

# Expected substring in the token-missing error message
_TOKEN_MISSING_MARKER = "No access token configured"


# --- Helpers ---


def _make_no_token_env(base_env: dict) -> dict:
    """Return env dict with CS_ACCESS_TOKEN explicitly removed."""
    env = base_env.copy()
    env.pop("CS_ACCESS_TOKEN", None)
    return env


def _start_client(command: list[str], env: dict, cwd: str) -> MCPClient | None:
    """Start and initialize an MCPClient, returning None on failure."""
    client = MCPClient(command, env=env, cwd=cwd)
    if not client.start():
        print_test("Server started", False, client.get_stderr())
        return None
    print_test("Server started", True)
    client.initialize()
    return client


# --- Test functions ---


def test_guarded_tool_blocked_without_token(
    command: list[str], env: dict, cwd: str, repo_dir: Path,
) -> bool:
    """Without CS_ACCESS_TOKEN, a guarded tool returns the token-missing message."""
    print_header("Test: Guarded Tool Blocked Without Token")

    no_token_env = _make_no_token_env(env)
    client = _start_client(command, no_token_env, cwd)
    if client is None:
        return False

    try:
        test_file = repo_dir / "hello.py"
        print(f"\n  Calling code_health_score on: {test_file}")

        response = client.call_tool(
            "code_health_score",
            {"file_path": str(test_file)},
            timeout=30,
        )
        result_text = extract_result_text(response)

        has_marker = _TOKEN_MISSING_MARKER in result_text
        print_test(
            "Response contains token-missing message",
            has_marker,
            f"Response preview: {result_text[:200]}",
        )

        has_set_config_hint = "set_config" in result_text
        print_test("Response mentions set_config", has_set_config_hint)

        return has_marker and has_set_config_hint

    except Exception as e:
        print_test("Guarded tool blocked", False, str(e))
        return False
    finally:
        client.stop()


def test_explain_tool_blocked_without_token(
    command: list[str], env: dict, cwd: str,
) -> bool:
    """Without CS_ACCESS_TOKEN, explain tools also return the token-missing message."""
    print_header("Test: Explain Tool Blocked Without Token")

    no_token_env = _make_no_token_env(env)
    client = _start_client(command, no_token_env, cwd)
    if client is None:
        return False

    try:
        print("\n  Calling explain_code_health...")

        response = client.call_tool(
            "explain_code_health",
            {},
            timeout=30,
        )
        result_text = extract_result_text(response)

        has_marker = _TOKEN_MISSING_MARKER in result_text
        print_test(
            "explain_code_health returns token-missing message",
            has_marker,
            f"Response preview: {result_text[:200]}",
        )

        return has_marker

    except Exception as e:
        print_test("Explain tool blocked", False, str(e))
        return False
    finally:
        client.stop()


def test_get_config_works_without_token(
    command: list[str], env: dict, cwd: str,
) -> bool:
    """Without CS_ACCESS_TOKEN, get_config still works (not guarded)."""
    print_header("Test: get_config Works Without Token")

    no_token_env = _make_no_token_env(env)
    client = _start_client(command, no_token_env, cwd)
    if client is None:
        return False

    try:
        print("\n  Calling get_config...")

        response = client.call_tool(
            "get_config",
            {},
            timeout=30,
        )
        result_text = extract_result_text(response)

        not_blocked = _TOKEN_MISSING_MARKER not in result_text
        print_test(
            "get_config NOT blocked by token guard",
            not_blocked,
            f"Response preview: {result_text[:200]}",
        )

        has_content = len(result_text) > 0
        print_test("get_config returned content", has_content)

        return not_blocked and has_content

    except Exception as e:
        print_test("get_config without token", False, str(e))
        return False
    finally:
        client.stop()


def test_set_config_works_without_token(
    command: list[str], env: dict, cwd: str,
) -> bool:
    """Without CS_ACCESS_TOKEN, set_config still works (not guarded)."""
    print_header("Test: set_config Works Without Token")

    no_token_env = _make_no_token_env(env)
    client = _start_client(command, no_token_env, cwd)
    if client is None:
        return False

    try:
        print("\n  Calling set_config with a test value...")

        response = client.call_tool(
            "set_config",
            {"key": "onprem_url", "value": "https://test.example.com"},
            timeout=30,
        )
        result_text = extract_result_text(response)

        not_blocked = _TOKEN_MISSING_MARKER not in result_text
        print_test(
            "set_config NOT blocked by token guard",
            not_blocked,
            f"Response preview: {result_text[:200]}",
        )

        has_content = len(result_text) > 0
        print_test("set_config returned content", has_content)

        return not_blocked and has_content

    except Exception as e:
        print_test("set_config without token", False, str(e))
        return False
    finally:
        client.stop()


def test_guarded_tool_works_with_token(
    command: list[str], env: dict, cwd: str, repo_dir: Path,
) -> bool:
    """With CS_ACCESS_TOKEN set, guarded tools work normally (sanity check)."""
    print_header("Test: Guarded Tool Works With Token")

    # env already has CS_ACCESS_TOKEN from the real environment
    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        test_file = repo_dir / "hello.py"
        print(f"\n  Calling code_health_score on: {test_file}")

        response = client.call_tool(
            "code_health_score",
            {"file_path": str(test_file)},
            timeout=60,
        )
        result_text = extract_result_text(response)

        not_blocked = _TOKEN_MISSING_MARKER not in result_text
        print_test(
            "Response does NOT contain token-missing message",
            not_blocked,
            f"Response preview: {result_text[:200]}",
        )

        has_content = len(result_text) > 0
        print_test("Tool returned content", has_content)

        return not_blocked and has_content

    except Exception as e:
        print_test("Guarded tool with token", False, str(e))
        return False
    finally:
        client.stop()


# --- Runner ---


def run_require_access_token_tests(executable: Path) -> int:
    """Run all access token guard tests with a Nuitka executable."""
    backend = NuitkaBackend(executable=executable)
    return run_require_access_token_tests_with_backend(backend)


def run_require_access_token_tests_with_backend(backend: ServerBackend) -> int:
    """Run all access token guard tests using a backend.

    Returns exit code (0 for success, 1 for failure).
    """
    with safe_temp_directory(prefix="cs_mcp_token_guard_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        # Minimal git repo needed for server startup
        sample_files = {"hello.py": "def hello():\n    return 'world'\n"}
        repo_dir = create_git_repo(test_dir, sample_files)
        print(f"Repository: {repo_dir}")

        config_dir = test_dir / "config"
        config_dir.mkdir()

        command = backend.get_command(repo_dir)
        base_env = backend.get_env(os.environ.copy(), repo_dir)
        # Use an isolated config dir so set_config tests don't pollute real config
        base_env["CS_CONFIG_DIR"] = str(config_dir)
        cwd = str(repo_dir)

        results: list[tuple[str, bool | str]] = [
            (
                "Guarded tool blocked without token",
                test_guarded_tool_blocked_without_token(command, base_env, cwd, repo_dir),
            ),
            (
                "Explain tool blocked without token",
                test_explain_tool_blocked_without_token(command, base_env, cwd),
            ),
            (
                "get_config works without token",
                test_get_config_works_without_token(command, base_env, cwd),
            ),
            (
                "set_config works without token",
                test_set_config_works_without_token(command, base_env, cwd),
            ),
            (
                "Guarded tool works with token",
                test_guarded_tool_works_with_token(command, base_env, cwd, repo_dir),
            ),
        ]

        return print_summary(results)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_require_access_token.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Access Token Guard Integration Tests")
    print("\nThese tests verify that tools are blocked when no CS_ACCESS_TOKEN")
    print("is configured, while configure tools remain accessible.")

    return run_require_access_token_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

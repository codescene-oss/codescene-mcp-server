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
from dataclasses import dataclass
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


@dataclass
class TokenGuardTestCase:
    """Describes a single token-guard test scenario."""

    name: str
    tool_name: str
    args: dict
    expect_blocked: bool
    remove_token: bool
    timeout: int = 30


# --- Helpers ---


def _make_no_token_env(base_env: dict) -> dict:
    """Return env dict with CS_ACCESS_TOKEN explicitly removed."""
    env = base_env.copy()
    env.pop("CS_ACCESS_TOKEN", None)
    return env


def _run_single_test(
    command: list[str], env: dict, cwd: str, case: TokenGuardTestCase,
) -> bool:
    """Run one token-guard test case and return pass/fail."""
    print_header(f"Test: {case.name}")

    effective_env = _make_no_token_env(env) if case.remove_token else env
    client = MCPClient(command, env=effective_env, cwd=cwd)

    if not client.start():
        print_test("Server started", False, client.get_stderr())
        return False
    print_test("Server started", True)

    try:
        client.initialize()
        print(f"\n  Calling {case.tool_name}...")

        response = client.call_tool(case.tool_name, case.args, timeout=case.timeout)
        result_text = extract_result_text(response)

        if case.expect_blocked:
            return _verify_blocked(result_text)
        return _verify_not_blocked(result_text, case.tool_name)

    except Exception as e:
        print_test(case.name, False, str(e))
        return False
    finally:
        client.stop()


def _verify_blocked(result_text: str) -> bool:
    """Assert the response is the token-missing guard message."""
    has_marker = _TOKEN_MISSING_MARKER in result_text
    print_test(
        "Response contains token-missing message",
        has_marker,
        f"Response preview: {result_text[:200]}",
    )
    has_hint = "set_config" in result_text
    print_test("Response mentions set_config", has_hint)
    return has_marker and has_hint


def _verify_not_blocked(result_text: str, tool_name: str) -> bool:
    """Assert the response is NOT blocked and contains real content."""
    not_blocked = _TOKEN_MISSING_MARKER not in result_text
    print_test(
        f"{tool_name} NOT blocked by token guard",
        not_blocked,
        f"Response preview: {result_text[:200]}",
    )
    has_content = len(result_text) > 0
    print_test(f"{tool_name} returned content", has_content)
    return not_blocked and has_content


def _build_test_cases(repo_dir: Path) -> list[TokenGuardTestCase]:
    """Build the full list of test cases for the token guard."""
    test_file = str(repo_dir / "hello.py")
    return [
        TokenGuardTestCase(
            name="Guarded Tool Blocked Without Token",
            tool_name="code_health_score",
            args={"file_path": test_file},
            expect_blocked=True,
            remove_token=True,
        ),
        TokenGuardTestCase(
            name="Explain Tool Blocked Without Token",
            tool_name="explain_code_health",
            args={},
            expect_blocked=True,
            remove_token=True,
        ),
        TokenGuardTestCase(
            name="get_config Works Without Token",
            tool_name="get_config",
            args={},
            expect_blocked=False,
            remove_token=True,
        ),
        TokenGuardTestCase(
            name="set_config Works Without Token",
            tool_name="set_config",
            args={"key": "onprem_url", "value": "https://test.example.com"},
            expect_blocked=False,
            remove_token=True,
        ),
        TokenGuardTestCase(
            name="Guarded Tool Works With Token",
            tool_name="code_health_score",
            args={"file_path": test_file},
            expect_blocked=False,
            remove_token=False,
            timeout=60,
        ),
    ]


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

        sample_files = {"hello.py": "def hello():\n    return 'world'\n"}
        repo_dir = create_git_repo(test_dir, sample_files)
        print(f"Repository: {repo_dir}")

        config_dir = test_dir / "config"
        config_dir.mkdir()

        command = backend.get_command(repo_dir)
        base_env = backend.get_env(os.environ.copy(), repo_dir)
        base_env["CS_CONFIG_DIR"] = str(config_dir)
        cwd = str(repo_dir)

        cases = _build_test_cases(repo_dir)
        results: list[tuple[str, bool | str]] = [
            (case.name, _run_single_test(command, base_env, cwd, case))
            for case in cases
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

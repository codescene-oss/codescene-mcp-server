#!/usr/bin/env python3
"""
Standalone license integration tests.

Tests that the MCP server correctly gates API-based tools when the
CS_ACCESS_TOKEN is a standalone Ed25519-signed JWT, and exposes all
tools when it is a regular PAT.

This test suite validates:
1. With a standalone JWT, API tools (select_project, technical_debt_*,
   code_ownership) are NOT listed in tools/list.
2. CLI tools (code_health_score, code_health_review, etc.) and explain
   tools are always listed regardless of token type.
3. With a regular PAT, all tools (API + CLI) are listed.
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
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)

# A valid JWT signed with the production private key.  Only signature
# verification matters (expiry is not enforced by is_standalone_license).
_VALID_STANDALONE_JWT = (
    "eyJhbGciOiJFZERTQSIsImtpZCI6ImNzbWNwIiwidHlwIjoiSldTIn0."
    "eyJpc3MiOiJjb2Rlc2NlbmUtbWNwIiwiYXVkIjoiY29kZXNjZW5lLWNsaSIs"
    "ImlhdCI6MTc3MTk0NTM1NSwiZXhwIjoxNzcyMjgxNjUzLCJzdWIiOiIyYTM5"
    "NDAyNS1kYjg2LTQwMDAtYWE0NS1lODY2Yjk5YmJhMzcifQ."
    "V0UxjlS1ZK-hcF1M7edu6GfvMAjv1XukFe8m6iHzS9guh_4rqu4HGbRTzl21"
    "7qMemCjwyHtAG9pO6NUu3SWbCQ"
)

# --- Tool name constants ---

API_TOOLS = frozenset({
    "select_project",
    "list_technical_debt_goals_for_project",
    "list_technical_debt_goals_for_project_file",
    "list_technical_debt_hotspots_for_project",
    "list_technical_debt_hotspots_for_project_file",
    "code_ownership_for_path",
})

CLI_TOOLS = frozenset({
    "pre_commit_code_health_safeguard",
    "analyze_change_set",
    "code_health_refactoring_business_case",
    "code_health_score",
    "code_health_review",
    "explain_code_health",
    "explain_code_health_productivity",
    "code_health_auto_refactor",
})

# --- Test helpers ---


def _make_standalone_env(base_env: dict) -> dict:
    """Return env dict configured for standalone JWT mode."""
    env = base_env.copy()
    env["CS_ACCESS_TOKEN"] = _VALID_STANDALONE_JWT
    return env


def _make_pat_env(base_env: dict) -> dict:
    """Return env dict configured with a regular PAT."""
    env = base_env.copy()
    env["CS_ACCESS_TOKEN"] = "cst_fake_pat_for_integration_test"
    return env


ServerParams = tuple[list[str], str]  # (command, cwd)


def _get_tool_names(server: ServerParams, env: dict) -> set[str] | None:
    """Start server, initialize, list tools, and return their names.

    Returns None if the server fails to start.
    """
    command, cwd = server
    client = MCPClient(command, env=env, cwd=cwd)
    try:
        if not client.start():
            print_test("Server started", False, client.get_stderr())
            return None
        print_test("Server started", True)

        client.initialize()
        response = client.send_request("tools/list", timeout=15)
        tools = response.get("result", {}).get("tools", [])
        return {t["name"] for t in tools}
    finally:
        client.stop()


def _check_tools_membership(tool_names: set[str], expected: frozenset, should_be_present: bool) -> bool:
    """Verify each expected tool is present/absent and print results."""
    all_ok = True
    for tool in sorted(expected):
        found = tool in tool_names
        ok = found == should_be_present
        label = f"  {tool} {'present' if should_be_present else 'hidden'}"
        print_test(label, ok)
        if not ok:
            all_ok = False
    return all_ok


# --- Test functions ---


def _run_tool_visibility_test(
    server: ServerParams,
    env: dict,
    tools_to_check: frozenset,
    should_be_present: bool,
) -> bool:
    """Start server with given env then verify tool visibility."""
    try:
        tool_names = _get_tool_names(server, env)
        if tool_names is None:
            return False

        print(f"\n  Tools listed: {len(tool_names)}")
        return _check_tools_membership(tool_names, tools_to_check, should_be_present)

    except Exception as e:
        print_test("Tool visibility check", False, str(e))
        return False


def test_standalone_hides_api_tools(server: ServerParams, env: dict) -> bool:
    """With a standalone JWT, API tools must not appear in tools/list."""
    print_header("Test: Standalone JWT Hides API Tools")
    return _run_tool_visibility_test(
        server, _make_standalone_env(env),
        API_TOOLS, should_be_present=False,
    )


def test_standalone_keeps_cli_tools(server: ServerParams, env: dict) -> bool:
    """With a standalone JWT, CLI tools must still appear in tools/list."""
    print_header("Test: Standalone JWT Keeps CLI Tools")
    return _run_tool_visibility_test(
        server, _make_standalone_env(env),
        CLI_TOOLS, should_be_present=True,
    )


def test_pat_exposes_all_tools(server: ServerParams, env: dict) -> bool:
    """With a regular PAT, all tools (API + CLI) must appear in tools/list."""
    print_header("Test: PAT Exposes All Tools")
    return _run_tool_visibility_test(
        server, _make_pat_env(env),
        API_TOOLS | CLI_TOOLS, should_be_present=True,
    )


# --- Runner ---


def run_standalone_license_tests(executable: Path) -> int:
    backend = NuitkaBackend(executable=executable)
    return run_standalone_license_tests_with_backend(backend)


def run_standalone_license_tests_with_backend(backend: ServerBackend) -> int:
    """Run all standalone license tests using a backend.

    Returns exit code (0 for success, 1 for failure).
    """
    with safe_temp_directory(prefix="cs_mcp_standalone_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        # Create a minimal git repo (needed for server startup)
        sample_files = {"hello.py": "def hello():\n    return 'world'\n"}
        repo_dir = create_git_repo(test_dir, sample_files)
        print(f"Repository: {repo_dir}")

        command = backend.get_command(repo_dir)
        env = backend.get_env(os.environ.copy(), repo_dir)
        server: ServerParams = (command, str(repo_dir))

        results: list[tuple[str, bool | str]] = [
            ("Standalone JWT hides API tools", test_standalone_hides_api_tools(server, env)),
            ("Standalone JWT keeps CLI tools", test_standalone_keeps_cli_tools(server, env)),
            ("PAT exposes all tools", test_pat_exposes_all_tools(server, env)),
        ]

        return print_summary(results)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_standalone_license.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Standalone License Integration Tests")
    print("\nThese tests verify that API tools are hidden for standalone JWT")
    print("licenses and visible for regular PAT tokens.")

    return run_standalone_license_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

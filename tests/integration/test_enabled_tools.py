#!/usr/bin/env python3
"""
Enabled tools integration tests.

Tests that the MCP server correctly filters tools based on the
``enabled_tools`` configuration option.

This test suite validates:
1. Without ``enabled_tools`` set, all tools are exposed.
2. When ``enabled_tools`` is set via an environment variable, only those
   tools (plus get_config/set_config) appear in tools/list.
3. ``get_config`` and ``set_config`` are always present, even if not listed.
4. Setting ``enabled_tools`` via ``set_config`` returns a restart warning.
5. Setting ``enabled_tools`` with invalid tool names returns a warning.
6. Querying ``enabled_tools`` via ``get_config`` includes ``available_tools``.
"""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from test_utils import (
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


# --- Helpers ---


def _tool_names(client: MCPClient) -> set[str]:
    """Return the set of tool names from tools/list."""
    response = client.send_request("tools/list", timeout=15)
    tools = response.get("result", {}).get("tools", [])
    return {t["name"] for t in tools}


def _make_config_dir(test_dir: Path) -> Path:
    config_dir = test_dir / "config"
    config_dir.mkdir()
    return config_dir


def _start_client(command: list[str], env: dict, cwd: str) -> MCPClient | None:
    client = MCPClient(command, env=env, cwd=cwd)
    if not client.start():
        print_test("Server started", False, client.get_stderr())
        return None
    print_test("Server started", True)
    client.initialize()
    return client


# --- Test functions ---


def test_all_tools_without_filter(command: list[str], env: dict, cwd: str) -> bool:
    """Without enabled_tools set, all tools are available."""
    print_header("Test: All Tools Without Filter")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        names = _tool_names(client)

        has_get_config = "get_config" in names
        has_set_config = "set_config" in names
        has_review = "code_health_review" in names
        has_score = "code_health_score" in names
        has_explain = "explain_code_health" in names

        print_test("get_config listed", has_get_config)
        print_test("set_config listed", has_set_config)
        print_test("code_health_review listed", has_review)
        print_test("code_health_score listed", has_score)
        print_test("explain_code_health listed", has_explain)

        # Expect at least 10 tools when unfiltered (standalone has 10, API mode has 16)
        enough_tools = len(names) >= 10
        print_test(
            "At least 10 tools available",
            enough_tools,
            f"Found {len(names)} tools",
        )

        return all([has_get_config, has_set_config, has_review, has_score, has_explain, enough_tools])

    except Exception as e:
        print_test("All tools without filter", False, str(e))
        return False
    finally:
        client.stop()


def test_filter_restricts_tools(
    command: list[str], base_env: dict, cwd: str, config_dir: Path,
) -> bool:
    """Setting CS_ENABLED_TOOLS restricts tools/list to the allowlist."""
    print_header("Test: Filter Restricts Tools")

    env = base_env.copy()
    env["CS_ENABLED_TOOLS"] = "code_health_review,code_health_score"

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        names = _tool_names(client)

        has_review = "code_health_review" in names
        has_score = "code_health_score" in names
        has_get_config = "get_config" in names
        has_set_config = "set_config" in names

        # Should NOT have tools outside the allowlist
        no_explain = "explain_code_health" not in names
        no_refactor = "code_health_auto_refactor" not in names

        print_test("code_health_review listed", has_review)
        print_test("code_health_score listed", has_score)
        print_test("get_config always listed", has_get_config)
        print_test("set_config always listed", has_set_config)
        print_test("explain_code_health NOT listed", no_explain)
        print_test("code_health_auto_refactor NOT listed", no_refactor)

        expected_count = 4  # 2 enabled + get_config + set_config
        correct_count = len(names) == expected_count
        print_test(
            f"Exactly {expected_count} tools",
            correct_count,
            f"Found {len(names)}: {sorted(names)}",
        )

        return all([
            has_review, has_score, has_get_config, has_set_config,
            no_explain, no_refactor, correct_count,
        ])

    except Exception as e:
        print_test("Filter restricts tools", False, str(e))
        return False
    finally:
        client.stop()


def test_config_tools_always_present(
    command: list[str], base_env: dict, cwd: str, config_dir: Path,
) -> bool:
    """get_config and set_config cannot be disabled."""
    print_header("Test: Config Tools Always Present")

    env = base_env.copy()
    # Enable only a single non-config tool
    env["CS_ENABLED_TOOLS"] = "explain_code_health"

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        names = _tool_names(client)

        has_get_config = "get_config" in names
        has_set_config = "set_config" in names
        has_explain = "explain_code_health" in names

        print_test("get_config always present", has_get_config)
        print_test("set_config always present", has_set_config)
        print_test("explain_code_health present", has_explain)

        return has_get_config and has_set_config and has_explain

    except Exception as e:
        print_test("Config tools always present", False, str(e))
        return False
    finally:
        client.stop()


def test_set_enabled_tools_restart_warning(command: list[str], env: dict, cwd: str) -> bool:
    """Setting enabled_tools via set_config includes a restart warning."""
    print_header("Test: Set Enabled Tools Restart Warning")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        resp = client.call_tool(
            "set_config",
            {"key": "enabled_tools", "value": "code_health_review,code_health_score"},
            timeout=30,
        )
        text = extract_result_text(resp)

        has_restart = "restart" in text.lower()
        has_saved = "saved" in text.lower()
        print_test("Response says saved", has_saved, text[:200])
        print_test("Restart warning present", has_restart)

        return has_saved and has_restart

    except Exception as e:
        print_test("Set enabled_tools restart warning", False, str(e))
        return False
    finally:
        client.stop()


def test_set_invalid_tool_name_warning(command: list[str], env: dict, cwd: str) -> bool:
    """Setting enabled_tools with unknown tool names includes a warning."""
    print_header("Test: Invalid Tool Name Warning")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        resp = client.call_tool(
            "set_config",
            {"key": "enabled_tools", "value": "code_health_review,nonexistent_tool"},
            timeout=30,
        )
        text = extract_result_text(resp)

        has_warning = "nonexistent_tool" in text
        has_unrecognized = "unrecognized" in text.lower()
        print_test("Warning mentions unknown tool name", has_warning, text[:200])
        print_test("Warning says unrecognized", has_unrecognized)

        return has_warning and has_unrecognized

    except Exception as e:
        print_test("Invalid tool name warning", False, str(e))
        return False
    finally:
        client.stop()


def test_get_enabled_tools_shows_available(command: list[str], env: dict, cwd: str) -> bool:
    """Querying enabled_tools via get_config includes available_tools."""
    print_header("Test: Get Enabled Tools Shows Available")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        resp = client.call_tool(
            "get_config",
            {"key": "enabled_tools"},
            timeout=30,
        )
        text = extract_result_text(resp)

        has_available = "available_tools" in text
        has_review_in_list = "code_health_review" in text
        # get_config and set_config should NOT appear in available_tools
        # (they are always-on, not configurable)
        no_get_config = text.count("get_config") <= 1  # Only the response key itself

        print_test("Response contains available_tools", has_available, text[:200])
        print_test("Available tools includes code_health_review", has_review_in_list)

        return has_available and has_review_in_list

    except Exception as e:
        print_test("Get enabled_tools shows available", False, str(e))
        return False
    finally:
        client.stop()


# --- Runner ---


def run_enabled_tools_tests_with_backend(backend: ServerBackend) -> int:
    """Run all enabled_tools tests using a backend.

    Returns exit code (0 for success, 1 for failure).
    """
    with safe_temp_directory(prefix="cs_mcp_enabled_tools_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        config_dir = _make_config_dir(test_dir)

        sample_files = {"hello.py": "def hello():\n    return 'world'\n"}
        repo_dir = create_git_repo(test_dir, sample_files)
        print(f"Repository: {repo_dir}")

        command = backend.get_command(repo_dir)
        base_env = backend.get_env(os.environ.copy(), repo_dir)
        env = base_env.copy()
        env["CS_CONFIG_DIR"] = str(config_dir)
        cwd = str(repo_dir)

        results: list[tuple[str, bool]] = [
            ("All tools without filter", test_all_tools_without_filter(command, env, cwd)),
            ("Filter restricts tools", test_filter_restricts_tools(command, env, cwd, config_dir)),
            ("Config tools always present", test_config_tools_always_present(command, env, cwd, config_dir)),
            ("Set enabled_tools restart warning", test_set_enabled_tools_restart_warning(command, env, cwd)),
            ("Invalid tool name warning", test_set_invalid_tool_name_warning(command, env, cwd)),
            ("Get enabled_tools shows available", test_get_enabled_tools_shows_available(command, env, cwd)),
        ]

        return print_summary(results)


def run_enabled_tools_tests(executable: Path) -> int:
    """Run all enabled_tools tests with a Cargo executable.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = CargoBackend(executable=executable)
    return run_enabled_tools_tests_with_backend(backend)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_enabled_tools.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Enabled Tools Integration Tests")
    print("\nThese tests verify the enabled_tools configuration option")
    print("for restricting which MCP tools are exposed.")

    return run_enabled_tools_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

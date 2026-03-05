#!/usr/bin/env python3
"""
Configure tool integration tests.

Tests that the MCP server correctly exposes ``get_config`` and ``set_config``
tools for managing persistent configuration through conversation.

This test suite validates:
1. Both tools appear in the tools/list response.
2. Setting a value persists and can be read back.
3. Sensitive values are masked in ``get_config`` output.
4. Listing all options returns visible keys and hides internal ones.
5. Using an invalid key returns a helpful error.
6. Deleting a value (empty string) removes it from the config file.
7. Environment variables take precedence over config-file values.
8. Hidden options (disable_tracking, disable_version_check) never appear
   in the listing but are still accessible by explicit key.
9. API-only options (onprem_url, default_project_id) are hidden from
   the listing when running with a standalone license.
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

# A valid JWT signed with the production private key (from test_standalone_license).
_VALID_STANDALONE_JWT = (
    "eyJhbGciOiJFZERTQSIsImtpZCI6ImNzbWNwIiwidHlwIjoiSldTIn0."
    "eyJpc3MiOiJjb2Rlc2NlbmUtbWNwIiwiYXVkIjoiY29kZXNjZW5lLWNsaSIs"
    "ImlhdCI6MTc3MTk0NTM1NSwiZXhwIjoxNzcyMjgxNjUzLCJzdWIiOiIyYTM5"
    "NDAyNS1kYjg2LTQwMDAtYWE0NS1lODY2Yjk5YmJhMzcifQ."
    "V0UxjlS1ZK-hcF1M7edu6GfvMAjv1XukFe8m6iHzS9guh_4rqu4HGbRTzl21"
    "7qMemCjwyHtAG9pO6NUu3SWbCQ"
)


# --- Helpers ---


def _make_env_with_config_dir(base_env: dict, config_dir: Path) -> dict:
    """Return env dict with CS_CONFIG_DIR pointed at *config_dir*."""
    env = base_env.copy()
    env["CS_CONFIG_DIR"] = str(config_dir)
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


def _set_then_read(
    client: MCPClient, key: str, value: str, expected_in_get: str,
) -> tuple[bool, bool]:
    """Set a config key and read it back, returning (set_ok, get_ok)."""
    set_resp = client.call_tool(
        "set_config", {"key": key, "value": value}, timeout=30,
    )
    set_text = extract_result_text(set_resp)
    set_ok = "saved" in set_text.lower() or key in set_text.lower()

    get_resp = client.call_tool(
        "get_config", {"key": key}, timeout=30,
    )
    get_text = extract_result_text(get_resp)
    get_ok = expected_in_get in get_text

    return set_ok, get_ok


# --- Test functions ---


def test_tools_visible(command: list[str], env: dict, cwd: str) -> bool:
    """Verify that get_config and set_config appear in tools/list."""
    print_header("Test: Configure Tools Visible")

    client = MCPClient(command, env=env, cwd=cwd)
    try:
        if not client.start():
            print_test("Server started", False, client.get_stderr())
            return False
        print_test("Server started", True)
        client.initialize()

        response = client.send_request("tools/list", timeout=15)
        tools = response.get("result", {}).get("tools", [])
        tool_names = {t["name"] for t in tools}

        has_get = "get_config" in tool_names
        has_set = "set_config" in tool_names
        print_test("get_config listed", has_get)
        print_test("set_config listed", has_set)

        return has_get and has_set

    except Exception as e:
        print_test("Tools visible", False, str(e))
        return False
    finally:
        client.stop()


def test_set_then_get(command: list[str], env: dict, cwd: str) -> bool:
    """Set a config value and immediately read it back."""
    print_header("Test: Set Then Get")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        saved_ok, value_present = _set_then_read(
            client, "onprem_url", "https://test.example.com", "https://test.example.com",
        )
        print_test("set_config acknowledged", saved_ok)
        print_test("get_config returns stored value", value_present)
        return saved_ok and value_present

    except Exception as e:
        print_test("Set then get", False, str(e))
        return False
    finally:
        client.stop()


def test_sensitive_masking(command: list[str], env: dict, cwd: str) -> bool:
    """Sensitive values (access_token) must be masked in get_config output."""
    print_header("Test: Sensitive Value Masking")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        secret = "super-secret-token-value-12345"
        client.call_tool(
            "set_config",
            {"key": "access_token", "value": secret},
            timeout=30,
        )

        get_resp = client.call_tool(
            "get_config",
            {"key": "access_token"},
            timeout=30,
        )
        get_text = extract_result_text(get_resp)

        full_hidden = secret not in get_text
        has_mask = "..." in get_text
        print_test("Full token NOT in output", full_hidden)
        print_test("Masked prefix present", has_mask, get_text[:120])

        return full_hidden and has_mask

    except Exception as e:
        print_test("Sensitive masking", False, str(e))
        return False
    finally:
        client.stop()


def test_list_all(command: list[str], env: dict, cwd: str) -> bool:
    """Listing shows visible keys and hides hidden/internal ones.

    The default test env uses a PAT, so api_only options should be visible.
    Hidden options (disable_tracking, disable_version_check) must be absent.
    """
    print_header("Test: List All Options")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        resp = client.call_tool("get_config", {}, timeout=30)
        text = extract_result_text(resp)

        visible_keys = [
            "access_token",
            "onprem_url",
            "ace_access_token",
            "default_project_id",
            "ca_bundle",
        ]
        hidden_keys = ["disable_tracking", "disable_version_check"]

        found_visible = [k for k in visible_keys if k in text]
        all_visible = len(found_visible) == len(visible_keys)
        print_test(
            "Visible config keys listed",
            all_visible,
            f"Found {len(found_visible)}/{len(visible_keys)}",
        )

        found_hidden = [k for k in hidden_keys if k in text]
        none_hidden = len(found_hidden) == 0
        print_test(
            "Hidden keys absent from listing",
            none_hidden,
            f"Unexpectedly found: {found_hidden}" if found_hidden else "OK",
        )

        return all_visible and none_hidden

    except Exception as e:
        print_test("List all options", False, str(e))
        return False
    finally:
        client.stop()


def test_invalid_key(command: list[str], env: dict, cwd: str) -> bool:
    """Using an invalid key returns a helpful error message."""
    print_header("Test: Invalid Key Error")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        resp = client.call_tool(
            "set_config",
            {"key": "no_such_key", "value": "anything"},
            timeout=30,
        )
        text = extract_result_text(resp)

        has_error = "unknown" in text.lower()
        has_valid_keys = "valid keys" in text.lower() or "access_token" in text
        print_test("Error mentions unknown key", has_error, text[:120])
        print_test("Error lists valid keys", has_valid_keys)

        return has_error and has_valid_keys

    except Exception as e:
        print_test("Invalid key error", False, str(e))
        return False
    finally:
        client.stop()


def test_delete_value(command: list[str], env: dict, cwd: str) -> bool:
    """Passing an empty string removes the key from the config file."""
    print_header("Test: Delete Value")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        # Set a value first
        client.call_tool(
            "set_config",
            {"key": "default_project_id", "value": "42"},
            timeout=30,
        )

        # Delete it
        del_resp = client.call_tool(
            "set_config",
            {"key": "default_project_id", "value": ""},
            timeout=30,
        )
        del_text = extract_result_text(del_resp)
        removed_ok = "removed" in del_text.lower() or "default_project_id" in del_text.lower()
        print_test("Delete acknowledged", removed_ok, del_text[:120])

        # Read it back — should be not set
        get_resp = client.call_tool(
            "get_config",
            {"key": "default_project_id"},
            timeout=30,
        )
        get_text = extract_result_text(get_resp)
        is_unset = "not set" in get_text.lower()
        print_test("Value no longer set", is_unset, get_text[:120])

        return removed_ok and is_unset

    except Exception as e:
        print_test("Delete value", False, str(e))
        return False
    finally:
        client.stop()


def test_env_override(command: list[str], base_env: dict, cwd: str) -> bool:
    """Environment variable takes precedence over config-file value."""
    print_header("Test: Environment Override")

    # Pre-set an env var that shadows the config file value
    env = base_env.copy()
    env["CS_ONPREM_URL"] = "https://env-override.example.com"

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        # Write a different value to the config file
        client.call_tool(
            "set_config",
            {"key": "onprem_url", "value": "https://file-value.example.com"},
            timeout=30,
        )

        # Read it back — env should win
        get_resp = client.call_tool(
            "get_config",
            {"key": "onprem_url"},
            timeout=30,
        )
        get_text = extract_result_text(get_resp)

        env_wins = "https://env-override.example.com" in get_text
        source_env = "environment" in get_text.lower()
        print_test("Env value shown", env_wins, get_text[:120])
        print_test("Source is environment", source_env)

        return env_wins and source_env

    except Exception as e:
        print_test("Env override", False, str(e))
        return False
    finally:
        client.stop()


def test_hidden_option_accessible_by_key(command: list[str], env: dict, cwd: str) -> bool:
    """Hidden options can still be get/set when addressed by explicit key."""
    print_header("Test: Hidden Option Accessible By Key")

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        saved_ok, value_present = _set_then_read(
            client, "disable_tracking", "true", "true",
        )
        print_test("set_config on hidden key acknowledged", saved_ok)
        print_test("get_config returns hidden key value", value_present)
        return saved_ok and value_present

    except Exception as e:
        print_test("Hidden option accessible by key", False, str(e))
        return False
    finally:
        client.stop()


def test_standalone_hides_api_only(command: list[str], base_env: dict, cwd: str) -> bool:
    """With a standalone JWT, api_only options are hidden from the listing."""
    print_header("Test: Standalone Hides API-Only Options")

    env = base_env.copy()
    env["CS_ACCESS_TOKEN"] = _VALID_STANDALONE_JWT

    client = _start_client(command, env, cwd)
    if client is None:
        return False

    try:
        resp = client.call_tool("get_config", {}, timeout=30)
        text = extract_result_text(resp)

        api_only_keys = ["onprem_url", "default_project_id"]
        found = [k for k in api_only_keys if k in text]
        none_found = len(found) == 0
        print_test(
            "API-only keys hidden for standalone",
            none_found,
            f"Unexpectedly found: {found}" if found else "OK",
        )

        # Verify non-api-only keys are still present
        always_visible = ["access_token", "ca_bundle"]
        found_visible = [k for k in always_visible if k in text]
        all_visible = len(found_visible) == len(always_visible)
        print_test(
            "Non-API keys still visible",
            all_visible,
            f"Found {len(found_visible)}/{len(always_visible)}",
        )

        return none_found and all_visible

    except Exception as e:
        print_test("Standalone hides api-only", False, str(e))
        return False
    finally:
        client.stop()


# --- Runner ---


def run_configure_tests_with_backend(backend: ServerBackend) -> int:
    """Run all configure tests using a backend.

    Returns exit code (0 for success, 1 for failure).
    """
    with safe_temp_directory(prefix="cs_mcp_configure_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        config_dir = test_dir / "config"
        config_dir.mkdir()

        # Minimal git repo needed for server startup
        sample_files = {"hello.py": "def hello():\n    return 'world'\n"}
        repo_dir = create_git_repo(test_dir, sample_files)
        print(f"Repository: {repo_dir}")

        command = backend.get_command(repo_dir)
        base_env = backend.get_env(os.environ.copy(), repo_dir)
        env = _make_env_with_config_dir(base_env, config_dir)
        cwd = str(repo_dir)

        results: list[tuple[str, bool]] = [
            ("Configure tools visible", test_tools_visible(command, env, cwd)),
            ("Set then get", test_set_then_get(command, env, cwd)),
            ("Sensitive masking", test_sensitive_masking(command, env, cwd)),
            ("List all options", test_list_all(command, env, cwd)),
            ("Invalid key error", test_invalid_key(command, env, cwd)),
            ("Delete value", test_delete_value(command, env, cwd)),
            ("Env override", test_env_override(command, env, cwd)),
            ("Hidden option by key", test_hidden_option_accessible_by_key(command, env, cwd)),
            ("Standalone hides api-only", test_standalone_hides_api_only(command, env, cwd)),
        ]

        return print_summary(results)


def run_configure_tests(executable: Path) -> int:
    """Run all configure tests with a Nuitka executable.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = NuitkaBackend(executable=executable)
    return run_configure_tests_with_backend(backend)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_configure.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Configure Integration Tests")
    print("\nThese tests verify the get_config and set_config tools")
    print("for managing persistent CodeScene MCP Server configuration.")

    return run_configure_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

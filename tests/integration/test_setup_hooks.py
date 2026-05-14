#!/usr/bin/env python3
"""
Setup Hooks integration tests.

Tests that the setup_hooks MCP tool correctly writes agent hook
configuration files into a project directory.

This test suite validates:
1. The tool creates .claude/settings.json with correct hook config
2. The tool merges with existing settings without overwriting
3. The tool handles unsupported agents with informative messages
4. Duplicate invocations do not create duplicate hooks
5. Custom server names are applied correctly
"""

import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from test_utils import (
    MCPClient,
    CargoBackend,
    ServerBackend,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)


def run_setup_hooks_tests(executable: Path) -> int:
    """Run all setup_hooks tests with a compiled executable."""
    backend = CargoBackend(executable=executable)
    return run_setup_hooks_tests_with_backend(backend)


def run_setup_hooks_tests_with_backend(backend: ServerBackend) -> int:
    """Run all setup_hooks tests using a backend."""
    with safe_temp_directory(prefix="cs_mcp_setup_hooks_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        project_dir = test_dir / "project"
        project_dir.mkdir()

        command = backend.get_command(project_dir)
        env = backend.get_env(os.environ.copy(), project_dir)

        results = [
            ("Setup Hooks - Creates Claude Settings", test_creates_claude_settings(command, env, project_dir)),
            ("Setup Hooks - Creates OpenCode Plugin", test_creates_opencode_plugin(command, env, test_dir)),
            ("Setup Hooks - Merges Existing Settings", test_merges_existing_settings(command, env, test_dir)),
            ("Setup Hooks - Skips Duplicates", test_skips_duplicates(command, env, test_dir)),
            ("Setup Hooks - Custom Server Name", test_custom_server_name(command, env, test_dir)),
            ("Setup Hooks - Unsupported Agent", test_unsupported_agent(command, env, project_dir)),
            ("Setup Hooks - Unknown Agent", test_unknown_agent(command, env, project_dir)),
        ]

        return print_summary(results)


def _call_setup_hooks(command: list[str], env: dict, project_dir: Path, params: dict) -> str | None:
    """Start server, call setup_hooks, return result text or None on failure."""
    client = MCPClient(command, env=env, cwd=str(project_dir))
    try:
        if not client.start():
            print_test("Server started", False)
            return None
        print_test("Server started", True)
        client.initialize()
        response = client.call_tool("setup_hooks", params, timeout=30)
        return extract_result_text(response)
    except Exception as e:
        print_test("setup_hooks call", False, str(e))
        return None
    finally:
        client.stop()


def _validate_claude_hooks(settings_path: Path) -> bool:
    """Validate that settings.json contains expected hook structure."""
    file_exists = settings_path.exists()
    print_test("Settings file created", file_exists)
    if not file_exists:
        return False

    content = json.loads(settings_path.read_text())
    has_post = "PostToolUse" in content.get("hooks", {})
    has_pre = "PreToolUse" in content.get("hooks", {})
    print_test("PostToolUse hooks present", has_post)
    print_test("PreToolUse hooks present", has_pre)

    post_hooks = content["hooks"]["PostToolUse"]
    has_review = any(hook.get("matcher") == "Write|Edit" for hook in post_hooks)
    print_test("Write|Edit matcher configured", has_review)

    return has_post and has_pre and has_review


def test_creates_claude_settings(command: list[str], env: dict, project_dir: Path) -> bool:
    """Test that setup_hooks creates .claude/settings.json with correct hooks."""
    print_header("Test: Creates Claude Settings")

    result_text = _call_setup_hooks(command, env, project_dir, {"project_dir": str(project_dir)})
    if result_text is None:
        return False

    has_success = "successfully installed" in result_text.lower()
    print_test("Success message returned", has_success, result_text[:200])

    hooks_valid = _validate_claude_hooks(project_dir / ".claude" / "settings.json")
    return has_success and hooks_valid


def test_creates_opencode_plugin(command: list[str], env: dict, test_dir: Path) -> bool:
    """Test that setup_hooks creates .opencode/plugins/codescene.ts with correct content."""
    print_header("Test: Creates OpenCode Plugin")

    oc_dir = test_dir / "opencode_project"
    oc_dir.mkdir(exist_ok=True)

    result_text = _call_setup_hooks(
        command, env, oc_dir,
        {"project_dir": str(oc_dir), "agent": "opencode"},
    )
    if result_text is None:
        return False

    has_success = "successfully installed" in result_text.lower()
    print_test("Success message returned", has_success, result_text[:200])

    plugin_path = oc_dir / ".opencode" / "plugins" / "codescene.ts"
    file_exists = plugin_path.exists()
    print_test("Plugin file created", file_exists)

    if not file_exists:
        return False

    content = plugin_path.read_text()
    has_review = "code_health_review" in content
    has_safeguard = "pre_commit_code_health_safeguard" in content
    has_server = '"codescene"' in content
    print_test("Contains code_health_review", has_review)
    print_test("Contains pre_commit_code_health_safeguard", has_safeguard)
    print_test("Contains server name", has_server)

    return has_success and file_exists and has_review and has_safeguard and has_server


def test_merges_existing_settings(command: list[str], env: dict, test_dir: Path) -> bool:
    """Test that setup_hooks merges with existing settings."""
    print_header("Test: Merges Existing Settings")

    merge_dir = test_dir / "merge_project"
    merge_dir.mkdir(exist_ok=True)
    claude_dir = merge_dir / ".claude"
    claude_dir.mkdir(exist_ok=True)

    existing = {
        "permissions": {"allow": ["Read"]},
        "hooks": {
            "PostToolUse": [
                {"matcher": "CustomLint", "hooks": []}
            ]
        }
    }
    (claude_dir / "settings.json").write_text(json.dumps(existing))

    result_text = _call_setup_hooks(command, env, merge_dir, {"project_dir": str(merge_dir)})
    if result_text is None:
        return False

    has_success = "successfully installed" in result_text.lower()
    print_test("Success message returned", has_success)

    content = json.loads((claude_dir / "settings.json").read_text())

    permissions_kept = content.get("permissions", {}).get("allow") == ["Read"]
    print_test("Existing permissions preserved", permissions_kept)

    post_hooks = content.get("hooks", {}).get("PostToolUse", [])
    has_original = any(h.get("matcher") == "CustomLint" for h in post_hooks)
    has_ours = any(h.get("matcher") == "Write|Edit" for h in post_hooks)
    print_test("Original hook preserved", has_original)
    print_test("Our hook appended", has_ours)
    print_test("Total PostToolUse groups", len(post_hooks) == 2, f"count={len(post_hooks)}")

    return has_success and permissions_kept and has_original and has_ours


def test_skips_duplicates(command: list[str], env: dict, test_dir: Path) -> bool:
    """Test that running setup_hooks twice does not duplicate hooks."""
    print_header("Test: Skips Duplicates")

    dup_dir = test_dir / "dup_project"
    dup_dir.mkdir(exist_ok=True)

    client = MCPClient(command, env=env, cwd=str(dup_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        client.call_tool("setup_hooks", {"project_dir": str(dup_dir)}, timeout=30)
        client.call_tool("setup_hooks", {"project_dir": str(dup_dir)}, timeout=30)

        content = json.loads((dup_dir / ".claude" / "settings.json").read_text())
        post_count = len(content.get("hooks", {}).get("PostToolUse", []))
        pre_count = len(content.get("hooks", {}).get("PreToolUse", []))

        no_dup_post = post_count == 1
        no_dup_pre = pre_count == 1
        print_test("No duplicate PostToolUse", no_dup_post, f"count={post_count}")
        print_test("No duplicate PreToolUse", no_dup_pre, f"count={pre_count}")

        return no_dup_post and no_dup_pre

    except Exception as e:
        print_test("Skips duplicates", False, str(e))
        return False
    finally:
        client.stop()


def test_custom_server_name(command: list[str], env: dict, test_dir: Path) -> bool:
    """Test that a custom server name is used in the generated hooks."""
    print_header("Test: Custom Server Name")

    custom_dir = test_dir / "custom_server_project"
    custom_dir.mkdir(exist_ok=True)

    result_text = _call_setup_hooks(
        command, env, custom_dir,
        {"project_dir": str(custom_dir), "server_name": "my-cs-server"},
    )
    if result_text is None:
        return False

    has_name_in_msg = "my-cs-server" in result_text
    print_test("Custom name in response", has_name_in_msg)

    content = json.loads((custom_dir / ".claude" / "settings.json").read_text())
    post_hooks = content["hooks"]["PostToolUse"][0]["hooks"]
    server_in_config = post_hooks[0].get("server") == "my-cs-server"
    print_test("Custom name in config", server_in_config)

    return has_name_in_msg and server_in_config


def _test_agent_message(
    command: list[str],
    env: dict,
    project_dir: Path,
    agent: str,
) -> str | None:
    """Call setup_hooks with a specific agent and return result text."""
    return _call_setup_hooks(
        command, env, project_dir,
        {"project_dir": str(project_dir), "agent": agent},
    )


def test_unsupported_agent(command: list[str], env: dict, project_dir: Path) -> bool:
    """Test that unsupported agents return an informative message."""
    print_header("Test: Unsupported Agent")
    result = _test_agent_message(command, env, project_dir, agent="cursor")
    if result is None:
        return False
    has_expected = "not yet supported" in result.lower()
    print_test("Response contains 'not yet supported'", has_expected, result[:200])
    return has_expected


def test_unknown_agent(command: list[str], env: dict, project_dir: Path) -> bool:
    """Test that unknown agents return an error."""
    print_header("Test: Unknown Agent")
    result = _test_agent_message(command, env, project_dir, agent="vscode")
    if result is None:
        return False
    has_expected = "unknown agent" in result.lower()
    print_test("Response contains 'unknown agent'", has_expected, result[:200])
    return has_expected


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_setup_hooks.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Setup Hooks Integration Tests")
    print("\nThese tests verify the setup_hooks tool correctly installs")
    print("agent hook configurations into project directories.")

    return run_setup_hooks_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

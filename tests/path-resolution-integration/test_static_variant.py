#!/usr/bin/env python3
"""
Static binary variant path resolution integration test.

This test verifies that the MCP tools that require file path resolution
work correctly in static executable mode (without CS_MOUNT_PATH set).

The tools tested are:
- code_ownership_for_path
- list_technical_debt_hotspots_for_project_file
- list_technical_debt_goals_for_project_file

These tools previously failed with "CS_MOUNT_PATH not defined" error when
running in static executable mode. This test ensures they now work correctly
by using git root detection for path resolution.

Additionally, this test verifies git worktree support for CLI-based tools
when CS_ACCESS_TOKEN is available.

Usage: python test_static_variant.py /path/to/cs-mcp
"""

import os
import sys
from dataclasses import dataclass

from mcp_test_utils import (
    MCPClient,
    ToolTestConfig,
    cleanup_test_dir,
    create_static_mode_env,
    create_test_file_no_git,
    create_test_git_repo,
    create_test_git_worktree,
    print_header,
    print_test,
    print_test_summary,
    run_tool_test,
)


# Common forbidden patterns for path resolution tests
MOUNT_PATH_ERROR = "CS_MOUNT_PATH"
GIT_REPO_ERROR = "not in a git repository"
NONETYPE_ERROR = "NoneType"


@dataclass
class TestEnvironment:
    """Encapsulates all test environment paths and state."""
    binary_path: str
    git_tmpdir: str
    git_test_file: str
    no_git_tmpdir: str
    no_git_test_file: str
    worktree_base_dir: str | None
    worktree_test_file: str | None
    worktree_created: bool
    has_access_token: bool


def test_environment_setup(binary_path: str) -> bool:
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    checks = []
    
    binary_ok = os.path.exists(binary_path) and os.access(binary_path, os.X_OK)
    checks.append(binary_ok)
    print_test("cs-mcp binary exists and is executable", binary_ok, f"Path: {binary_path}")
    
    mount_path = os.getenv('CS_MOUNT_PATH')
    no_mount_ok = mount_path is None
    checks.append(no_mount_ok)
    print_test("CS_MOUNT_PATH is NOT set (static mode)", no_mount_ok, 
               f"Value: {mount_path}" if mount_path else "Not set (correct)")
    
    return all(checks)


def test_mcp_server_starts(binary_path: str) -> bool:
    """Verify the MCP server starts successfully in static mode."""
    print_header("Test MCP Server Startup (Static Mode)")
    
    env = create_static_mode_env()
    client = MCPClient([binary_path], env=env)
    
    try:
        started = client.start()
        print_test("MCP server process started", started)
        if not started:
            return False
        
        response = client.initialize()
        print_test("MCP server responds to initialize", "result" in response)
        return True
    except Exception as e:
        print_test("MCP server starts", False, str(e))
        return False
    finally:
        client.stop()


def build_tool_test_configs(git_test_file: str, no_git_test_file: str) -> list[tuple[str, ToolTestConfig]]:
    """Build all tool test configurations."""
    return [
        # Tests with files inside git repo
        ("code_ownership_for_path (in git repo)", ToolTestConfig(
            tool_name="code_ownership_for_path",
            arguments={"project_id": 1, "path": git_test_file},
            header="Test code_ownership_for_path (In Git Repo)",
            forbidden_patterns=[MOUNT_PATH_ERROR],
            test_description="No CS_MOUNT_PATH error",
        )),
        ("list_technical_debt_hotspots_for_project_file (in git repo)", ToolTestConfig(
            tool_name="list_technical_debt_hotspots_for_project_file",
            arguments={"project_id": 1, "file_path": git_test_file},
            header="Test list_technical_debt_hotspots_for_project_file (In Git Repo)",
            forbidden_patterns=[MOUNT_PATH_ERROR],
            test_description="No CS_MOUNT_PATH error",
        )),
        ("list_technical_debt_goals_for_project_file (in git repo)", ToolTestConfig(
            tool_name="list_technical_debt_goals_for_project_file",
            arguments={"project_id": 1, "file_path": git_test_file},
            header="Test list_technical_debt_goals_for_project_file (In Git Repo)",
            forbidden_patterns=[MOUNT_PATH_ERROR],
            test_description="No CS_MOUNT_PATH error",
        )),
        # Tests outside git repo
        ("code_ownership_for_path (no git repo)", ToolTestConfig(
            tool_name="code_ownership_for_path",
            arguments={"project_id": 1, "path": no_git_test_file},
            header="Test code_ownership_for_path (No Git Repo)",
            forbidden_patterns=[MOUNT_PATH_ERROR, GIT_REPO_ERROR],
            test_description="No CS_MOUNT_PATH or git repo error",
        )),
        # Test with relative path
        ("code_ownership_for_path (relative path)", ToolTestConfig(
            tool_name="code_ownership_for_path",
            arguments={"project_id": 1, "path": "src/components/Button.tsx"},
            header="Test code_ownership_for_path (Relative Path)",
            forbidden_patterns=[MOUNT_PATH_ERROR, GIT_REPO_ERROR],
            test_description="No CS_MOUNT_PATH or git repo error with relative path",
        )),
    ]


def build_worktree_test_configs(worktree_test_file: str) -> list[tuple[str, ToolTestConfig]]:
    """Build worktree-specific test configurations for API-based tools."""
    return [
        # API-based tools in git worktree (should work)
        ("code_ownership_for_path (git worktree)", ToolTestConfig(
            tool_name="code_ownership_for_path",
            arguments={"project_id": 1, "path": worktree_test_file},
            header="Test code_ownership_for_path (Git Worktree)",
            forbidden_patterns=[MOUNT_PATH_ERROR, NONETYPE_ERROR],
            test_description="No CS_MOUNT_PATH or NoneType error in worktree",
        )),
        ("list_technical_debt_hotspots_for_project_file (git worktree)", ToolTestConfig(
            tool_name="list_technical_debt_hotspots_for_project_file",
            arguments={"project_id": 1, "file_path": worktree_test_file},
            header="Test list_technical_debt_hotspots_for_project_file (Git Worktree)",
            forbidden_patterns=[MOUNT_PATH_ERROR, NONETYPE_ERROR],
            test_description="No CS_MOUNT_PATH or NoneType error in worktree",
        )),
    ]


def build_worktree_cli_test_configs(worktree_test_file: str) -> list[tuple[str, ToolTestConfig]]:
    """
    Build worktree test configurations for CLI-based tools.
    
    These tests require CS_ACCESS_TOKEN to be set as they invoke the actual CLI.
    They verify that the worktree GIT_DIR fix works for code_health_score and
    related tools.
    """
    return [
        # CLI-based tools in git worktree (requires GIT_DIR fix)
        ("code_health_score (git worktree)", ToolTestConfig(
            tool_name="code_health_score",
            arguments={"file_path": worktree_test_file},
            header="Test code_health_score (Git Worktree - CLI)",
            forbidden_patterns=[NONETYPE_ERROR, "Error:"],
            test_description="CLI tool works in worktree (returns valid score)",
            required_patterns=["code health score"],  # Response must contain "Code Health score: X.X"
        )),
    ]


def print_test_banner() -> None:
    """Print the test suite banner."""
    print("\n" + "="*60)
    print("  Static Binary Path Resolution Integration Tests")
    print("  Testing: cs-mcp binary without CS_MOUNT_PATH")
    print("="*60)


def create_worktree_environment() -> tuple[str | None, str | None, bool]:
    """Create test worktree environment. Returns (base_dir, test_file, success)."""
    print("\n  Creating temporary git worktree for testing...")
    try:
        base_dir, worktree_dir, test_file = create_test_git_worktree()
        print(f"  Worktree dir: {worktree_dir}")
        print(f"  Test file: {test_file}")
        return base_dir, test_file, True
    except Exception as e:
        print(f"  Warning: Could not create worktree (git worktree may not be available): {e}")
        return None, None, False


def setup_test_environment(binary_path: str) -> TestEnvironment:
    """Set up all test directories and files."""
    print("\n  Creating temporary git repository for testing...")
    git_tmpdir, git_test_file = create_test_git_repo()
    print(f"  Test repo: {git_tmpdir}")
    print(f"  Test file: {git_test_file}")
    
    print("\n  Creating temporary file outside git repo...")
    no_git_tmpdir, no_git_test_file = create_test_file_no_git()
    print(f"  Test dir: {no_git_tmpdir}")
    print(f"  Test file: {no_git_test_file}")
    
    worktree_base_dir, worktree_test_file, worktree_created = create_worktree_environment()
    
    has_access_token = os.getenv("CS_ACCESS_TOKEN") is not None
    if has_access_token:
        print("\n  CS_ACCESS_TOKEN detected - will run CLI-based worktree tests")
    else:
        print("\n  CS_ACCESS_TOKEN not set - skipping CLI-based worktree tests")
        print("  (Set CS_ACCESS_TOKEN to test code_health_score in worktrees)")
    
    return TestEnvironment(
        binary_path=binary_path,
        git_tmpdir=git_tmpdir,
        git_test_file=git_test_file,
        no_git_tmpdir=no_git_tmpdir,
        no_git_test_file=no_git_test_file,
        worktree_base_dir=worktree_base_dir,
        worktree_test_file=worktree_test_file,
        worktree_created=worktree_created,
        has_access_token=has_access_token,
    )


def run_test_configs(
    binary_path: str,
    env: dict,
    configs: list[tuple[str, ToolTestConfig]],
) -> list[tuple[str, bool]]:
    """Run a list of test configurations and return results."""
    results = []
    for name, config in configs:
        passed = run_tool_test(command=[binary_path], env=env, config=config)
        results.append((name, passed))
    return results


def run_worktree_tests(test_env: TestEnvironment, env: dict) -> list[tuple[str, bool]]:
    """Run worktree-specific tests if applicable."""
    if not test_env.worktree_created or not test_env.worktree_test_file:
        return []
    
    results = []
    print_header("Git Worktree Tests")
    print("  Testing tools in git worktree environment...")
    
    # API-based worktree tests (always run)
    results.extend(run_test_configs(
        test_env.binary_path, env, build_worktree_test_configs(test_env.worktree_test_file)
    ))
    
    # CLI-based worktree tests (only if CS_ACCESS_TOKEN is set)
    if test_env.has_access_token:
        results.extend(run_test_configs(
            test_env.binary_path, env, build_worktree_cli_test_configs(test_env.worktree_test_file)
        ))
    
    return results


def run_all_tests(test_env: TestEnvironment) -> list[tuple[str, bool]]:
    """Run all tests and return results."""
    results = [
        ("Environment Setup", test_environment_setup(test_env.binary_path)),
        ("MCP Server Startup", test_mcp_server_starts(test_env.binary_path)),
    ]
    
    env = create_static_mode_env()
    
    # Run tool tests from configuration
    results.extend(run_test_configs(
        test_env.binary_path, env, 
        build_tool_test_configs(test_env.git_test_file, test_env.no_git_test_file)
    ))
    
    # Run worktree tests
    results.extend(run_worktree_tests(test_env, env))
    
    return results


def cleanup_test_environment(test_env: TestEnvironment) -> None:
    """Clean up all test directories."""
    cleanup_test_dir(test_env.git_tmpdir)
    cleanup_test_dir(test_env.no_git_tmpdir)
    if test_env.worktree_base_dir:
        cleanup_test_dir(test_env.worktree_base_dir)


def main():
    if len(sys.argv) < 2:
        print("Usage: python test_static_variant.py /path/to/cs-mcp")
        return 1
    
    binary_path = sys.argv[1]
    
    print_test_banner()
    
    test_env = setup_test_environment(binary_path)
    
    try:
        results = run_all_tests(test_env)
        return print_test_summary(results)
    finally:
        cleanup_test_environment(test_env)


if __name__ == '__main__':
    sys.exit(main())

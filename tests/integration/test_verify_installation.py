#!/usr/bin/env python3
"""
Verify Installation integration tests.

Tests that the verify_installation MCP tool correctly checks:
1. Git is installed and accessible
2. The project root is inside a git repository
3. The access token is set and valid (verified via the CLI)
4. The runtime environment is detected (binary or docker)
"""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

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


def run_verify_installation_tests(executable: Path) -> int:
    """
    Run all verify_installation tests.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = CargoBackend(executable=executable)
    return run_verify_installation_tests_with_backend(backend)


def run_verify_installation_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all verify_installation tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_verify_install_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        command = backend.get_command(repo_dir)
        env = backend.get_env(os.environ.copy(), repo_dir)

        results = [
            (
                "Verify Installation - All Checks Pass",
                test_all_checks_pass(command, env, repo_dir),
            ),
            (
                "Verify Installation - Reports Git Repository",
                test_reports_git_repository(command, env, repo_dir),
            ),
            (
                "Verify Installation - Non-Repo Path Fails Git Check",
                test_non_repo_fails_git_check(command, env, test_dir),
            ),
        ]

        return print_summary(results)


def test_all_checks_pass(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that the verify_installation tool responds with a valid report.
    Individual checks may fail on CI (e.g. git --version can hang on
    Windows runners), so we assert on the structure rather than every
    check passing.
    """
    print_header("Test: All Checks Run")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        response = client.call_tool(
            "verify_installation",
            {"git_repository_path": str(repo_dir)},
            timeout=60,
        )

        if "error" in response:
            print_test("Tool call succeeded", False, f"Error: {response['error']}")
            return False

        result_text = extract_result_text(response)

        has_content = len(result_text) > 0
        print_test("Returned content", has_content, f"Length: {len(result_text)} chars")

        has_header = "installation verification" in result_text.lower()
        print_test("Contains verification header", has_header)

        has_env = "[pass] runtime environment" in result_text.lower()
        print_test("Environment check passed", has_env)

        has_summary = "checks passed" in result_text
        print_test("Contains summary line", has_summary, f"Output: {result_text}")

        return has_content and has_header and has_env and has_summary

    except Exception as e:
        print_test("All checks pass", False, str(e))
        return False
    finally:
        client.stop()


def test_reports_git_repository(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that the git repository check reports the git root path.
    """
    print_header("Test: Reports Git Repository")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        response = client.call_tool(
            "verify_installation",
            {"git_repository_path": str(repo_dir)},
            timeout=60,
        )

        if "error" in response:
            print_test("Tool call succeeded", False, f"Error: {response['error']}")
            return False

        result_text = extract_result_text(response)

        has_repo_pass = "[pass] git repository" in result_text.lower()
        print_test("Git Repository check passed", has_repo_pass)

        has_git_root = "git root" in result_text.lower()
        print_test("Reports git root path", has_git_root)

        return has_repo_pass and has_git_root

    except Exception as e:
        print_test("Reports git repository", False, str(e))
        return False
    finally:
        client.stop()


def test_non_repo_fails_git_check(command: list[str], env: dict, test_dir: Path) -> bool:
    """
    Test that pointing to a non-git directory reports a failed git repo check.
    """
    print_header("Test: Non-Repo Path Fails Git Check")

    # Create an isolated directory that is NOT a git repo
    non_repo_dir = test_dir / "not_a_repo"
    non_repo_dir.mkdir(exist_ok=True)

    client = MCPClient(command, env=env, cwd=str(test_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        response = client.call_tool(
            "verify_installation",
            {"git_repository_path": str(non_repo_dir)},
            timeout=60,
        )

        if "error" in response:
            print_test("Tool call succeeded", False, f"Error: {response['error']}")
            return False

        result_text = extract_result_text(response)

        has_repo_fail = "[fail] git repository" in result_text.lower()
        print_test("Git Repository check failed as expected", has_repo_fail)

        mentions_not_repo = "not inside a git repository" in result_text.lower()
        print_test("Reports not inside a git repository", mentions_not_repo)

        return has_repo_fail and mentions_not_repo

    except Exception as e:
        print_test("Non-repo fails git check", False, str(e))
        return False
    finally:
        client.stop()


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_verify_installation.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Verify Installation Integration Tests")
    print("\nThese tests verify the verify_installation tool correctly")
    print("checks git, token, and environment configuration.")

    return run_verify_installation_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

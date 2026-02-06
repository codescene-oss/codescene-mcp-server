#!/usr/bin/env python3
"""
Git worktree integration tests.

Tests that the MCP server correctly handles git worktrees,
which have special path resolution requirements.
"""

import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

from test_utils import (
    MCPClient,
    NuitkaBackend,
    ServerBackend,
    create_git_repo,
    extract_code_health_score,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)


def create_worktree(repo_dir: Path, worktree_name: str = "feature-branch") -> Path:
    """
    Create a git worktree from the main repository.

    Args:
        repo_dir: Main repository directory
        worktree_name: Name for the worktree branch and directory

    Returns:
        Path to the created worktree
    """
    worktree_dir = repo_dir.parent / f"worktree_{worktree_name}"

    # Create worktree from master branch (creates new branch automatically)
    subprocess.run(
        ["git", "worktree", "add", "-b", worktree_name, str(worktree_dir), "master"],
        cwd=repo_dir,
        check=True,
        capture_output=True,
    )

    return worktree_dir


def cleanup_worktree(repo_dir: Path, worktree_dir: Path) -> None:
    """Clean up a git worktree."""
    try:
        subprocess.run(
            ["git", "worktree", "remove", str(worktree_dir), "--force"],
            cwd=repo_dir,
            capture_output=True,
        )
    except Exception as e:
        print(f"  Warning: Failed to remove worktree: {e}")


def run_worktree_tests(executable: Path) -> int:
    """
    Run all git worktree tests.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = NuitkaBackend(executable=executable)
    return run_worktree_tests_with_backend(backend)


def run_worktree_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all git worktree tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_worktree_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        # Create main git repo
        print("\nCreating main repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        # Create worktree
        print("\nCreating git worktree...")
        try:
            worktree_dir = create_worktree(repo_dir, "test-feature")
            print(f"Worktree: {worktree_dir}")
        except Exception as e:
            print(f"\nError creating worktree: {e}")
            print("Git worktree may not be available. Skipping tests.")
            return 0

        try:
            # Get command and env from backend, using worktree_dir as working directory
            command = backend.get_command(worktree_dir)
            env = backend.get_env(os.environ.copy(), worktree_dir)

            results = [
                (
                    "Code Health Score",
                    test_worktree_code_health_score(command, env, worktree_dir),
                ),
                (
                    "Code Health Review",
                    test_worktree_code_health_review(command, env, worktree_dir),
                ),
                (
                    "Pre-commit Safeguard",
                    test_worktree_pre_commit(command, env, worktree_dir),
                ),
                (
                    "Absolute Paths",
                    test_worktree_absolute_paths(command, env, worktree_dir),
                ),
            ]

            return print_summary(results)
        finally:
            cleanup_worktree(repo_dir, worktree_dir)


def test_worktree_code_health_score(command: list[str], env: dict, worktree_dir: Path) -> bool:
    """Test code_health_score in a git worktree."""
    print_header("Test: Code Health Score in Git Worktree")

    client = MCPClient(command, env=env, cwd=str(worktree_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = worktree_dir / "src/utils/calculator.py"

        print(f"\n  Testing in worktree: {worktree_dir}")
        print(f"  File: {test_file}")

        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        result_text = extract_result_text(response)
        score = extract_code_health_score(result_text)

        if score is None:
            print_test("Code Health Score returned", False, f"Response: {result_text[:200]}")
            return False

        print_test("Code Health Score returned", True, f"Score: {score}")

        no_worktree_errors = all(err not in result_text.lower() for err in ["nonetype", "git_dir", "worktree error", "traceback"])
        print_test("No worktree-related errors", no_worktree_errors)

        return score is not None and no_worktree_errors

    except Exception as e:
        print_test("Worktree code health score test", False, str(e))
        return False
    finally:
        client.stop()


def test_worktree_code_health_review(command: list[str], env: dict, worktree_dir: Path) -> bool:
    """Test code_health_review in a git worktree."""
    print_header("Test: Code Health Review in Git Worktree")

    client = MCPClient(command, env=env, cwd=str(worktree_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = worktree_dir / "src/services/order_processor.py"

        print(f"\n  Reviewing file in worktree: {test_file}")

        response = client.call_tool("code_health_review", {"file_path": str(test_file)}, timeout=60)
        result_text = extract_result_text(response)

        has_content = len(result_text) > 50
        print_test("Review returned content", has_content, f"Length: {len(result_text)} chars")

        has_health_info = any(term in result_text.lower() for term in ["health", "score", "function"])
        print_test("Review contains Code Health info", has_health_info)

        no_errors = "traceback" not in result_text.lower()
        print_test("No errors in response", no_errors)

        return has_content and no_errors

    except Exception as e:
        print_test("Worktree code health review test", False, str(e))
        return False
    finally:
        client.stop()


def test_worktree_pre_commit(command: list[str], env: dict, worktree_dir: Path) -> bool:
    """Test pre_commit_code_health_safeguard in a git worktree."""
    print_header("Test: Pre-commit Safeguard in Git Worktree")

    client = MCPClient(command, env=env, cwd=str(worktree_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        # Modify a file in the worktree
        test_file = worktree_dir / "src/utils/calculator.py"
        original_content = test_file.read_text()
        test_file.write_text(original_content + "\n# Worktree modification\n")

        # Stage the change
        subprocess.run(
            ["git", "add", str(test_file)],
            cwd=worktree_dir,
            check=True,
            capture_output=True,
        )

        print("\n  Running safeguard in worktree with modification...")

        response = client.call_tool(
            "pre_commit_code_health_safeguard",
            {"git_repository_path": str(worktree_dir)},
            timeout=60,
        )

        result_text = extract_result_text(response)

        has_content = len(result_text) > 20
        print_test(
            "Safeguard returned content",
            has_content,
            f"Length: {len(result_text)} chars",
        )

        no_errors = "traceback" not in result_text.lower() and "nonetype" not in result_text.lower()
        print_test("No errors in response", no_errors)

        # Reset the file
        test_file.write_text(original_content)
        subprocess.run(
            ["git", "reset", "HEAD", str(test_file)],
            cwd=worktree_dir,
            capture_output=True,
        )

        return has_content and no_errors

    except Exception as e:
        print_test("Worktree pre-commit test", False, str(e))
        return False
    finally:
        client.stop()


def test_worktree_absolute_paths(command: list[str], env: dict, worktree_dir: Path) -> bool:
    """Test absolute path resolution in git worktree."""
    print_header("Test: Absolute Paths in Git Worktree")

    client = MCPClient(command, env=env, cwd=str(worktree_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = worktree_dir / "src/utils/calculator.py"

        print(f"\n  Testing absolute path: {test_file}")

        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        result_text = extract_result_text(response)
        score = extract_code_health_score(result_text)

        if score is None:
            print_test("Absolute path resolved", False, f"Response: {result_text[:200]}")
            return False

        print_test("Absolute path resolved", True, f"Score: {score}")
        return True

    except Exception as e:
        print_test("Worktree absolute paths test", False, str(e))
        return False
    finally:
        client.stop()


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_git_worktree.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Git Worktree Integration Tests")

    return run_worktree_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""
Auto-refactor functional integration tests.

Verifies that the code_health_auto_refactor tool correctly handles two
distinct scenarios using the real ACE API:

1. Bad-quality code (with code smells) — the tool should call ACE and
   return refactoring results containing code and confidence.
2. Good-quality code (no code smells) — the tool should report that no
   code smells were found without calling ACE.

These tests prevent regressions in the end-to-end auto-refactor pipeline:
CLI parse-fns, CLI review, code-smell extraction, ACE API call, and
response formatting.

Requires CS_ACE_ACCESS_TOKEN to be set (real ACE token).
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


# ---------------------------------------------------------------------------
# Environment helper
# ---------------------------------------------------------------------------

def _build_env(backend: ServerBackend, repo_dir: Path) -> dict:
    """Build the subprocess environment for auto-refactor tests."""
    env = backend.get_env(os.environ.copy(), repo_dir)
    env["CS_DISABLE_VERSION_CHECK"] = "1"
    env["CS_DISABLE_TRACKING"] = "1"
    return env


# ---------------------------------------------------------------------------
# Test: bad-quality code triggers ACE refactoring
# ---------------------------------------------------------------------------

def test_bad_quality_triggers_refactoring(
    backend: ServerBackend, repo_dir: Path,
) -> bool:
    """Test that a complex function with code smells returns refactoring results."""
    print_header("Test: Bad-Quality Code Triggers ACE Refactoring")

    env = _build_env(backend, repo_dir)
    command = backend.get_command(repo_dir)
    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False
        print_test("Server started", True)
        client.initialize()

        test_file = str(repo_dir / "src/services/order_processor.js")
        print(f"\n  Refactoring: {test_file}  function: processOrder")

        response = client.call_tool(
            "code_health_auto_refactor",
            {"file_path": test_file, "function_name": "processOrder"},
            timeout=120,
        )
        result_text = extract_result_text(response)

        has_content = len(result_text) > 0
        print_test("Tool returned content", has_content, f"Length: {len(result_text)} chars")

        # The response should contain refactored code (a "def" or "code" key)
        has_code = "def " in result_text or '"code"' in result_text
        print_test("Response contains refactored code", has_code)

        # The response should mention confidence level
        has_confidence = "confidence" in result_text.lower()
        print_test("Response contains confidence", has_confidence)

        # The response must not be an error
        is_not_error = "error" not in result_text[:20].lower()
        print_test("Response is not an error", is_not_error, f"Start: {result_text[:80]}")

        return has_content and has_code and has_confidence and is_not_error

    except Exception as e:
        print_test("Bad-quality refactoring test", False, str(e))
        return False
    finally:
        client.stop()


# ---------------------------------------------------------------------------
# Test: good-quality code reports no code smells
# ---------------------------------------------------------------------------

def test_good_quality_reports_no_smells(
    backend: ServerBackend, repo_dir: Path,
) -> bool:
    """Test that a clean function returns a 'no code smells' message."""
    print_header("Test: Good-Quality Code Reports No Code Smells")

    env = _build_env(backend, repo_dir)
    command = backend.get_command(repo_dir)
    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False
        print_test("Server started", True)
        client.initialize()

        test_file = str(repo_dir / "src/utils/calculator.py")
        print(f"\n  Refactoring: {test_file}  function: calculate_total")

        response = client.call_tool(
            "code_health_auto_refactor",
            {"file_path": test_file, "function_name": "calculate_total"},
            timeout=120,
        )
        result_text = extract_result_text(response)

        has_content = len(result_text) > 0
        print_test("Tool returned content", has_content, f"Length: {len(result_text)} chars")

        has_no_smells = "no code smells" in result_text.lower()
        print_test("Response says no code smells", has_no_smells, f"Text: {result_text[:120]}")

        return has_content and has_no_smells

    except Exception as e:
        print_test("Good-quality no-smells test", False, str(e))
        return False
    finally:
        client.stop()


# ---------------------------------------------------------------------------
# Backend-aware runner
# ---------------------------------------------------------------------------

def run_auto_refactor_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all auto-refactor tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_auto_refactor_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        results = [
            (
                "Auto-Refactor - bad-quality code triggers refactoring",
                test_bad_quality_triggers_refactoring(backend, repo_dir),
            ),
            (
                "Auto-Refactor - good-quality code reports no smells",
                test_good_quality_reports_no_smells(backend, repo_dir),
            ),
        ]

        return print_summary(results)


# ---------------------------------------------------------------------------
# CargoBackend convenience wrapper
# ---------------------------------------------------------------------------

def run_auto_refactor_tests(executable: Path) -> int:
    """
    Run all auto-refactor tests.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = CargoBackend(executable=executable)
    return run_auto_refactor_tests_with_backend(backend)


# ---------------------------------------------------------------------------
# Standalone entry point
# ---------------------------------------------------------------------------

def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_auto_refactor.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Auto-Refactor Integration Tests")
    print("\nThese tests verify that code_health_auto_refactor correctly")
    print("handles both bad-quality and good-quality code.")

    return run_auto_refactor_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

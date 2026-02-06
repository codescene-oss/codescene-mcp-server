#!/usr/bin/env python3
"""
Business Case integration tests.

Tests that the MCP server correctly generates business case data for
refactoring recommendations using the code_health_refactoring_business_case tool.

This test suite validates:
1. The tool returns meaningful content for files with varying Code Health
2. The response contains expected business case metrics (defects, development time)
3. The regression coefficient data files (defects.json, time.json) are properly
   bundled and accessible at runtime (preventing the "No such file or directory"
   error that occurs when these files are not included in the Nuitka build)

Issue this prevents: When the s_curve/regression/*.json files are not included
in the Nuitka build, the tool fails with:
    "No such file or directory (os error 2): .../regression/defects.json"
"""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

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


def run_business_case_tests(executable: Path) -> int:
    """
    Run all business case tests.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = NuitkaBackend(executable=executable)
    return run_business_case_tests_with_backend(backend)


def run_business_case_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all business case tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_business_case_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        # Create git repo with sample files
        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        # Get command and env from backend
        command = backend.get_command(repo_dir)
        env = backend.get_env(os.environ.copy(), repo_dir)

        results = [
            (
                "Business Case - Basic Response",
                test_business_case_basic_response(command, env, repo_dir),
            ),
            (
                "Business Case - Contains Metrics",
                test_business_case_contains_metrics(command, env, repo_dir),
            ),
            (
                "Business Case - No Regression File Errors",
                test_business_case_no_file_errors(command, env, repo_dir),
            ),
        ]

        return print_summary(results)


def test_business_case_basic_response(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that business case tool returns a valid response.

    This validates the tool can be called and returns content without crashing.
    """
    print_header("Test: Business Case Basic Response")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        # Use the complex Python file which should benefit from refactoring
        test_file = repo_dir / "src/services/order_processor.py"
        print(f"\n  Analyzing: {test_file}")

        response = client.call_tool(
            "code_health_refactoring_business_case",
            {"file_path": str(test_file)},
            timeout=60,
        )
        result_text = extract_result_text(response)

        # Check we got a non-empty response
        has_content = len(result_text) > 0
        print_test("Business case returned content", has_content, f"Length: {len(result_text)} chars")

        if not has_content:
            print(f"  Response: {response}")
            return False

        return True

    except Exception as e:
        print_test("Business case basic response", False, str(e))
        return False
    finally:
        client.stop()


def test_business_case_contains_metrics(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that business case response contains expected metrics.

    The business case should include information about defects and development
    time improvements from refactoring.
    """
    print_header("Test: Business Case Contains Metrics")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/services/order_processor.py"
        print(f"\n  Analyzing: {test_file}")

        response = client.call_tool(
            "code_health_refactoring_business_case",
            {"file_path": str(test_file)},
            timeout=60,
        )
        result_text = extract_result_text(response)

        # Check for expected business case terms
        # These terms come from the s_curve metrics calculations
        expected_terms = ["defect", "development", "optimistic", "pessimistic", "scenario"]
        found_terms = [term for term in expected_terms if term in result_text.lower()]

        has_metrics = len(found_terms) >= 2
        print_test(
            "Response contains business metrics",
            has_metrics,
            f"Found terms: {found_terms}",
        )

        return has_metrics

    except Exception as e:
        print_test("Business case metrics", False, str(e))
        return False
    finally:
        client.stop()


def test_business_case_no_file_errors(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that business case does not fail with file not found errors.

    This is a regression test for the bug where regression coefficient files
    (defects.json, time.json) were not included in the Nuitka build, causing:
        "No such file or directory (os error 2): .../regression/defects.json"

    The fix was to add --include-data-dir for the s_curve/regression directory
    in the Makefile Nuitka build command.
    """
    print_header("Test: Business Case No Regression File Errors")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        test_file = repo_dir / "src/services/order_processor.py"
        print(f"\n  Analyzing: {test_file}")

        response = client.call_tool(
            "code_health_refactoring_business_case",
            {"file_path": str(test_file)},
            timeout=60,
        )
        result_text = extract_result_text(response)

        # Check for the specific file not found error patterns
        # Check for the specific file not found error patterns
        has_file_error = "no such file or directory" in result_text.lower() or "os error 2" in result_text.lower()

        no_file_errors = not has_file_error
        print_test("No regression file errors", no_file_errors, f"Response preview: {result_text[:300]}...")

        if has_file_error:
            print("\n  ERROR: Regression coefficient files not found!")
            print("  This likely means the s_curve/regression/*.json files are not")
            print("  included in the Nuitka build. Check the Makefile for:")
            print("    --include-data-dir=./src/code_health_refactoring_business_case/s_curve/regression=...")
            return False

        # Additional check: response should not contain traceback
        no_traceback = "traceback" not in result_text.lower()
        print_test("No traceback in response", no_traceback)

        return no_file_errors and no_traceback

    except Exception as e:
        print_test("Business case file errors", False, str(e))
        return False
    finally:
        client.stop()


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_business_case.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Business Case Integration Tests")
    print("\nThese tests verify the code_health_refactoring_business_case tool")
    print("works correctly, including that regression coefficient files are")
    print("properly bundled in the Nuitka executable.")

    return run_business_case_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

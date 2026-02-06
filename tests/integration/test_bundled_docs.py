#!/usr/bin/env python3
"""
Bundled documentation integration tests.

Tests that the MCP server correctly accesses bundled documentation files
using the explain_code_health and explain_code_health_productivity tools.

This test suite validates:
1. The documentation tools return meaningful content
2. The docs directory is properly bundled and accessible at runtime
3. No file not found errors occur when accessing bundled docs

Issue this prevents: When the src/docs directory is not correctly included
in the Nuitka build or the path resolution is wrong, the tools fail with
file not found errors.
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


def run_bundled_docs_tests(executable: Path) -> int:
    """
    Run all bundled docs tests.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = NuitkaBackend(executable=executable)
    return run_bundled_docs_tests_with_backend(backend)


def run_bundled_docs_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all bundled docs tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_bundled_docs_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        # Create git repo with sample files (needed for server startup)
        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        # Get command and env from backend
        command = backend.get_command(repo_dir)
        env = backend.get_env(os.environ.copy(), repo_dir)

        results = [
            (
                "Explain Code Health",
                test_explain_code_health(command, env, repo_dir),
            ),
            (
                "Explain Code Health Productivity",
                test_explain_code_health_productivity(command, env, repo_dir),
            ),
            (
                "No Doc File Errors",
                test_no_doc_file_errors(command, env, repo_dir),
            ),
        ]

        return print_summary(results)


def test_explain_code_health(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that explain_code_health tool returns documentation content.

    This tool reads from docs/code-health/how-it-works.md
    """
    print_header("Test: Explain Code Health")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        print("\n  Calling explain_code_health tool...")

        response = client.call_tool("explain_code_health", {}, timeout=30)
        result_text = extract_result_text(response)

        # Check we got content
        has_content = len(result_text) > 100
        print_test("Documentation returned", has_content, f"Length: {len(result_text)} chars")

        # Check for expected documentation terms
        expected_terms = ["code health", "maintainability", "code", "quality"]
        found_terms = [term for term in expected_terms if term in result_text.lower()]

        has_doc_content = len(found_terms) >= 2
        print_test("Contains documentation content", has_doc_content, f"Found terms: {found_terms}")

        return has_content and has_doc_content

    except Exception as e:
        print_test("Explain code health", False, str(e))
        return False
    finally:
        client.stop()


def test_explain_code_health_productivity(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that explain_code_health_productivity tool returns documentation content.

    This tool reads from docs/code-health/business-case.md
    """
    print_header("Test: Explain Code Health Productivity")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        print("\n  Calling explain_code_health_productivity tool...")

        response = client.call_tool("explain_code_health_productivity", {}, timeout=30)
        result_text = extract_result_text(response)

        # Check we got content
        has_content = len(result_text) > 100
        print_test("Documentation returned", has_content, f"Length: {len(result_text)} chars")

        # Check for expected business case terms
        expected_terms = ["productivity", "defect", "business", "code health"]
        found_terms = [term for term in expected_terms if term in result_text.lower()]

        has_doc_content = len(found_terms) >= 2
        print_test("Contains business case content", has_doc_content, f"Found terms: {found_terms}")

        return has_content and has_doc_content

    except Exception as e:
        print_test("Explain code health productivity", False, str(e))
        return False
    finally:
        client.stop()


def test_no_doc_file_errors(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that documentation tools do not fail with file not found errors.

    This is a regression test to ensure the docs directory is properly
    bundled in the Nuitka build with the correct path mapping.
    """
    print_header("Test: No Documentation File Errors")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        # Test both documentation tools
        tools_to_test = [
            ("explain_code_health", "how-it-works.md"),
            ("explain_code_health_productivity", "business-case.md"),
        ]

        all_passed = True

        for tool_name, doc_file in tools_to_test:
            print(f"\n  Testing {tool_name}...")

            response = client.call_tool(tool_name, {}, timeout=30)
            result_text = extract_result_text(response)

            # Check for file not found error patterns
            has_file_error = any(pattern in result_text.lower() for pattern in ["no such file or directory", "filenotfounderror", "not found"])

            no_errors = not has_file_error
            print_test(f"No file errors for {doc_file}", no_errors)

            if has_file_error:
                print(f"\n  ERROR: Documentation file not found: {doc_file}")
                print("  This likely means the docs directory is not correctly")
                print("  included in the Nuitka build. Check for:")
                print("    --include-data-dir=./src/docs=src/docs")
                all_passed = False

            # Check for traceback
            no_traceback = "traceback" not in result_text.lower()
            print_test(f"No traceback for {tool_name}", no_traceback)

            if not no_traceback:
                all_passed = False

        return all_passed

    except Exception as e:
        print_test("Documentation file errors test", False, str(e))
        return False
    finally:
        client.stop()


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_bundled_docs.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Bundled Documentation Integration Tests")
    print("\nThese tests verify the explain_code_health and")
    print("explain_code_health_productivity tools work correctly,")
    print("including that documentation files are properly bundled")
    print("in the Nuitka executable.")

    return run_bundled_docs_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

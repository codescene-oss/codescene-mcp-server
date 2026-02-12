#!/usr/bin/env python3
"""
Comprehensive MCP Server Integration Test Suite

This test suite:
1. Builds the static executable using Nuitka in an isolated directory
2. Moves the executable OUTSIDE the repo root to avoid picking up bundled CLI
3. Creates test fixtures with real code samples
4. Runs the MCP server and invokes actual tools
5. Validates Code Health scores and other tool responses

This mimics real-world usage where users run cs-mcp without the repo's bundled CLI.

Usage:
    # Run all tests (builds executable automatically)
    python run_all_tests.py

    # Run with existing executable (skips build)
    python run_all_tests.py --executable /path/to/cs-mcp

    # Set required environment variables
    export CS_ACCESS_TOKEN=your_token_here
    export CS_ONPREM_URL=https://your-codescene-instance.com
"""

import argparse
import os
import sys
from pathlib import Path

# Add current directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_expected_scores, get_sample_files

from test_utils import (
    BuildConfig,
    DockerBackend,
    ExecutableBuilder,
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


def check_prerequisites() -> tuple[bool, list[str]]:
    """
    Check if required environment and tools are available.

    Returns:
        Tuple of (all_ok, list_of_issues)
    """
    issues = []

    # Check for CS_ACCESS_TOKEN
    if not os.getenv("CS_ACCESS_TOKEN"):
        issues.append("CS_ACCESS_TOKEN not set (required for actual Code Health analysis)")

    # Check for git
    import shutil

    if not shutil.which("git"):
        issues.append("git not found in PATH")

    return len(issues) == 0, issues


def test_server_startup_with_backend(command: list[str], env: dict, test_dir: Path) -> bool:
    """Test that the MCP server starts successfully (backend version)."""
    print_header("Test 1: MCP Server Startup")

    client = MCPClient(command, env=env, cwd=str(test_dir))

    try:
        started = client.start()
        print_test("Server process started", started)
        if not started:
            stderr = client.get_stderr()
            print(f"  Stderr: {stderr}")
            return False

        response = client.initialize()
        has_result = "result" in response
        print_test("Server responds to initialize", has_result)
        if not has_result:
            print(f"  Response: {response}")

        return has_result
    except Exception as e:
        print_test("Server startup", False, str(e))
        return False
    finally:
        client.stop()


def test_code_health_score_with_backend(command: list[str], env: dict, test_dir: Path, repo_dir: Path) -> list[tuple[str, bool]]:
    """Test code_health_score tool with actual code samples (backend version)."""
    print_header("Test 2: Code Health Score Tool")

    results = []
    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return [("code_health_score - server start", False)]

        print_test("Server started", True)
        client.initialize()

        expected_scores = get_expected_scores()

        for file_path, (min_score, max_score) in expected_scores.items():
            full_path = repo_dir / file_path

            print(f"\n  Testing: {file_path}")
            response = client.call_tool("code_health_score", {"file_path": str(full_path)}, timeout=60)

            result_text = extract_result_text(response)
            score = extract_code_health_score(result_text)

            if score is None:
                print_test(
                    f"  Score returned for {file_path}",
                    False,
                    f"Response: {result_text[:200]}",
                )
                results.append((f"code_health_score - {file_path}", False))
            else:
                in_range = min_score <= score <= max_score
                print_test(
                    f"  Score in expected range ({min_score}-{max_score})",
                    in_range,
                    f"Actual score: {score}",
                )
                results.append((f"code_health_score - {file_path} (score: {score})", in_range))

        return results

    except Exception as e:
        print_test("Code Health Score test", False, str(e))
        return [("code_health_score - exception", False)]
    finally:
        client.stop()


def test_code_health_review_with_backend(command: list[str], env: dict, test_dir: Path, repo_dir: Path) -> bool:
    """Test code_health_review tool (backend version)."""
    print_header("Test 3: Code Health Review Tool")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        # Test with the complex Python file
        test_file = repo_dir / "src/services/order_processor.py"
        print(f"\n  Analyzing: {test_file}")

        response = client.call_tool("code_health_review", {"file_path": str(test_file)}, timeout=60)

        result_text = extract_result_text(response)

        # Check that we got a meaningful response
        has_content = len(result_text) > 50
        print_test("Review returned content", has_content, f"Length: {len(result_text)} chars")

        # Check for common Code Health terms
        has_health_info = any(term in result_text.lower() for term in ["code health", "complexity", "function"])
        print_test("Review contains Code Health information", has_health_info)

        return has_content and has_health_info

    except Exception as e:
        print_test("Code Health Review test", False, str(e))
        return False
    finally:
        client.stop()


def test_pre_commit_safeguard_with_backend(command: list[str], env: dict, test_dir: Path, repo_dir: Path) -> bool:
    """Test pre_commit_code_health_safeguard tool (backend version)."""
    print_header("Test 4: Pre-commit Code Health Safeguard")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        # Make a modification to a file to have something to check
        test_file = repo_dir / "src/utils/calculator.py"
        original_content = test_file.read_text()
        test_file.write_text(original_content + "\n# Test modification\n")

        # Stage the change
        import subprocess

        subprocess.run(
            ["git", "add", str(test_file)],
            cwd=repo_dir,
            check=True,
            capture_output=True,
        )

        print("\n  Running safeguard on modified file...")

        response = client.call_tool(
            "pre_commit_code_health_safeguard",
            {"git_repository_path": str(repo_dir)},
            timeout=60,
        )

        result_text = extract_result_text(response)

        # Check that we got a response
        has_content = len(result_text) > 20
        print_test(
            "Safeguard returned content",
            has_content,
            f"Length: {len(result_text)} chars",
        )

        # Check for safeguard-related terms
        has_safeguard_info = any(term in result_text.lower() for term in ["quality", "gate", "code health", "file"])
        print_test("Safeguard contains quality gate information", has_safeguard_info)

        # Reset the file
        test_file.write_text(original_content)
        subprocess.run(["git", "reset", "HEAD", str(test_file)], cwd=repo_dir, capture_output=True)

        return has_content and has_safeguard_info

    except Exception as e:
        print_test("Pre-commit Safeguard test", False, str(e))
        return False
    finally:
        client.stop()


def test_outside_git_repo_with_backend(command: list[str], env: dict, test_dir: Path) -> bool:
    """Test tools with files outside a git repository (backend version)."""
    print_header("Test 5: Tools Outside Git Repository")

    # Create a standalone file outside any git repo
    standalone_file = test_dir / "standalone.py"
    standalone_file.write_text("def test():\n    pass\n")

    client = MCPClient(command, env=env, cwd=str(test_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        print(f"\n  Testing with standalone file: {standalone_file}")

        response = client.call_tool("code_health_score", {"file_path": str(standalone_file)}, timeout=60)

        result_text = extract_result_text(response)

        # Should either get a score or a clear error (not a crash)
        has_response = len(result_text) > 0
        print_test(
            "Tool handles non-git file gracefully",
            has_response,
            f"Response: {result_text[:200]}",
        )

        # Should not have NoneType errors or similar crashes
        no_crash_errors = "NoneType" not in result_text and "Traceback" not in result_text
        print_test("No crash errors in response", no_crash_errors)

        return has_response and no_crash_errors

    except Exception as e:
        print_test("Outside git repo test", False, str(e))
        return False
    finally:
        client.stop()


def test_no_bundled_cli_interference(executable: Path, test_dir: Path) -> bool:
    """Verify that the test environment doesn't have bundled CLI interference."""
    return test_no_bundled_cli_interference_with_backend([str(executable)], {}, test_dir)


def test_no_bundled_cli_interference_with_backend(command: list[str], env: dict, test_dir: Path) -> bool:
    """Verify that the test environment doesn't have bundled CLI interference (backend version)."""
    print_header("Test 6: No Bundled CLI Interference")

    # Check that cs/cs.exe doesn't exist in the test directory structure
    has_cs = (test_dir / "cs").exists() or (test_dir / "cs.exe").exists()
    print_test("No cs/cs.exe in test directory", not has_cs)

    # Check parent directories up to 3 levels
    parent = test_dir.parent
    for i in range(3):
        has_cs_parent = (parent / "cs").exists() or (parent / "cs.exe").exists()
        print_test(f"No cs/cs.exe in parent {i + 1} levels up", not has_cs_parent)
        if has_cs_parent:
            has_cs = True
        parent = parent.parent

    return not has_cs


def build_executable() -> Path:
    """
    Build the static executable in an isolated directory.

    Returns:
        Path to the built executable
    """
    import shutil

    repo_root = Path(__file__).parent.parent.parent

    # Create build directory outside repo
    with safe_temp_directory(prefix="cs_mcp_build_") as tmp:
        build_dir = tmp / "build"

        config = BuildConfig(repo_root=repo_root, build_dir=build_dir, python_executable=sys.executable)

        builder = ExecutableBuilder(config)
        binary_path = builder.build()

        # Move executable to a persistent location outside repo
        test_bin_dir = repo_root.parent / "cs_mcp_test_bin"
        test_bin_dir.mkdir(exist_ok=True)

        executable_name = binary_path.name
        final_path = test_bin_dir / executable_name

        # Copy instead of move to handle cross-device issues
        shutil.copy2(binary_path, final_path)

        # Make executable on Unix-like systems
        if os.name != "nt" and sys.platform != "win32":
            os.chmod(final_path, 0o755)

        print(f"\n\033[92mExecutable ready:\033[0m {final_path}")
        return final_path


def validate_prerequisites() -> bool:
    """Validate prerequisites and print any issues. Returns True if OK."""
    prereqs_ok, issues = check_prerequisites()
    if prereqs_ok:
        print("\n\033[92mPrerequisites OK\033[0m")
        return True

    print("\n\033[91mPrerequisite checks failed:\033[0m")
    for issue in issues:
        print(f"  - {issue}")
    return False


def get_executable(args) -> Path | None:
    """Get executable from args or build it. Returns None on failure."""
    if args.executable:
        if not args.executable.exists():
            print(f"\n\033[91mError:\033[0m Executable not found: {args.executable}")
            return None
        print(f"\nUsing existing executable: {args.executable}")
        return args.executable

    try:
        return build_executable()
    except Exception as e:
        print(f"\n\033[91mBuild failed:\033[0m {e}")
        return None


def create_backend(args) -> ServerBackend | None:
    """Create the appropriate backend based on command line args."""
    if args.executable:
        if not args.executable.exists():
            print(f"\n\033[91mError:\033[0m Executable not found: {args.executable}")
            return None
        return NuitkaBackend(executable=args.executable)

    if args.backend == "docker":
        return DockerBackend()
    else:
        return NuitkaBackend()


def _run_test_module(module_name: str, run_func, backend: ServerBackend) -> tuple[str, bool]:
    """
    Run a test module and return its result.

    Args:
        module_name: Display name for the test module
        run_func: Function to run tests, takes backend and returns exit code
        backend: Server backend to use

    Returns:
        Tuple of (test_name, passed)
    """
    print("\n" + "=" * 70)
    print(f"  Running {module_name}")
    print("=" * 70)
    result = run_func(backend)
    return (module_name, result == 0)


def run_all_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all integration tests using the specified backend.

    Args:
        backend: Server backend to use for running tests

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    # Create isolated test directory
    with safe_temp_directory(prefix="cs_mcp_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        # Create git repo with sample files
        print("\nCreating test repository with sample files...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository created: {repo_dir}")

        # Get command and environment from backend
        command = backend.get_command(repo_dir)
        base_env = backend.get_env(os.environ.copy(), repo_dir)

        all_results = []

        # Run tests using backend command
        all_results.append(
            (
                "Server Startup",
                test_server_startup_with_backend(command, base_env, test_dir),
            )
        )

        score_results = test_code_health_score_with_backend(command, base_env, test_dir, repo_dir)
        all_results.extend(score_results)

        all_results.append(
            (
                "Code Health Review",
                test_code_health_review_with_backend(command, base_env, test_dir, repo_dir),
            )
        )
        all_results.append(
            (
                "Pre-commit Safeguard",
                test_pre_commit_safeguard_with_backend(command, base_env, test_dir, repo_dir),
            )
        )
        all_results.append(
            (
                "Outside Git Repo",
                test_outside_git_repo_with_backend(command, base_env, test_dir),
            )
        )

        # Note: For Nuitka backend, also test no bundled CLI interference
        if isinstance(backend, NuitkaBackend):
            all_results.append(
                (
                    "No Bundled CLI",
                    test_no_bundled_cli_interference_with_backend(command, base_env, test_dir),
                )
            )

        # Run additional test modules
        from test_analytics_tracking import run_analytics_tracking_tests_with_backend
        from test_bundled_docs import run_bundled_docs_tests_with_backend
        from test_business_case import run_business_case_tests_with_backend
        from test_git_subtree import run_subtree_tests_with_backend
        from test_git_worktree import run_worktree_tests_with_backend
        from test_relative_paths import run_relative_path_tests_with_backend
        from test_version_check import run_version_check_tests_with_backend

        all_results.append(_run_test_module("Git Worktree Tests", run_worktree_tests_with_backend, backend))
        all_results.append(_run_test_module("Git Subtree Tests", run_subtree_tests_with_backend, backend))
        all_results.append(_run_test_module("Relative Path Tests", run_relative_path_tests_with_backend, backend))
        all_results.append(_run_test_module("Business Case Tests", run_business_case_tests_with_backend, backend))
        all_results.append(_run_test_module("Bundled Docs Tests", run_bundled_docs_tests_with_backend, backend))
        all_results.append(_run_test_module("Version Check Tests", run_version_check_tests_with_backend, backend))
        all_results.append(_run_test_module("Analytics Tracking Tests", run_analytics_tracking_tests_with_backend, backend))

        return print_summary(all_results)


def main() -> int:
    parser = argparse.ArgumentParser(description="Run comprehensive MCP integration tests")
    parser.add_argument(
        "--executable",
        type=Path,
        help="Path to existing cs-mcp executable (skips build)",
    )
    parser.add_argument(
        "--backend",
        choices=["static", "docker"],
        default="static",
        help="Backend to use for running the server (default: static)",
    )
    args = parser.parse_args()

    print_header("MCP Server Comprehensive Integration Tests")
    print(f"  Backend: {args.backend}")

    if not validate_prerequisites():
        return 1

    backend = create_backend(args)
    if backend is None:
        return 1

    try:
        backend.prepare()
        return run_all_tests_with_backend(backend)
    except Exception as e:
        print(f"\n\033[91mError:\033[0m {e}")
        import traceback

        traceback.print_exc()
        return 1
    finally:
        backend.cleanup()


if __name__ == "__main__":
    sys.exit(main())

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
import tempfile
from pathlib import Path

# Add current directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))

from test_utils import (
    BuildConfig,
    ExecutableBuilder,
    MCPClient,
    cleanup_dir,
    create_git_repo,
    create_test_environment,
    extract_code_health_score,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
)
from fixtures import get_sample_files, get_expected_scores


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
    
    # Check Python version
    if sys.version_info < (3, 10):
        issues.append(f"Python 3.10+ required, found {sys.version_info.major}.{sys.version_info.minor}")
    
    return len(issues) == 0, issues


def test_server_startup(executable: Path, test_dir: Path) -> bool:
    """Test that the MCP server starts successfully."""
    print_header("Test 1: MCP Server Startup")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(test_dir))
    
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


def test_code_health_score(executable: Path, test_dir: Path, repo_dir: Path) -> list[tuple[str, bool]]:
    """Test code_health_score tool with actual code samples."""
    print_header("Test 2: Code Health Score Tool")
    
    results = []
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
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
                print_test(f"  Score returned for {file_path}", False, f"Response: {result_text[:200]}")
                results.append((f"code_health_score - {file_path}", False))
            else:
                in_range = min_score <= score <= max_score
                print_test(
                    f"  Score in expected range ({min_score}-{max_score})",
                    in_range,
                    f"Actual score: {score}"
                )
                results.append((f"code_health_score - {file_path} (score: {score})", in_range))
        
        return results
        
    except Exception as e:
        print_test("Code Health Score test", False, str(e))
        return [("code_health_score - exception", False)]
    finally:
        client.stop()


def test_code_health_review(executable: Path, test_dir: Path, repo_dir: Path) -> bool:
    """Test code_health_review tool."""
    print_header("Test 3: Code Health Review Tool")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
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


def test_pre_commit_safeguard(executable: Path, test_dir: Path, repo_dir: Path) -> bool:
    """Test pre_commit_code_health_safeguard tool."""
    print_header("Test 4: Pre-commit Code Health Safeguard")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
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
        subprocess.run(["git", "add", str(test_file)], cwd=repo_dir, check=True, capture_output=True)
        
        print(f"\n  Running safeguard on modified file...")
        
        response = client.call_tool(
            "pre_commit_code_health_safeguard",
            {"git_repository_path": str(repo_dir)},
            timeout=60
        )
        
        result_text = extract_result_text(response)
        
        # Check that we got a response
        has_content = len(result_text) > 20
        print_test("Safeguard returned content", has_content, f"Length: {len(result_text)} chars")
        
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


def test_outside_git_repo(executable: Path, test_dir: Path) -> bool:
    """Test tools with files outside a git repository."""
    print_header("Test 5: Tools Outside Git Repository")
    
    env = create_test_environment()
    
    # Create a standalone file outside any git repo
    standalone_file = test_dir / "standalone.py"
    standalone_file.write_text("def test():\n    pass\n")
    
    client = MCPClient([str(executable)], env=env, cwd=str(test_dir))
    
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
        print_test("Tool handles non-git file gracefully", has_response, f"Response: {result_text[:200]}")
        
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
    print_header("Test 6: No Bundled CLI Interference")
    
    # Check that cs/cs.exe doesn't exist in the test directory structure
    has_cs = (test_dir / "cs").exists() or (test_dir / "cs.exe").exists()
    print_test("No cs/cs.exe in test directory", not has_cs)
    
    # Check parent directories up to 3 levels
    parent = test_dir.parent
    for i in range(3):
        has_cs_parent = (parent / "cs").exists() or (parent / "cs.exe").exists()
        print_test(f"No cs/cs.exe in parent {i+1} levels up", not has_cs_parent)
        if has_cs_parent:
            has_cs = True
        parent = parent.parent
    
    return not has_cs


def run_all_tests(executable: Path) -> int:
    """
    Run all integration tests.
    
    Args:
        executable: Path to the cs-mcp executable
        
    Returns:
        Exit code (0 for success, 1 for failure)
    """
    # Create isolated test directory
    with tempfile.TemporaryDirectory(prefix="cs_mcp_test_") as tmp:
        test_dir = Path(tmp)
        print(f"\nTest directory: {test_dir}")
        
        # Create git repo with sample files
        print("\nCreating test repository with sample files...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository created: {repo_dir}")
        
        all_results = []
        
        # Run tests
        all_results.append(("Server Startup", test_server_startup(executable, test_dir)))
        
        score_results = test_code_health_score(executable, test_dir, repo_dir)
        all_results.extend(score_results)
        
        all_results.append(("Code Health Review", test_code_health_review(executable, test_dir, repo_dir)))
        all_results.append(("Pre-commit Safeguard", test_pre_commit_safeguard(executable, test_dir, repo_dir)))
        all_results.append(("Outside Git Repo", test_outside_git_repo(executable, test_dir)))
        all_results.append(("No Bundled CLI", test_no_bundled_cli_interference(executable, test_dir)))
        
        # Run git worktree tests
        print("\n" + "="*70)
        print("  Running Git Worktree Tests")
        print("="*70)
        from test_git_worktree import run_worktree_tests
        worktree_result = run_worktree_tests(executable)
        all_results.append(("Git Worktree Tests", worktree_result == 0))
        
        # Run git subtree tests
        print("\n" + "="*70)
        print("  Running Git Subtree Tests")
        print("="*70)
        from test_git_subtree import run_subtree_tests
        subtree_result = run_subtree_tests(executable)
        all_results.append(("Git Subtree Tests", subtree_result == 0))
        
        return print_summary(all_results)


def build_executable() -> Path:
    """
    Build the static executable in an isolated directory.
    
    Returns:
        Path to the built executable
    """
    repo_root = Path(__file__).parent.parent.parent
    
    # Create build directory outside repo
    with tempfile.TemporaryDirectory(prefix="cs_mcp_build_") as tmp:
        build_dir = Path(tmp) / "build"
        
        config = BuildConfig(
            repo_root=repo_root,
            build_dir=build_dir,
            python_executable=sys.executable
        )
        
        builder = ExecutableBuilder(config)
        binary_path = builder.build()
        
        # Move executable to a persistent location outside repo
        test_bin_dir = repo_root.parent / "cs_mcp_test_bin"
        test_bin_dir.mkdir(exist_ok=True)
        
        executable_name = binary_path.name
        final_path = test_bin_dir / executable_name
        
        # Copy instead of move to handle cross-device issues
        import shutil
        shutil.copy2(binary_path, final_path)
        
        # Make executable on Unix-like systems
        if os.name != "nt" and sys.platform != "win32":
            os.chmod(final_path, 0o755)
        
        print(f"\n\033[92mExecutable ready:\033[0m {final_path}")
        return final_path


def main() -> int:
    parser = argparse.ArgumentParser(description="Run comprehensive MCP integration tests")
    parser.add_argument(
        "--executable",
        type=Path,
        help="Path to existing cs-mcp executable (skips build)"
    )
    args = parser.parse_args()
    
    print_header("MCP Server Comprehensive Integration Tests")
    
    # Check prerequisites
    prereqs_ok, issues = check_prerequisites()
    if not prereqs_ok:
        print("\n\033[91mPrerequisite checks failed:\033[0m")
        for issue in issues:
            print(f"  - {issue}")
        return 1
    
    print("\n\033[92mPrerequisites OK\033[0m")
    
    # Get or build executable
    if args.executable:
        executable = args.executable
        if not executable.exists():
            print(f"\n\033[91mError:\033[0m Executable not found: {executable}")
            return 1
        print(f"\nUsing existing executable: {executable}")
    else:
        try:
            executable = build_executable()
        except Exception as e:
            print(f"\n\033[91mBuild failed:\033[0m {e}")
            return 1
    
    # Run tests
    return run_all_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

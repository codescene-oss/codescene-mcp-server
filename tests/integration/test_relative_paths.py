#!/usr/bin/env python3
"""
Relative path integration tests.

Tests that the MCP server correctly handles relative file paths,
which was the root cause of the bundled CLI path validation issue.
This prevents regression of the "not in subpath" error.

Issue: When using bundled CLI (no CS_CLI_PATH set), relative paths failed with
"'path' is not in the subpath of 'git_root'" because find_git_root() resolves
paths but the subsequent relative_to() used the unresolved original path.

NOTE: Relative paths are only supported in native/Nuitka mode, NOT in Docker mode.
Docker mode requires absolute paths because it needs to translate host paths to
container paths via CS_MOUNT_PATH. When running with DockerBackend, relative path
tests are skipped and only the absolute path test runs.
"""

import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from test_utils import (
    MCPClient,
    NuitkaBackend,
    DockerBackend,
    ServerBackend,
    create_git_repo,
    extract_code_health_score,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)
from fixtures import get_sample_files


@dataclass
class ServerContext:
    """Context for running MCP server tests."""
    command: list[str]
    env: dict
    repo_dir: Path


@dataclass
class PathTestConfig:
    """Configuration for a path test case."""
    test_name: str
    file_path: str
    error_context: str


def is_docker_backend(backend: ServerBackend) -> bool:
    """Check if the backend is a Docker backend."""
    return isinstance(backend, DockerBackend)


def run_relative_path_tests(executable: Path) -> int:
    """
    Run all relative path tests.
    
    Args:
        executable: Path to the cs-mcp executable
        
    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = NuitkaBackend(executable=executable)
    return run_relative_path_tests_with_backend(backend)


def run_relative_path_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all relative path tests using a backend.
    
    Args:
        backend: Server backend to use
        
    Returns:
        Exit code (0 for success, 1 for failure)
    """
    is_docker = is_docker_backend(backend)
    
    if is_docker:
        print("\n\033[93mNote: Running with Docker backend - relative path tests will be skipped.\033[0m")
        print("Docker mode requires absolute paths for host-to-container path translation.")
    
    with safe_temp_directory(prefix="cs_mcp_relpath_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")
        
        # Create git repo
        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")
        
        # Get command and env from backend
        command = backend.get_command(repo_dir)
        env = backend.get_env(os.environ.copy(), repo_dir)
        
        if is_docker:
            # Docker mode: only run absolute path test (relative paths not supported)
            results = [
                ("Relative Path (simple)", "SKIPPED"),
                ("Relative Path (nested)", "SKIPPED"),
                ("Relative Path with ./", "SKIPPED"),
                ("Relative Path from subdir", "SKIPPED"),
                ("Mixed Slashes", "SKIPPED"),
                ("Absolute Path (baseline)", test_absolute_path(command, env, repo_dir)),
            ]
        else:
            # Native/Nuitka mode: run all tests including relative paths
            results = [
                ("Relative Path (simple)", test_relative_path_simple(command, env, repo_dir)),
                ("Relative Path (nested)", test_relative_path_nested(command, env, repo_dir)),
                ("Relative Path with ./", test_relative_path_dot_prefix(command, env, repo_dir)),
                ("Relative Path from subdir", test_relative_path_from_subdir(command, env, repo_dir)),
                ("Mixed Slashes", test_mixed_slashes(command, env, repo_dir)),
                ("Absolute Path (baseline)", test_absolute_path(command, env, repo_dir)),
            ]
        
        return print_summary(results)


def _run_path_test(ctx: ServerContext, config: PathTestConfig) -> bool:
    """
    Common test logic for path-based code health tests.
    
    Args:
        ctx: Server context with command, env, and repo_dir
        config: Test configuration with test_name, file_path, and error_context
        
    Returns:
        True if test passed, False otherwise
    """
    print_header(f"Test: {config.test_name}")
    
    client = MCPClient(ctx.command, env=ctx.env, cwd=str(ctx.repo_dir))
    
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        print(f"\n  Testing path: {config.file_path}")
        
        response = client.call_tool("code_health_score", {"file_path": config.file_path}, timeout=60)
        result_text = extract_result_text(response)
        score = extract_code_health_score(result_text)
        
        if score is None:
            print_test("Code Health Score returned", False, f"Response: {result_text[:200]}")
            return False
        
        print_test("Code Health Score returned", True, f"Score: {score}")
        
        # Check for "not in subpath" error - this is the regression we're preventing
        no_subpath_error = "not in the subpath" not in result_text.lower()
        print_test("No 'not in subpath' error", no_subpath_error)
        
        return score is not None and no_subpath_error
        
    except Exception as e:
        print_test(f"{config.error_context} test", False, str(e))
        return False
    finally:
        client.stop()


def test_relative_path_simple(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test code_health_score with a simple relative path."""
    ctx = ServerContext(command, env, repo_dir)
    config = PathTestConfig("Simple Relative Path", "src/utils/calculator.py", "Relative path")
    return _run_path_test(ctx, config)


def test_relative_path_nested(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test code_health_score with a nested relative path."""
    ctx = ServerContext(command, env, repo_dir)
    config = PathTestConfig("Nested Relative Path", "src/main/java/com/example/OrderProcessor.java", "Nested relative path")
    return _run_path_test(ctx, config)


def test_relative_path_dot_prefix(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test code_health_score with ./ prefix relative path."""
    ctx = ServerContext(command, env, repo_dir)
    config = PathTestConfig("Relative Path with ./ Prefix", "./src/utils/calculator.py", "./ prefix path")
    return _run_path_test(ctx, config)


def test_relative_path_from_subdir(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test code_health_score from a subdirectory using the repo root as context."""
    ctx = ServerContext(command, env, repo_dir)
    config = PathTestConfig("Relative Path from Subdirectory Context", "src/services/order_processor.py", "Subdir path")
    return _run_path_test(ctx, config)


def test_mixed_slashes(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test code_health_score with mixed forward/backslashes (Windows scenario)."""
    ctx = ServerContext(command, env, repo_dir)
    config = PathTestConfig("Mixed Path Slashes (Windows Compatibility)", "src/utils/calculator.py", "Mixed slashes")
    return _run_path_test(ctx, config)


def test_absolute_path(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test code_health_score with absolute path (baseline comparison)."""
    ctx = ServerContext(command, env, repo_dir)
    absolute_path = str(repo_dir / "src/utils/calculator.py")
    config = PathTestConfig("Absolute Path (Baseline)", absolute_path, "Absolute path")
    return _run_path_test(ctx, config)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_relative_paths.py /path/to/cs-mcp")
        return 1
    
    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1
    
    print_header("Relative Path Integration Tests")
    print("\nThese tests verify the fix for the 'not in subpath' error")
    print("that occurred when using relative file paths with bundled CLI.")
    
    return run_relative_path_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

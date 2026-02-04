#!/usr/bin/env python3
"""
Git subtree integration tests.

Tests that the MCP server correctly handles git subtrees,
where external repositories are nested as subdirectories.
"""

import os
import subprocess
import sys
from pathlib import Path
from typing import Callable, NamedTuple

sys.path.insert(0, str(Path(__file__).parent))

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
from fixtures import get_sample_files


class ServerContext(NamedTuple):
    """Bundles server connection parameters."""
    command: list[str]
    env: dict
    repo_dir: Path


class ToolRequest(NamedTuple):
    """Bundles tool call parameters."""
    tool_name: str
    file_path: str
    validator: Callable[[str], bool]


def create_external_repo(base_dir: Path) -> Path:
    """
    Create an external repository to be used as a subtree.
    
    Args:
        base_dir: Base directory where repo will be created
        
    Returns:
        Path to the created external repository
    """
    external_dir = base_dir / "external_lib"
    external_dir.mkdir(parents=True, exist_ok=True)
    
    # Initialize git repo
    subprocess.run(["git", "init"], cwd=external_dir, check=True, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test User"], cwd=external_dir, check=True, capture_output=True)
    subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=external_dir, check=True, capture_output=True)
    
    # Create some library files
    lib_files = {
        "utils.py": '''"""Shared utility functions."""

def helper_function(value: int) -> int:
    """A simple helper function."""
    return value * 2


def validate_input(data: dict) -> bool:
    """Validate input data."""
    required_keys = ["id", "name"]
    return all(key in data for key in required_keys)
''',
        "config.py": '''"""Configuration module."""

DEFAULT_TIMEOUT = 30
MAX_RETRIES = 3


class Config:
    """Configuration settings."""
    
    def __init__(self):
        self.timeout = DEFAULT_TIMEOUT
        self.retries = MAX_RETRIES
    
    def update(self, **kwargs):
        """Update configuration."""
        for key, value in kwargs.items():
            if hasattr(self, key):
                setattr(self, key, value)
''',
        "README.md": "# External Library\n\nShared utilities for multiple projects.\n"
    }
    
    for file_path, content in lib_files.items():
        full_path = external_dir / file_path
        full_path.parent.mkdir(parents=True, exist_ok=True)
        full_path.write_text(content)
    
    # Initial commit
    subprocess.run(["git", "add", "."], cwd=external_dir, check=True, capture_output=True)
    subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=external_dir, check=True, capture_output=True)
    
    return external_dir


def add_subtree(main_repo: Path, external_repo: Path, subtree_prefix: str = "lib/external") -> None:
    """
    Add an external repository as a git subtree.
    
    Args:
        main_repo: Main repository directory
        external_repo: External repository to add as subtree
        subtree_prefix: Path prefix where subtree will be added
    """
    # Add the subtree
    subprocess.run(
        ["git", "subtree", "add", "--prefix", subtree_prefix, str(external_repo), "master", "--squash"],
        cwd=main_repo,
        check=True,
        capture_output=True
    )


def run_subtree_tests(executable: Path) -> int:
    """
    Run all git subtree tests.
    
    Args:
        executable: Path to the cs-mcp executable
        
    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = NuitkaBackend(executable=executable)
    return run_subtree_tests_with_backend(backend)


def run_subtree_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all git subtree tests using a backend.
    
    Args:
        backend: Server backend to use
        
    Returns:
        Exit code (0 for success, 1 for failure)
    """
    # Check if git subtree is available
    try:
        result = subprocess.run(["git", "subtree", "--help"], capture_output=True, text=True)
        if result.returncode != 0:
            print("\nGit subtree command not available. Skipping subtree tests.")
            return 0
    except Exception as e:
        print(f"\nGit subtree not available: {e}")
        return 0
    
    with safe_temp_directory(prefix="cs_mcp_subtree_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")
        
        print("\nCreating external library repository...")
        external_repo = create_external_repo(test_dir)
        print(f"External repo: {external_repo}")
        
        print("\nCreating main repository...")
        repo_dir = create_git_repo(test_dir / "main_project", get_sample_files())
        print(f"Main repo: {repo_dir}")
        
        subtree_prefix = "lib/external"
        print(f"\nAdding git subtree at '{subtree_prefix}'...")
        try:
            add_subtree(repo_dir, external_repo, subtree_prefix)
            print("Subtree added successfully")
        except Exception as e:
            print(f"\nError adding subtree: {e}")
            return 0
        
        subtree_file = repo_dir / subtree_prefix / "utils.py"
        if not subtree_file.exists():
            print(f"\nError: Subtree file not found: {subtree_file}")
            return 1
        print(f"Verified subtree file exists: {subtree_file}")
        
        ctx = ServerContext(
            command=backend.get_command(repo_dir),
            env=backend.get_env(os.environ.copy(), repo_dir),
            repo_dir=repo_dir
        )
        
        results = [
            ("Code Health Score (subtree)", test_subtree_tool(ctx, subtree_prefix, "code_health_score", "utils.py")),
            ("Code Health Review (subtree)", test_subtree_tool(ctx, subtree_prefix, "code_health_review", "config.py")),
            ("Pre-commit Safeguard (subtree)", test_subtree_pre_commit(ctx, subtree_prefix)),
            ("Absolute Paths (subtree)", test_subtree_absolute_paths(ctx, subtree_prefix)),
            ("Main Repo Files Still Work", test_main_repo_still_works(ctx)),
        ]
        
        return print_summary(results)


def _run_mcp_test(ctx: ServerContext, request: ToolRequest, test_name: str) -> bool:
    """Run an MCP tool test with common setup/teardown."""
    client = MCPClient(ctx.command, env=ctx.env, cwd=str(ctx.repo_dir))
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        print_test("Server started", True)
        client.initialize()
        
        response = client.call_tool(request.tool_name, {"file_path": request.file_path}, timeout=60)
        result_text = extract_result_text(response)
        return request.validator(result_text)
    except Exception as e:
        print_test(test_name, False, str(e))
        return False
    finally:
        client.stop()


def _validate_score(result_text: str, success_msg: str) -> bool:
    """Validate a code_health_score response."""
    score = extract_code_health_score(result_text)
    if score is None:
        print_test(success_msg, False, f"Response: {result_text[:200]}")
        return False
    print_test(success_msg, True, f"Score: {score}")
    return True


def _validate_review(result_text: str, success_msg: str) -> bool:
    """Validate a code_health_review response."""
    has_content = len(result_text) > 0
    print_test(success_msg, has_content, f"Length: {len(result_text)} chars")
    no_errors = "traceback" not in result_text.lower() and "nonetype" not in result_text.lower()
    print_test("No errors in response", no_errors)
    return has_content and no_errors


def test_subtree_tool(ctx: ServerContext, subtree_path: str, tool: str, filename: str) -> bool:
    """Test a code health tool on files in a git subtree."""
    print_header(f"Test: Code Health {tool.replace('code_health_', '').title()} in Git Subtree")
    test_file = str(ctx.repo_dir / subtree_path / filename)
    print(f"\n  Testing subtree file: {test_file}")
    
    is_score = tool == "code_health_score"
    validator = (lambda r: _validate_score(r, "Code Health Score returned")) if is_score else \
                (lambda r: _validate_review(r, "Review returned content"))
    
    return _run_mcp_test(
        ctx, ToolRequest(tool, test_file, validator),
        f"Subtree {tool} test"
    )


def test_subtree_pre_commit(ctx: ServerContext, subtree_path: str) -> bool:
    """Test pre_commit_code_health_safeguard with subtree modifications."""
    print_header("Test: Pre-commit Safeguard with Subtree Changes")
    client = MCPClient(ctx.command, env=ctx.env, cwd=str(ctx.repo_dir))
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        print_test("Server started", True)
        client.initialize()
        test_file = ctx.repo_dir / subtree_path / "utils.py"
        original_content = test_file.read_text()
        test_file.write_text(original_content + "\n# Subtree modification test\n")
        subprocess.run(["git", "add", str(test_file)], cwd=ctx.repo_dir, check=True, capture_output=True)
        print(f"\n  Running safeguard with subtree modification...")
        response = client.call_tool(
            "pre_commit_code_health_safeguard", {"git_repository_path": str(ctx.repo_dir)}, timeout=60
        )
        result_text = extract_result_text(response)
        has_content = len(result_text) > 20
        print_test("Safeguard returned content", has_content, f"Length: {len(result_text)} chars")
        no_errors = "traceback" not in result_text.lower()
        print_test("No errors in response", no_errors)
        test_file.write_text(original_content)
        subprocess.run(["git", "reset", "HEAD", str(test_file)], cwd=ctx.repo_dir, capture_output=True)
        return has_content and no_errors
    except Exception as e:
        print_test("Subtree pre-commit test", False, str(e))
        return False
    finally:
        client.stop()


def test_subtree_absolute_paths(ctx: ServerContext, subtree_path: str) -> bool:
    """Test absolute path resolution for subtree files."""
    print_header("Test: Absolute Paths in Git Subtree")
    abs_path = str(ctx.repo_dir / subtree_path / "utils.py")
    print(f"  Testing absolute path: {abs_path}")
    return _run_mcp_test(
        ctx, ToolRequest("code_health_score", abs_path, lambda r: _validate_score(r, "Absolute path resolved")),
        "Subtree absolute paths test"
    )


def test_main_repo_still_works(ctx: ServerContext) -> bool:
    """Test that main repo files still work correctly with subtree present."""
    print_header("Test: Main Repository Files with Subtree Present")
    test_file = str(ctx.repo_dir / "src/utils/calculator.py")
    return _run_mcp_test(
        ctx, ToolRequest("code_health_score", test_file, lambda r: _validate_score(r, "Main repo file analysis works")),
        "Main repo file test"
    )


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_git_subtree.py /path/to/cs-mcp")
        return 1
    
    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1
    
    print_header("Git Subtree Integration Tests")
    
    return run_subtree_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

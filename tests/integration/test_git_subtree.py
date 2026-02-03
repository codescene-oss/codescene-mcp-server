#!/usr/bin/env python3
"""
Git subtree integration tests.

Tests that the MCP server correctly handles git subtrees,
where external repositories are nested as subdirectories.
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from test_utils import (
    MCPClient,
    create_git_repo,
    create_test_environment,
    extract_code_health_score,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
)
from fixtures import get_sample_files


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


def test_subtree_code_health_score(executable: Path, repo_dir: Path, subtree_path: str) -> bool:
    """Test code_health_score on files in a git subtree."""
    print_header("Test: Code Health Score in Git Subtree")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        # Test file in subtree
        test_file = repo_dir / subtree_path / "utils.py"
        
        print(f"\n  Testing subtree file: {test_file}")
        print(f"  Relative to repo: {test_file.relative_to(repo_dir)}")
        
        response = client.call_tool("code_health_score", {"file_path": str(test_file)}, timeout=60)
        result_text = extract_result_text(response)
        score = extract_code_health_score(result_text)
        
        if score is None:
            print_test("Code Health Score returned", False, f"Response: {result_text[:200]}")
            return False
        
        print_test("Code Health Score returned", True, f"Score: {score}")
        
        # Check for common errors
        no_errors = all(err not in result_text.lower() for err in [
            "nonetype",
            "traceback",
            "error:",
            "failed"
        ])
        print_test("No errors in response", no_errors)
        
        return score is not None and no_errors
        
    except Exception as e:
        print_test("Subtree code health score test", False, str(e))
        return False
    finally:
        client.stop()


def test_subtree_code_health_review(executable: Path, repo_dir: Path, subtree_path: str) -> bool:
    """Test code_health_review on files in a git subtree."""
    print_header("Test: Code Health Review in Git Subtree")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        test_file = repo_dir / subtree_path / "config.py"
        
        print(f"\n  Reviewing subtree file: {test_file}")
        
        response = client.call_tool("code_health_review", {"file_path": str(test_file)}, timeout=60)
        result_text = extract_result_text(response)
        
        # Code health review may return short responses for healthy files
        has_content = len(result_text) > 0
        print_test("Review returned content", has_content, f"Length: {len(result_text)} chars")
        
        # Check for score or health info - healthy files may just return a score
        has_health_info = any(term in result_text.lower() for term in ["health", "score", "10", "9", "8"])
        print_test("Review contains Code Health information", has_health_info)
        
        no_errors = "traceback" not in result_text.lower() and "nonetype" not in result_text.lower()
        print_test("No errors in response", no_errors)
        
        return has_content and no_errors
        
    except Exception as e:
        print_test("Subtree code health review test", False, str(e))
        return False
    finally:
        client.stop()


def test_subtree_pre_commit(executable: Path, repo_dir: Path, subtree_path: str) -> bool:
    """Test pre_commit_code_health_safeguard with subtree modifications."""
    print_header("Test: Pre-commit Safeguard with Subtree Changes")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        # Modify a file in the subtree
        test_file = repo_dir / subtree_path / "utils.py"
        original_content = test_file.read_text()
        test_file.write_text(original_content + "\n# Subtree modification test\n")
        
        # Stage the change
        subprocess.run(["git", "add", str(test_file)], cwd=repo_dir, check=True, capture_output=True)
        
        print(f"\n  Running safeguard with subtree modification...")
        
        response = client.call_tool(
            "pre_commit_code_health_safeguard",
            {"git_repository_path": str(repo_dir)},
            timeout=60
        )
        
        result_text = extract_result_text(response)
        
        has_content = len(result_text) > 20
        print_test("Safeguard returned content", has_content, f"Length: {len(result_text)} chars")
        
        no_errors = "traceback" not in result_text.lower() and "error:" not in result_text.lower()
        print_test("No errors in response", no_errors)
        
        # Reset the file
        test_file.write_text(original_content)
        subprocess.run(["git", "reset", "HEAD", str(test_file)], cwd=repo_dir, capture_output=True)
        
        return has_content and no_errors
        
    except Exception as e:
        print_test("Subtree pre-commit test", False, str(e))
        return False
    finally:
        client.stop()


def run_subtree_code_health_test(
    executable: Path, 
    repo_dir: Path, 
    file_path: str, 
    test_description: str
) -> bool:
    """
    Run a code health score test for subtree files.
    
    Args:
        executable: Path to the cs-mcp executable
        repo_dir: Repository directory
        file_path: Path to the file to analyze
        test_description: Description for test output
        
    Returns:
        True if test passed
    """
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        print(f"\n  Testing: {file_path}")
        
        response = client.call_tool("code_health_score", {"file_path": file_path}, timeout=60)
        result_text = extract_result_text(response)
        score = extract_code_health_score(result_text)
        
        if score is None:
            print_test(test_description, False, f"Response: {result_text[:200]}")
            return False
        
        print_test(test_description, True, f"Score: {score}")
        return True
        
    except Exception as e:
        print_test(test_description, False, str(e))
        return False
    finally:
        client.stop()


def test_subtree_absolute_paths(executable: Path, repo_dir: Path, subtree_path: str) -> bool:
    """Test absolute path resolution for subtree files."""
    print_header("Test: Absolute Paths in Git Subtree")
    
    abs_path = str(repo_dir / subtree_path / "utils.py")
    print(f"  Testing absolute path: {abs_path}")
    
    return run_subtree_code_health_test(
        executable, repo_dir, abs_path, "Absolute path resolved"
    )


def test_main_repo_still_works(executable: Path, repo_dir: Path) -> bool:
    """Test that main repo files still work correctly with subtree present."""
    print_header("Test: Main Repository Files with Subtree Present")
    
    test_file = str(repo_dir / "src/utils/calculator.py")
    
    return run_subtree_code_health_test(
        executable, repo_dir, test_file, "Main repo file analysis works"
    )


def run_subtree_tests(executable: Path) -> int:
    """
    Run all git subtree tests.
    
    Args:
        executable: Path to the cs-mcp executable
        
    Returns:
        Exit code (0 for success, 1 for failure)
    """
    # Check if git subtree is available
    try:
        result = subprocess.run(
            ["git", "subtree", "--help"],
            capture_output=True,
            text=True
        )
        if result.returncode != 0:
            print("\nGit subtree command not available. Skipping subtree tests.")
            print("Note: git-subtree is a contrib command and may need separate installation.")
            return 0
    except Exception as e:
        print(f"\nGit subtree not available: {e}")
        print("Skipping subtree tests.")
        return 0
    
    with tempfile.TemporaryDirectory(prefix="cs_mcp_subtree_test_") as tmp:
        test_dir = Path(tmp)
        print(f"\nTest directory: {test_dir}")
        
        # Create external library repo
        print("\nCreating external library repository...")
        external_repo = create_external_repo(test_dir)
        print(f"External repo: {external_repo}")
        
        # Create main git repo
        print("\nCreating main repository...")
        repo_dir = create_git_repo(test_dir / "main_project", get_sample_files())
        print(f"Main repo: {repo_dir}")
        
        # Add subtree
        subtree_prefix = "lib/external"
        print(f"\nAdding git subtree at '{subtree_prefix}'...")
        try:
            add_subtree(repo_dir, external_repo, subtree_prefix)
            print(f"Subtree added successfully")
        except subprocess.CalledProcessError as e:
            print(f"\nError adding subtree: {e}")
            print(f"Stderr: {e.stderr if hasattr(e, 'stderr') else 'N/A'}")
            print("Git subtree may not be available. Skipping tests.")
            return 0
        
        # Verify subtree files exist
        subtree_file = repo_dir / subtree_prefix / "utils.py"
        if not subtree_file.exists():
            print(f"\nError: Subtree file not found: {subtree_file}")
            return 1
        print(f"Verified subtree file exists: {subtree_file}")
        
        results = [
            ("Code Health Score (subtree)", test_subtree_code_health_score(executable, repo_dir, subtree_prefix)),
            ("Code Health Review (subtree)", test_subtree_code_health_review(executable, repo_dir, subtree_prefix)),
            ("Pre-commit Safeguard (subtree)", test_subtree_pre_commit(executable, repo_dir, subtree_prefix)),
            ("Absolute Paths (subtree)", test_subtree_absolute_paths(executable, repo_dir, subtree_prefix)),
            ("Main Repo Files Still Work", test_main_repo_still_works(executable, repo_dir)),
        ]
        
        return print_summary(results)


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

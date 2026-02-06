#!/usr/bin/env python3
"""
File and git repository utilities for integration tests.

This module provides utilities for creating test environments,
managing git repositories, and safe directory cleanup.
"""

import contextlib
import os
import platform
import shutil
import subprocess
import tempfile
import time
from pathlib import Path


def create_test_environment() -> dict[str, str]:
    """
    Create a clean environment for testing.

    This removes CS_MOUNT_PATH and other variables that might interfere
    with testing static executable behavior.

    Returns:
        Environment dictionary suitable for subprocess execution
    """
    env = os.environ.copy()

    # Remove CS_MOUNT_PATH to test static mode
    env.pop("CS_MOUNT_PATH", None)

    return env


def create_git_repo(base_dir: Path, sample_files: dict[str, str]) -> Path:
    """
    Create a temporary git repository with sample files.

    Args:
        base_dir: Base directory where repo will be created
        sample_files: Dictionary mapping file paths to content

    Returns:
        Path to the created git repository
    """
    repo_dir = base_dir / "test_repo"
    repo_dir.mkdir(parents=True, exist_ok=True)

    # Initialize git repo
    subprocess.run(["git", "init", "-b", "master"], cwd=repo_dir, check=True, capture_output=True)
    subprocess.run(
        ["git", "config", "user.name", "Test User"],
        cwd=repo_dir,
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["git", "config", "user.email", "test@example.com"],
        cwd=repo_dir,
        check=True,
        capture_output=True,
    )

    # Create sample files
    for file_path, content in sample_files.items():
        full_path = repo_dir / file_path
        full_path.parent.mkdir(parents=True, exist_ok=True)
        full_path.write_text(content)

    # Initial commit
    subprocess.run(["git", "add", "."], cwd=repo_dir, check=True, capture_output=True)
    subprocess.run(
        ["git", "commit", "-m", "Initial commit"],
        cwd=repo_dir,
        check=True,
        capture_output=True,
    )

    return repo_dir


def _is_windows() -> bool:
    """Check if running on Windows."""
    return os.name == "nt" or platform.system() == "Windows"


def cleanup_dir(path: Path, retries: int = 5, delay: float = 0.5) -> None:
    """
    Safely remove a directory with retry logic for Windows.

    On Windows, file handles may be kept open by Git or other processes,
    causing PermissionError. This function retries with increasing delays.

    Args:
        path: Directory to remove
        retries: Number of retry attempts (default 5)
        delay: Initial delay between retries in seconds (default 0.5)
    """
    if not path.exists():
        return

    for attempt in range(retries):
        try:
            shutil.rmtree(path)
            return
        except PermissionError as e:
            if attempt < retries - 1:
                wait_time = delay * (attempt + 1)
                time.sleep(wait_time)
            else:
                print(f"  Warning: Failed to cleanup {path} after {retries} attempts: {e}")
        except Exception as e:
            print(f"  Warning: Failed to cleanup {path}: {e}")
            return


@contextlib.contextmanager
def safe_temp_directory(prefix: str = "cs_mcp_test_"):
    """
    Context manager for temporary directories with robust cleanup on Windows.

    Unlike tempfile.TemporaryDirectory, this handles Windows file locking
    by retrying cleanup with delays if files are still in use.

    Args:
        prefix: Prefix for the temporary directory name

    Yields:
        Path to the temporary directory
    """
    tmp_dir = Path(tempfile.mkdtemp(prefix=prefix)).resolve()
    try:
        yield tmp_dir
    finally:
        cleanup_dir(tmp_dir)

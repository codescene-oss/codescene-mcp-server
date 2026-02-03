#!/usr/bin/env python3
"""
Platform-specific integration tests.

Tests platform-specific behaviors:
- Windows path handling (C:\\ paths)
- Linux/Unix path handling
- Cross-platform CLI invocation
- Path resolution in different environments
"""

import os
import platform
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from test_utils import (
    MCPClient,
    create_git_repo,
    create_test_environment,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
)
from fixtures import get_sample_files


def is_windows() -> bool:
    """Check if running on Windows."""
    return platform.system() == "Windows"


def run_code_health_test(
    executable: Path, 
    cwd: Path, 
    file_path: str, 
    test_name: str
) -> tuple[bool, str]:
    """
    Run a code health score test and return the result.
    
    Args:
        executable: Path to the cs-mcp executable
        cwd: Working directory for the client
        file_path: Path to the file to analyze
        test_name: Name of the test for logging
        
    Returns:
        Tuple of (success, response_text)
    """
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(cwd))
    
    try:
        if not client.start():
            return False, "Server failed to start"
        
        client.initialize()
        response = client.call_tool("code_health_score", {"file_path": file_path}, timeout=60)
        result_text = extract_result_text(response)
        return True, result_text
    finally:
        client.stop()


def validate_test_result(result_text: str, check_errors: bool = True) -> tuple[bool, bool]:
    """
    Validate a code health test result.
    
    Args:
        result_text: Response text from the tool
        check_errors: Whether to check for 'error' keyword
        
    Returns:
        Tuple of (has_content, no_issues)
    """
    has_content = len(result_text) > 0
    has_score = "score" in result_text.lower()
    
    if check_errors:
        no_issues = "error" not in result_text.lower() or has_score
    else:
        no_issues = "Traceback" not in result_text
    
    return has_content, no_issues


@dataclass
class PathTestConfig:
    """Configuration for a path test."""
    file_path: str
    test_name: str
    header: str
    check_errors: bool = True


@dataclass 
class SpecialPathTestConfig:
    """Configuration for a special path test (spaces, unicode, etc.)."""
    dir_name: str
    file_name: str
    content: str
    test_name: str
    header: str


def run_path_test(executable: Path, cwd: Path, config: PathTestConfig) -> bool:
    """
    Run a path-based code health test with standardized output.
    
    Args:
        executable: Path to the cs-mcp executable
        cwd: Working directory
        config: Test configuration
        
    Returns:
        True if test passed
    """
    print_header(config.header)
    print(f"\n  Testing: {config.file_path}")
    
    success, result_text = run_code_health_test(executable, cwd, config.file_path, config.test_name)
    
    if not success:
        print_test("Server started", False)
        return False
    
    print_test("Server started", True)
    has_content, no_issues = validate_test_result(result_text, config.check_errors)
    passed = has_content and no_issues
    
    print_test(f"Tool handles {config.test_name}", passed, f"Response: {result_text[:150]}")
    return passed


def test_absolute_paths(executable: Path, repo_dir: Path) -> bool:
    """Test that absolute paths work correctly on current platform."""
    abs_path = str((repo_dir / "src/utils/calculator.py").resolve())
    config = PathTestConfig(
        file_path=abs_path,
        test_name="absolute path",
        header=f"Test: Absolute Paths ({platform.system()})"
    )
    return run_path_test(executable, repo_dir, config)


def test_relative_paths(executable: Path, repo_dir: Path) -> bool:
    """Test that relative paths work correctly."""
    print(f"  Working directory: {repo_dir}")
    config = PathTestConfig(
        file_path="src/utils/calculator.py",
        test_name="relative path",
        header="Test: Relative Paths"
    )
    return run_path_test(executable, repo_dir, config)


def test_symlinks(executable: Path, test_dir: Path) -> bool:
    """Test that symlinks are handled correctly (Unix-like systems only)."""
    if is_windows():
        print_header("Test: Symlinks (Skipped on Windows)")
        print_test("Symlink test skipped on Windows", True, "N/A on Windows")
        return True
    
    print_header("Test: Symlinks")
    
    try:
        # Create a test file
        original_file = test_dir / "original.py"
        original_file.write_text("def test():\n    return 42\n")
        
        # Create symlink
        symlink_file = test_dir / "symlink.py"
        symlink_file.symlink_to(original_file)
        
        env = create_test_environment()
        client = MCPClient([str(executable)], env=env, cwd=str(test_dir))
        
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        print(f"\n  Testing symlink: {symlink_file} -> {original_file}")
        
        response = client.call_tool("code_health_score", {"file_path": str(symlink_file)}, timeout=60)
        result_text = extract_result_text(response)
        
        has_content = len(result_text) > 0
        no_crash = "Traceback" not in result_text
        
        print_test("Tool handles symlink", has_content and no_crash, f"Response: {result_text[:150]}")
        
        return has_content and no_crash
        
    except Exception as e:
        print_test("Symlink test", False, str(e))
        return False
    finally:
        if 'client' in locals():
            client.stop()


def create_special_path_test(test_dir: Path, config: SpecialPathTestConfig) -> Path:
    """Create a test file in a special path (spaces, unicode, etc.)."""
    special_dir = test_dir / config.dir_name
    special_dir.mkdir(exist_ok=True)
    special_file = special_dir / config.file_name
    special_file.write_text(config.content, encoding='utf-8')
    return special_file


def run_special_path_test(executable: Path, test_dir: Path, config: SpecialPathTestConfig) -> bool:
    """Run a test for special paths (spaces, unicode)."""
    try:
        special_file = create_special_path_test(test_dir, config)
        path_config = PathTestConfig(
            file_path=str(special_file),
            test_name=config.test_name,
            header=config.header,
            check_errors=False
        )
        return run_path_test(executable, test_dir, path_config)
    except Exception as e:
        print_test(f"{config.test_name} test", False, str(e))
        return False


SPACES_PATH_CONFIG = SpecialPathTestConfig(
    dir_name="directory with spaces",
    file_name="file with spaces.py",
    content="def function_with_spaces():\n    return 'test'\n",
    test_name="spaces in path",
    header="Test: Spaces in Paths"
)

UNICODE_PATH_CONFIG = SpecialPathTestConfig(
    dir_name="tëst_ディレクトリ",
    file_name="fîlé_ファイル.py",
    content="def unicode_function():\n    return 'тест'\n",
    test_name="Unicode in path",
    header="Test: Unicode in Paths"
)


def test_spaces_in_paths(executable: Path, test_dir: Path) -> bool:
    """Test that paths with spaces are handled correctly."""
    return run_special_path_test(executable, test_dir, SPACES_PATH_CONFIG)


def test_unicode_in_paths(executable: Path, test_dir: Path) -> bool:
    """Test that Unicode characters in paths are handled correctly."""
    return run_special_path_test(executable, test_dir, UNICODE_PATH_CONFIG)


def run_platform_tests(executable: Path) -> int:
    """
    Run all platform-specific tests.
    
    Args:
        executable: Path to the cs-mcp executable
        
    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with tempfile.TemporaryDirectory(prefix="cs_mcp_platform_test_") as tmp:
        test_dir = Path(tmp)
        print(f"\nTest directory: {test_dir}")
        print(f"Platform: {platform.system()} {platform.release()}")
        
        # Create git repo with sample files
        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        
        results = [
            ("Absolute Paths", test_absolute_paths(executable, repo_dir)),
            ("Relative Paths", test_relative_paths(executable, repo_dir)),
            ("Symlinks", test_symlinks(executable, test_dir)),
            ("Spaces in Paths", test_spaces_in_paths(executable, test_dir)),
            ("Unicode in Paths", test_unicode_in_paths(executable, test_dir)),
        ]
        
        return print_summary(results)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_platform_specific.py /path/to/cs-mcp")
        return 1
    
    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1
    
    print_header(f"Platform-Specific Integration Tests ({platform.system()})")
    
    return run_platform_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

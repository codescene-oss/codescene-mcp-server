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


def test_absolute_paths(executable: Path, repo_dir: Path) -> bool:
    """Test that absolute paths work correctly on current platform."""
    print_header(f"Test: Absolute Paths ({platform.system()})")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        # Test with absolute path
        test_file = repo_dir / "src/utils/calculator.py"
        abs_path = test_file.resolve()
        
        print(f"\n  Testing absolute path: {abs_path}")
        
        response = client.call_tool("code_health_score", {"file_path": str(abs_path)}, timeout=60)
        result_text = extract_result_text(response)
        
        has_content = len(result_text) > 0
        no_path_errors = "path" not in result_text.lower() or "score" in result_text.lower()
        
        print_test("Tool handles absolute path", has_content and no_path_errors, f"Response: {result_text[:150]}")
        
        return has_content and no_path_errors
        
    finally:
        client.stop()


def test_relative_paths(executable: Path, repo_dir: Path) -> bool:
    """Test that relative paths work correctly."""
    print_header("Test: Relative Paths")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        # Test with relative path
        rel_path = "src/utils/calculator.py"
        
        print(f"\n  Testing relative path: {rel_path}")
        print(f"  Working directory: {repo_dir}")
        
        response = client.call_tool("code_health_score", {"file_path": rel_path}, timeout=60)
        result_text = extract_result_text(response)
        
        has_content = len(result_text) > 0
        no_errors = "error" not in result_text.lower() or "score" in result_text.lower()
        
        print_test("Tool handles relative path", has_content and no_errors, f"Response: {result_text[:150]}")
        
        return has_content and no_errors
        
    finally:
        client.stop()


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


def test_spaces_in_paths(executable: Path, test_dir: Path) -> bool:
    """Test that paths with spaces are handled correctly."""
    print_header("Test: Spaces in Paths")
    
    try:
        # Create directory and file with spaces
        space_dir = test_dir / "directory with spaces"
        space_dir.mkdir(exist_ok=True)
        space_file = space_dir / "file with spaces.py"
        space_file.write_text("def function_with_spaces():\n    return 'test'\n")
        
        env = create_test_environment()
        client = MCPClient([str(executable)], env=env, cwd=str(test_dir))
        
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        print(f"\n  Testing path with spaces: {space_file}")
        
        response = client.call_tool("code_health_score", {"file_path": str(space_file)}, timeout=60)
        result_text = extract_result_text(response)
        
        has_content = len(result_text) > 0
        no_crash = "Traceback" not in result_text
        
        print_test("Tool handles spaces in path", has_content and no_crash, f"Response: {result_text[:150]}")
        
        return has_content and no_crash
        
    except Exception as e:
        print_test("Spaces in path test", False, str(e))
        return False
    finally:
        if 'client' in locals():
            client.stop()


def test_unicode_in_paths(executable: Path, test_dir: Path) -> bool:
    """Test that Unicode characters in paths are handled correctly."""
    print_header("Test: Unicode in Paths")
    
    try:
        # Create directory and file with Unicode characters
        unicode_dir = test_dir / "tëst_ディレクトリ"
        unicode_dir.mkdir(exist_ok=True)
        unicode_file = unicode_dir / "fîlé_ファイル.py"
        unicode_file.write_text("def unicode_function():\n    return 'тест'\n", encoding='utf-8')
        
        env = create_test_environment()
        client = MCPClient([str(executable)], env=env, cwd=str(test_dir))
        
        if not client.start():
            print_test("Server started", False)
            return False
        
        print_test("Server started", True)
        client.initialize()
        
        print(f"\n  Testing Unicode path: {unicode_file}")
        
        response = client.call_tool("code_health_score", {"file_path": str(unicode_file)}, timeout=60)
        result_text = extract_result_text(response)
        
        has_content = len(result_text) > 0
        no_crash = "Traceback" not in result_text and "UnicodeDecodeError" not in result_text
        
        print_test("Tool handles Unicode in path", has_content and no_crash, f"Response: {result_text[:150]}")
        
        return has_content and no_crash
        
    except Exception as e:
        print_test("Unicode in path test", False, str(e))
        return False
    finally:
        if 'client' in locals():
            client.stop()


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

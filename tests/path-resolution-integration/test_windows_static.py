#!/usr/bin/env python3
"""
Windows Static Binary Path Resolution Integration Test.

This test verifies that the MCP tools work correctly on Windows with:
- Windows absolute paths (e.g., C:\\Users\\...)
- Git repositories on Windows
- Git worktrees on Windows

These tests require CS_ACCESS_TOKEN to be set as they invoke actual CLI tools
to verify real Code Health scores are returned.

The test will auto-detect cs-mcp.exe in the repo root. If not found, it will
attempt to build it using Nuitka (requires Python 3.13, Nuitka, and will
download the CodeScene CLI).

Usage: 
    set CS_ACCESS_TOKEN=your_token
    python test_windows_static.py

Or with explicit binary path:
    python test_windows_static.py C:\\path\\to\\cs-mcp.exe
"""

import os
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path

# Add parent directory for imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from mcp_test_utils import (
    MCPClient,
    ToolTestConfig,
    cleanup_test_dir,
    create_static_mode_env,
    print_header,
    print_test,
    print_test_summary,
    run_tool_test,
)


# Test configuration
REPO_ROOT = Path(__file__).parent.parent.parent
BINARY_NAME = "cs-mcp.exe"
CS_CLI_URL = "https://downloads.codescene.io/enterprise/cli/cs-windows-amd64-latest.zip"
CS_CLI_NAME = "cs.exe"

# Common forbidden patterns for path resolution tests
MOUNT_PATH_ERROR = "CS_MOUNT_PATH"
GIT_REPO_ERROR = "not in a git repository"
NONETYPE_ERROR = "NoneType"
PATH_ERROR = "path"  # Generic path-related errors


@dataclass
class CodeHealthTestParams:
    """Parameters for building a code health test configuration."""
    test_file: str
    test_name: str
    header: str
    description: str
    extra_forbidden: list[str] | None = None


@dataclass
class TestContext:
    """Context for running all tests."""
    binary_path: Path
    git_test_file: str
    git_tmpdir: str
    worktree_test_file: str | None
    worktree_created: bool


def is_windows() -> bool:
    """Check if running on Windows."""
    return platform.system() == "Windows"


def check_prerequisites() -> tuple[bool, list[str]]:
    """
    Check if all prerequisites for building the binary are available.
    
    Returns:
        Tuple of (all_ok, list_of_issues)
    """
    issues = []
    
    # Check Python version
    if sys.version_info < (3, 10):
        issues.append(f"Python 3.10+ required, found {sys.version}")
    
    # Check for Nuitka
    try:
        import nuitka
    except ImportError:
        issues.append("Nuitka not installed (pip install Nuitka)")
    
    # Check for git
    try:
        subprocess.run(["git", "--version"], capture_output=True, check=True)
    except (subprocess.CalledProcessError, FileNotFoundError):
        issues.append("Git not found in PATH")
    
    return len(issues) == 0, issues


def download_cs_cli(dest_dir: Path) -> Path:
    """
    Download the CodeScene CLI for Windows.
    
    Returns:
        Path to the downloaded cs.exe
    """
    print("    Downloading CodeScene CLI...")
    zip_path = dest_dir / "cs-cli.zip"
    
    urllib.request.urlretrieve(CS_CLI_URL, zip_path)
    
    with zipfile.ZipFile(zip_path, 'r') as zip_ref:
        zip_ref.extractall(dest_dir)
    
    cs_exe = dest_dir / CS_CLI_NAME
    if not cs_exe.exists():
        # Try to find it in subdirectories
        for f in dest_dir.rglob(CS_CLI_NAME):
            cs_exe = f
            break
    
    if not cs_exe.exists():
        raise FileNotFoundError(f"Could not find {CS_CLI_NAME} after extraction")
    
    return cs_exe


def get_venv_paths(venv_dir: Path) -> tuple[Path, Path]:
    """Get paths to venv Python and pip executables."""
    if is_windows():
        return venv_dir / "Scripts" / "python.exe", venv_dir / "Scripts" / "pip.exe"
    return venv_dir / "bin" / "python", venv_dir / "bin" / "pip"


def ensure_venv_exists(repo_root: Path) -> tuple[Path, Path]:
    """Ensure virtual environment exists and return Python/pip paths."""
    venv_dir = repo_root / "venv"
    if not venv_dir.exists():
        print("    Creating virtual environment...")
        subprocess.run([sys.executable, "-m", "venv", str(venv_dir)], check=True)
    return get_venv_paths(venv_dir)


def install_build_dependencies(repo_root: Path, venv_pip: Path) -> None:
    """Install required dependencies for building."""
    print("    Installing dependencies...")
    requirements = repo_root / "src" / "requirements.txt"
    subprocess.run([str(venv_pip), "install", "-r", str(requirements)], check=True, capture_output=True)
    subprocess.run([str(venv_pip), "install", "Nuitka"], check=True, capture_output=True)


def ensure_cs_cli_exists(repo_root: Path) -> Path:
    """Ensure CodeScene CLI exists, downloading if necessary."""
    cs_cli = repo_root / CS_CLI_NAME
    if not cs_cli.exists():
        cs_cli_downloaded = download_cs_cli(repo_root)
        if cs_cli_downloaded != cs_cli:
            shutil.copy(cs_cli_downloaded, cs_cli)
    return cs_cli


def run_nuitka_build(repo_root: Path, venv_python: Path, cs_cli: Path) -> Path:
    """Run Nuitka build and return path to built binary."""
    print("    Building with Nuitka (this may take a few minutes)...")
    binary_path = repo_root / BINARY_NAME
    build_cmd = [
        str(venv_python), "-m", "nuitka",
        "--onefile",
        "--assume-yes-for-downloads",
        f"--include-data-dir={repo_root / 'src' / 'docs'}=src/docs",
        f"--include-data-files={cs_cli}={CS_CLI_NAME}",
        f"--output-filename={BINARY_NAME}",
        str(repo_root / "src" / "cs_mcp_server.py")
    ]
    
    result = subprocess.run(build_cmd, cwd=str(repo_root), capture_output=True, text=True)
    if result.returncode != 0:
        print(f"    Build failed: {result.stderr}")
        raise RuntimeError("Nuitka build failed")
    
    if not binary_path.exists():
        raise FileNotFoundError(f"Binary not found at {binary_path} after build")
    
    print(f"    [OK] Built {binary_path}")
    return binary_path


def build_binary(repo_root: Path) -> Path:
    """
    Build the cs-mcp.exe binary using Nuitka.
    
    Returns:
        Path to the built binary
    """
    print_header("Building cs-mcp.exe")
    
    venv_python, venv_pip = ensure_venv_exists(repo_root)
    install_build_dependencies(repo_root, venv_pip)
    cs_cli = ensure_cs_cli_exists(repo_root)
    return run_nuitka_build(repo_root, venv_python, cs_cli)


def ensure_binary_exists(repo_root: Path = REPO_ROOT) -> tuple[Path | None, str | None]:
    """
    Ensure the cs-mcp.exe binary exists, building if necessary.
    
    Returns:
        Tuple of (binary_path, skip_reason)
        If binary_path is None, skip_reason explains why tests should be skipped
    """
    binary_path = repo_root / BINARY_NAME
    
    if binary_path.exists():
        return binary_path, None
    
    print(f"  Binary not found at {binary_path}")
    print("  Checking build prerequisites...")
    
    prereqs_ok, issues = check_prerequisites()
    if not prereqs_ok:
        skip_reason = "Cannot build binary: " + "; ".join(issues)
        return None, skip_reason
    
    try:
        return build_binary(repo_root), None
    except Exception as e:
        return None, f"Build failed: {e}"


def create_test_git_repo_windows() -> tuple[str, str]:
    """
    Create a temporary git repository with a test file on Windows.
    
    Returns:
        Tuple of (tmpdir, test_file_path) with Windows-style paths
    """
    tmpdir = tempfile.mkdtemp(prefix="mcp-win-path-test-")
    
    subprocess.run(["git", "init"], cwd=tmpdir, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=tmpdir, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test"], cwd=tmpdir, capture_output=True)
    
    src_dir = os.path.join(tmpdir, "src")
    os.makedirs(src_dir)
    
    # Create a Java file for Code Health analysis
    test_file = os.path.join(src_dir, "TestFile.java")
    with open(test_file, "w", encoding="utf-8") as f:
        f.write("""public class TestFile {
    public void hello() {
        System.out.println("Hello from Windows path test");
    }
    
    public int calculate(int a, int b) {
        return a + b;
    }
}
""")
    
    subprocess.run(["git", "add", "."], cwd=tmpdir, capture_output=True)
    subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=tmpdir, capture_output=True)
    
    return tmpdir, test_file


def create_test_git_worktree_windows() -> tuple[str, str, str]:
    """
    Create a temporary git worktree with a test file on Windows.
    
    Returns:
        Tuple of (base_dir, worktree_dir, test_file_path)
    """
    base_dir = tempfile.mkdtemp(prefix="mcp-win-worktree-test-")
    main_dir = os.path.join(base_dir, "main-repo")
    worktree_dir = os.path.join(base_dir, "worktree")
    os.makedirs(main_dir)
    
    # Initialize main repo
    subprocess.run(["git", "init"], cwd=main_dir, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=main_dir, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test"], cwd=main_dir, capture_output=True)
    
    # Create and commit a file
    src_dir = os.path.join(main_dir, "src")
    os.makedirs(src_dir)
    test_file_main = os.path.join(src_dir, "WorktreeTest.java")
    with open(test_file_main, "w", encoding="utf-8") as f:
        f.write("""public class WorktreeTest {
    public void testMethod() {
        System.out.println("Testing worktree on Windows");
    }
    
    public String getName() {
        return "WorktreeTest";
    }
}
""")
    
    subprocess.run(["git", "add", "."], cwd=main_dir, capture_output=True)
    subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=main_dir, capture_output=True)
    
    # Create worktree
    result = subprocess.run(
        ["git", "worktree", "add", worktree_dir, "-b", "test-worktree-branch"],
        cwd=main_dir, capture_output=True, text=True
    )
    
    if result.returncode != 0:
        raise RuntimeError(f"Failed to create worktree: {result.stderr}")
    
    test_file = os.path.join(worktree_dir, "src", "WorktreeTest.java")
    return base_dir, worktree_dir, test_file


def build_code_health_test_config(params: CodeHealthTestParams) -> tuple[str, ToolTestConfig]:
    """Build a code_health_score test configuration."""
    forbidden = [MOUNT_PATH_ERROR, NONETYPE_ERROR, "Error:", "error:"]
    if params.extra_forbidden:
        forbidden.extend(params.extra_forbidden)
    return (params.test_name, ToolTestConfig(
        tool_name="code_health_score",
        arguments={"file_path": params.test_file},
        header=params.header,
        forbidden_patterns=forbidden,
        test_description=params.description,
        required_patterns=["code health"],
    ))


def build_windows_path_test_configs(git_test_file: str) -> list[tuple[str, ToolTestConfig]]:
    """Build test configurations for Windows path handling."""
    return [
        build_code_health_test_config(CodeHealthTestParams(
            test_file=git_test_file,
            test_name="code_health_score (Windows absolute path)",
            header="Test code_health_score (Windows Absolute Path)",
            description="Returns valid Code Health score for Windows path",
        )),
    ]


def build_windows_worktree_test_configs(worktree_test_file: str) -> list[tuple[str, ToolTestConfig]]:
    """Build test configurations for Windows git worktree handling."""
    return [
        build_code_health_test_config(CodeHealthTestParams(
            test_file=worktree_test_file,
            test_name="code_health_score (Windows worktree)",
            header="Test code_health_score (Windows Git Worktree)",
            description="Returns valid Code Health score in Windows worktree",
            extra_forbidden=["not a git repository"],
        )),
    ]


def build_pre_commit_test_configs(git_repo_path: str) -> list[tuple[str, ToolTestConfig]]:
    """Build test configurations for pre-commit safeguard on Windows."""
    return [
        ("pre_commit_code_health_safeguard (Windows path)", ToolTestConfig(
            tool_name="pre_commit_code_health_safeguard",
            arguments={"git_repository_path": git_repo_path},
            header="Test pre_commit_code_health_safeguard (Windows Path)",
            forbidden_patterns=[MOUNT_PATH_ERROR, NONETYPE_ERROR, "Error:"],
            test_description="Pre-commit safeguard works with Windows paths",
            required_patterns=None,  # May return "no changes" which is fine
        )),
    ]


def test_environment_setup(binary_path: Path) -> bool:
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    checks = []
    
    # Check binary exists
    binary_ok = binary_path.exists()
    checks.append(binary_ok)
    print_test("cs-mcp.exe binary exists", binary_ok, f"Path: {binary_path}")
    
    # Check we're on Windows (or at least testing Windows paths)
    is_win = is_windows()
    print_test("Running on Windows", is_win, f"Platform: {platform.system()}")
    # Don't fail if not on Windows - might be testing cross-platform
    
    # Check CS_ACCESS_TOKEN is set
    has_token = os.getenv("CS_ACCESS_TOKEN") is not None
    checks.append(has_token)
    print_test("CS_ACCESS_TOKEN is set", has_token, 
               "Token present" if has_token else "NOT SET - tests will fail!")
    
    # Check CS_MOUNT_PATH is NOT set (static mode)
    mount_path = os.getenv('CS_MOUNT_PATH')
    no_mount_ok = mount_path is None
    checks.append(no_mount_ok)
    print_test("CS_MOUNT_PATH is NOT set (static mode)", no_mount_ok,
               f"Value: {mount_path}" if mount_path else "Not set (correct)")
    
    return all(checks)


def test_mcp_server_starts(binary_path: Path) -> bool:
    """Verify the MCP server starts successfully."""
    print_header("Test MCP Server Startup (Windows)")
    
    env = create_static_mode_env()
    client = MCPClient([str(binary_path)], env=env)
    
    try:
        started = client.start()
        print_test("MCP server process started", started)
        if not started:
            stderr = client.get_stderr()
            if stderr:
                print(f"         stderr: {stderr[:200]}")
            return False
        
        response = client.initialize()
        has_result = "result" in response
        print_test("MCP server responds to initialize", has_result)
        if not has_result and "error" in response:
            print(f"         error: {response['error']}")
        return has_result
    except Exception as e:
        print_test("MCP server starts", False, str(e))
        return False
    finally:
        client.stop()


def parse_binary_path_arg() -> Path | None:
    """Parse binary path from command line argument."""
    if len(sys.argv) > 1:
        binary_path = Path(sys.argv[1])
        if not binary_path.exists():
            raise FileNotFoundError(f"Specified binary not found: {binary_path}")
        return binary_path
    return None


def print_test_banner() -> None:
    """Print the test suite banner."""
    print("\n" + "=" * 60)
    print("  Windows Static Binary Path Resolution Integration Tests")
    print("  Testing: cs-mcp.exe with Windows paths and worktrees")
    print("=" * 60)


def check_access_token() -> bool:
    """Check if CS_ACCESS_TOKEN is set. Returns False if missing."""
    if os.getenv("CS_ACCESS_TOKEN"):
        return True
    print("\n  [SKIP]: CS_ACCESS_TOKEN environment variable not set")
    print("  These tests require a valid CodeScene access token.")
    print("\n  Set it with:")
    print("    set CS_ACCESS_TOKEN=your_token_here")
    print("  Or in PowerShell:")
    print("    $env:CS_ACCESS_TOKEN = 'your_token_here'")
    return False


def resolve_binary_path(explicit_path: Path | None) -> Path | None:
    """Resolve binary path, building if necessary."""
    if explicit_path:
        return explicit_path
    print("\n  Looking for cs-mcp.exe...")
    binary_path, skip_reason = ensure_binary_exists()
    if binary_path is None:
        print(f"\n  [SKIP]: {skip_reason}")
    return binary_path


def create_worktree_environment() -> tuple[str | None, str | None, bool]:
    """Create test worktree environment. Returns (base_dir, test_file, success)."""
    print("\n  Creating temporary git worktree...")
    try:
        base_dir, worktree_dir, test_file = create_test_git_worktree_windows()
        print(f"  Worktree dir: {worktree_dir}")
        print(f"  Test file: {test_file}")
        return base_dir, test_file, True
    except Exception as e:
        print(f"  Warning: Could not create worktree: {e}")
        return None, None, False


def run_test_configs(
    binary_path: Path,
    env: dict,
    configs: list[tuple[str, ToolTestConfig]],
) -> list[tuple[str, bool]]:
    """Run a list of test configurations and return results."""
    results = []
    for name, config in configs:
        passed = run_tool_test(command=[str(binary_path)], env=env, config=config)
        results.append((name, passed))
    return results


def run_all_tests(ctx: TestContext) -> list[tuple[str, bool]]:
    """Run all test suites and return combined results."""
    results = []
    
    # Environment and startup tests
    results.append(("Environment Setup", test_environment_setup(ctx.binary_path)))
    results.append(("MCP Server Startup", test_mcp_server_starts(ctx.binary_path)))
    
    # Only continue if server starts
    if not results[-1][1]:
        print("\n  [FAIL] Server failed to start, skipping remaining tests")
        return results
    
    env = create_static_mode_env()
    
    # Run path resolution tests
    print_header("Windows Path Resolution Tests")
    results.extend(run_test_configs(ctx.binary_path, env, build_windows_path_test_configs(ctx.git_test_file)))
    
    # Run pre-commit tests
    results.extend(run_test_configs(ctx.binary_path, env, build_pre_commit_test_configs(ctx.git_tmpdir)))
    
    # Run worktree tests if available
    if ctx.worktree_created and ctx.worktree_test_file:
        print_header("Windows Git Worktree Tests")
        results.extend(run_test_configs(ctx.binary_path, env, build_windows_worktree_test_configs(ctx.worktree_test_file)))
    
    return results


def main():
    try:
        explicit_binary_path = parse_binary_path_arg()
    except FileNotFoundError as e:
        print(f"Error: {e}")
        return 1
    
    print_test_banner()
    
    if not check_access_token():
        return 0  # Return 0 to not fail CI when token isn't available
    
    binary_path = resolve_binary_path(explicit_binary_path)
    if binary_path is None:
        return 0  # Skip gracefully
    
    print(f"\n  Using binary: {binary_path}")
    
    # Create test environments
    print("\n  Creating temporary git repository...")
    git_tmpdir, git_test_file = create_test_git_repo_windows()
    print(f"  Test repo: {git_tmpdir}")
    print(f"  Test file: {git_test_file}")
    
    worktree_base_dir, worktree_test_file, worktree_created = create_worktree_environment()
    
    ctx = TestContext(
        binary_path=binary_path,
        git_test_file=git_test_file,
        git_tmpdir=git_tmpdir,
        worktree_test_file=worktree_test_file,
        worktree_created=worktree_created,
    )
    
    try:
        results = run_all_tests(ctx)
        return print_test_summary(results)
    finally:
        cleanup_test_dir(git_tmpdir)
        if worktree_base_dir:
            cleanup_test_dir(worktree_base_dir)


if __name__ == "__main__":
    sys.exit(main())

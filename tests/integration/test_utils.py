#!/usr/bin/env python3
"""
Shared utilities for comprehensive MCP integration tests.

This module provides:
- Build utilities for creating static executables in isolated test environments
- MCPClient for communicating with MCP servers via stdio
- Test helpers for validating Code Health and other tool responses
- Fixture management for test code samples
"""

import json
import os
import platform
import queue
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Optional


# MCP Protocol message IDs
_msg_id = 0


def next_msg_id() -> int:
    """Get the next message ID for MCP requests."""
    global _msg_id
    _msg_id += 1
    return _msg_id


def print_header(msg: str) -> None:
    """Print a formatted test section header."""
    print(f"\n{'='*70}")
    print(f"  {msg}")
    print(f"{'='*70}\n")


def print_test(name: str, passed: bool, details: str = "") -> None:
    """Print a test result."""
    status = "\u2713 PASS" if passed else "\u2717 FAIL"
    color = "\033[92m" if passed else "\033[91m"
    reset = "\033[0m"
    print(f"  {color}{status}{reset}: {name}")
    if details:
        for line in details.split('\n')[:10]:
            print(f"         {line}")


def print_summary(results: list[tuple[str, bool]]) -> int:
    """
    Print test summary and return exit code.
    
    Args:
        results: List of (test_name, passed) tuples
        
    Returns:
        0 if all tests passed, 1 otherwise
    """
    print_header("Test Summary")
    
    passed = [name for name, p in results if p]
    failed = [name for name, p in results if not p]
    
    print(f"  Total: {len(results)} tests")
    print(f"  \033[92mPassed: {len(passed)}\033[0m")
    if failed:
        print(f"  \033[91mFailed: {len(failed)}\033[0m")
        print("\n  Failed tests:")
        for name in failed:
            print(f"    - {name}")
    
    return 0 if len(failed) == 0 else 1


class MCPClient:
    """MCP client that communicates with the server via stdio."""
    
    def __init__(self, command: list[str], env: Optional[dict[str, str]] = None, cwd: Optional[str] = None):
        """
        Initialize MCP client.
        
        Args:
            command: Command to start the MCP server
            env: Environment variables (defaults to current environment)
            cwd: Working directory for the process
        """
        self.command = command
        self.env = env or os.environ.copy()
        self.cwd = cwd
        self.process: Optional[subprocess.Popen] = None
        self.response_queue: queue.Queue = queue.Queue()
        self.reader_thread: Optional[threading.Thread] = None
        self.stderr_lines: list[str] = []
        
    def start(self) -> bool:
        """
        Start the MCP server process.
        
        Returns:
            True if server started successfully
        """
        try:
            self.process = subprocess.Popen(
                self.command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=self.env,
                cwd=self.cwd,
                text=True,
                encoding="utf-8",
                bufsize=1
            )
            self.reader_thread = threading.Thread(target=self._read_responses, daemon=True)
            self.reader_thread.start()
            self.stderr_thread = threading.Thread(target=self._read_stderr, daemon=True)
            self.stderr_thread.start()
            time.sleep(1)
            return self.process.poll() is None
        except Exception as e:
            print(f"Failed to start MCP server: {e}")
            return False
    
    def _read_responses(self) -> None:
        """Read responses from the server in a background thread."""
        try:
            while self.process and self.process.poll() is None:
                line = self.process.stdout.readline()
                if line:
                    self.response_queue.put(line.strip())
        except Exception as e:
            self.response_queue.put(f"ERROR: {e}")
    
    def _read_stderr(self) -> None:
        """Read stderr from the server in a background thread."""
        try:
            while self.process and self.process.poll() is None:
                line = self.process.stderr.readline()
                if line:
                    self.stderr_lines.append(line.strip())
        except Exception:
            pass
    
    def send_request(self, method: str, params: Optional[dict] = None, timeout: int = 30) -> dict:
        """
        Send a JSON-RPC request and wait for response.
        
        Args:
            method: The JSON-RPC method to call
            params: Parameters for the method
            timeout: Timeout in seconds
            
        Returns:
            Response dictionary
        """
        request = {
            "jsonrpc": "2.0",
            "id": next_msg_id(),
            "method": method,
            "params": params or {}
        }
        try:
            self.process.stdin.write(json.dumps(request) + "\n")
            self.process.stdin.flush()
        except Exception as e:
            return {"error": f"Failed to send request: {e}"}
        
        try:
            response_str = self.response_queue.get(timeout=timeout)
            return json.loads(response_str)
        except queue.Empty:
            stderr = "\n".join(self.stderr_lines[-10:]) if self.stderr_lines else "No stderr"
            return {"error": f"Timeout waiting for response. Recent stderr:\n{stderr}"}
        except json.JSONDecodeError as e:
            return {"error": f"Invalid JSON response: {e}"}
    
    def send_notification(self, method: str, params: Optional[dict] = None) -> None:
        """Send a JSON-RPC notification (no response expected)."""
        notification = {"jsonrpc": "2.0", "method": method}
        if params:
            notification["params"] = params
        try:
            self.process.stdin.write(json.dumps(notification) + "\n")
            self.process.stdin.flush()
        except Exception as e:
            print(f"Failed to send notification: {e}")
    
    def call_tool(self, tool_name: str, arguments: dict, timeout: int = 30) -> dict:
        """
        Call an MCP tool.
        
        Args:
            tool_name: Name of the tool to call
            arguments: Arguments for the tool
            timeout: Timeout in seconds
            
        Returns:
            Tool response dictionary
        """
        return self.send_request("tools/call", {"name": tool_name, "arguments": arguments}, timeout=timeout)
    
    def initialize(self) -> dict:
        """Initialize the MCP session and send initialized notification."""
        response = self.send_request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "integration-test-client", "version": "1.0.0"}
        })
        time.sleep(0.2)
        self.send_notification("notifications/initialized")
        time.sleep(0.3)
        return response
    
    def stop(self) -> None:
        """Stop the MCP server process."""
        if self.process:
            try:
                self.process.stdin.close()
                self.process.terminate()
                self.process.wait(timeout=5)
            except Exception:
                try:
                    self.process.kill()
                except Exception:
                    pass
    
    def get_stderr(self) -> str:
        """Get stderr output from the server."""
        return "\n".join(self.stderr_lines)


def extract_result_text(tool_response: dict) -> str:
    """
    Extract the actual result text from MCP response format.
    
    Args:
        tool_response: The tool response dictionary
        
    Returns:
        Extracted text content
    """
    if "result" not in tool_response:
        return ""
    result = tool_response["result"]
    if not isinstance(result, dict):
        return ""
    content = result.get("content", [])
    has_valid_content = content and isinstance(content, list) and len(content) > 0
    if has_valid_content:
        return content[0].get("text", "")
    structured = result.get("structuredContent", {})
    return structured.get("result", "")


def extract_code_health_score(response_text: str) -> Optional[float]:
    """
    Extract Code Health score from response text.
    
    Args:
        response_text: Response text from code_health_score or code_health_review tool
        
    Returns:
        The score as a float, or None if not found
    """
    import re
    
    # Try different patterns
    patterns = [
        r'code health score[:\s]+([0-9]+\.?[0-9]*)',
        r'score[:\s]+([0-9]+\.?[0-9]*)',
        r'health[:\s]+([0-9]+\.?[0-9]*)',
    ]
    
    text_lower = response_text.lower()
    for pattern in patterns:
        match = re.search(pattern, text_lower)
        if match:
            try:
                return float(match.group(1))
            except ValueError:
                continue
    
    return None


@dataclass
class BuildConfig:
    """Configuration for building the static executable."""
    repo_root: Path
    build_dir: Path
    python_executable: str = "python3.13"
    
    
class ExecutableBuilder:
    """Handles building the static executable in an isolated environment."""
    
    # Platform-specific CLI download URLs
    CLI_URLS = {
        ("Darwin", "arm64"): "https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip",
        ("Darwin", "aarch64"): "https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip",
        ("Darwin", "x86_64"): "https://downloads.codescene.io/enterprise/cli/cs-macos-amd64-latest.zip",
        ("Darwin", "amd64"): "https://downloads.codescene.io/enterprise/cli/cs-macos-amd64-latest.zip",
        ("Linux", "aarch64"): "https://downloads.codescene.io/enterprise/cli/cs-linux-aarch64-latest.zip",
        ("Linux", "arm64"): "https://downloads.codescene.io/enterprise/cli/cs-linux-aarch64-latest.zip",
        ("Linux", "x86_64"): "https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip",
        ("Linux", "amd64"): "https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip",
        ("Windows", "amd64"): "https://downloads.codescene.io/enterprise/cli/cs-windows-amd64-latest.zip",
        ("Windows", "x86_64"): "https://downloads.codescene.io/enterprise/cli/cs-windows-amd64-latest.zip",
    }
    
    def __init__(self, config: BuildConfig):
        self.config = config
    
    def _get_cli_download_url(self) -> str:
        """Get the appropriate CLI download URL for the current platform."""
        system = platform.system()
        machine = platform.machine().lower()
        
        url = self.CLI_URLS.get((system, machine))
        if url:
            return url
        
        # Fallback for unrecognized architectures
        fallback_key = (system, "x86_64")
        url = self.CLI_URLS.get(fallback_key)
        if url:
            return url
        
        raise RuntimeError(f"Unsupported platform: {system} {machine}")
    
    def _download_cli(self, dest_dir: Path) -> Path:
        """
        Download the CodeScene CLI for the current platform.
        
        Args:
            dest_dir: Directory to download and extract CLI to
            
        Returns:
            Path to the extracted CLI executable
        """
        url = self._get_cli_download_url()
        print(f"  Downloading CLI from: {url}")
        
        zip_path = dest_dir / "cli.zip"
        urllib.request.urlretrieve(url, zip_path)
        
        # Extract the zip
        with zipfile.ZipFile(zip_path, 'r') as zip_ref:
            zip_ref.extractall(dest_dir)
        
        # Find the CLI executable
        is_windows = os.name == "nt" or platform.system() == "Windows"
        cli_name = "cs.exe" if is_windows else "cs"
        
        cli_path = dest_dir / cli_name
        if not cli_path.exists():
            # Try to find it in subdirectories
            for file_path in dest_dir.rglob(cli_name):
                cli_path = file_path
                break
        
        if not cli_path.exists():
            raise FileNotFoundError(f"Could not find {cli_name} after extraction")
        
        # Make executable on Unix-like systems
        if not is_windows:
            cli_path.chmod(0o755)
        
        print(f"  Downloaded CLI to: {cli_path}")
        return cli_path
        
    def _is_windows(self) -> bool:
        """Check if running on Windows."""
        return os.name == "nt" or platform.system() == "Windows"
    
    def _get_cli_name(self) -> str:
        """Get the platform-specific CLI executable name."""
        return "cs.exe" if self._is_windows() else "cs"
    
    def _get_executable_name(self) -> str:
        """Get the platform-specific output executable name."""
        return "cs-mcp.exe" if self._is_windows() else "cs-mcp"
    
    def _copy_source_files(self) -> None:
        """Copy source files to the build directory."""
        print("  Copying source files to build directory...")
        src_dest = self.config.build_dir / "src"
        if src_dest.exists():
            shutil.rmtree(src_dest)
        shutil.copytree(self.config.repo_root / "src", src_dest)
        
        docs_dest = self.config.build_dir / "src" / "docs"
        if docs_dest.exists():
            shutil.rmtree(docs_dest)
        shutil.copytree(self.config.repo_root / "src" / "docs", docs_dest)
    
    def _ensure_cli_available(self) -> None:
        """Ensure the CodeScene CLI is available in the build directory."""
        cs_name = self._get_cli_name()
        cs_source = self.config.repo_root / cs_name
        cs_dest = self.config.build_dir / cs_name
        
        if cs_source.exists():
            shutil.copy2(cs_source, cs_dest)
            print(f"  Copied {cs_name} from repo root")
            return
        
        print(f"  No {cs_name} found in repo root, downloading...")
        cli_path = self._download_cli(self.config.build_dir)
        if cli_path != cs_dest:
            shutil.move(str(cli_path), str(cs_dest))
    
    def _run_nuitka_build(self) -> Path:
        """Run Nuitka to build the executable."""
        print("  Building with Nuitka (this may take several minutes)...")
        print("  ", end="", flush=True)
        
        executable_name = self._get_executable_name()
        cs_data_file = self._get_cli_name()
        
        build_cmd = [
            self.config.python_executable,
            "-m", "nuitka",
            "--onefile",
            "--assume-yes-for-downloads",
            f"--include-data-dir=./src/docs=src/docs",
            f"--include-data-files=./{cs_data_file}={cs_data_file}",
            f"--output-filename={executable_name}",
            "src/cs_mcp_server.py"
        ]
        
        # Stream output to avoid GitHub Actions timeout on silence
        process = subprocess.Popen(
            build_cmd,
            cwd=str(self.config.build_dir),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1
        )
        
        # Print dots every few lines to show progress
        line_count = 0
        output_lines = []
        for line in process.stdout:
            output_lines.append(line)
            line_count += 1
            if line_count % 10 == 0:
                print(".", end="", flush=True)
        
        return_code = process.wait()
        print()  # Newline after dots
        
        if return_code != 0:
            print(f"  \033[91mBuild failed:\033[0m")
            print("".join(output_lines[-50:]))  # Show last 50 lines
            raise RuntimeError("Nuitka build failed")
        
        binary_path = self.config.build_dir / executable_name
        if not binary_path.exists():
            raise FileNotFoundError(f"Binary not found at {binary_path} after build")
        
        return binary_path
        
    def build(self) -> Path:
        """
        Build the static executable using Nuitka.
        
        Returns:
            Path to the built executable in the isolated build directory
        """
        print_header("Building Static Executable")
        
        self.config.build_dir.mkdir(parents=True, exist_ok=True)
        self._copy_source_files()
        self._ensure_cli_available()
        
        binary_path = self._run_nuitka_build()
        print(f"  \033[92mBuild successful:\033[0m {binary_path}")
        return binary_path


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
    subprocess.run(["git", "config", "user.name", "Test User"], cwd=repo_dir, check=True, capture_output=True)
    subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=repo_dir, check=True, capture_output=True)
    
    # Create sample files
    for file_path, content in sample_files.items():
        full_path = repo_dir / file_path
        full_path.parent.mkdir(parents=True, exist_ok=True)
        full_path.write_text(content)
    
    # Initial commit
    subprocess.run(["git", "add", "."], cwd=repo_dir, check=True, capture_output=True)
    subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=repo_dir, check=True, capture_output=True)
    
    return repo_dir


def cleanup_dir(path: Path) -> None:
    """Safely remove a directory."""
    if path.exists():
        try:
            shutil.rmtree(path)
        except Exception as e:
            print(f"  Warning: Failed to cleanup {path}: {e}")

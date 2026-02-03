#!/usr/bin/env python3
"""
Shared utilities for MCP integration tests.

This module provides common functionality for testing MCP tools:
- MCPClient: A client for communicating with MCP servers via stdio
- Test helpers for checking tool responses
- Common test patterns for path resolution tests
"""

import json
import os
import queue
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass
from typing import Callable


# MCP Protocol message IDs
_msg_id = 0


def next_msg_id():
    global _msg_id
    _msg_id += 1
    return _msg_id


def print_header(msg: str):
    print(f"\n{'='*60}")
    print(f"  {msg}")
    print(f"{'='*60}\n")


def print_test(name: str, passed: bool, details: str = ""):
    status = "[PASS]" if passed else "[FAIL]"
    print(f"  {status}: {name}")
    if details:
        for line in details.split('\n')[:5]:
            print(f"         {line}")


def extract_result_text(tool_response: dict) -> str:
    """Extract the actual result text from MCP response format."""
    if "result" not in tool_response:
        return ""
    result = tool_response["result"]
    if not isinstance(result, dict):
        return ""
    content = result.get("content", [])
    if content and isinstance(content, list):
        return content[0].get("text", "")
    structured = result.get("structuredContent", {})
    return structured.get("result", "")


class MCPClient:
    """MCP client that communicates with the server via stdio."""
    
    def __init__(self, command: list, env: dict = None):
        self.command = command
        self.env = env or os.environ.copy()
        self.process = None
        self.response_queue = queue.Queue()
        self.reader_thread = None
        
    def start(self):
        """Start the MCP server process."""
        self.process = subprocess.Popen(
            self.command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=self.env,
            text=True,
            encoding="utf-8",
            bufsize=1
        )
        self.reader_thread = threading.Thread(target=self._read_responses, daemon=True)
        self.reader_thread.start()
        time.sleep(1)
        return self.process.poll() is None
    
    def _read_responses(self):
        """Read responses from the server in a background thread."""
        try:
            while self.process and self.process.poll() is None:
                line = self.process.stdout.readline()
                if line:
                    self.response_queue.put(line.strip())
        except Exception as e:
            self.response_queue.put(f"ERROR: {e}")
    
    def send_request(self, method: str, params: dict = None) -> dict:
        """Send a JSON-RPC request and wait for response."""
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
            response_str = self.response_queue.get(timeout=30)
            return json.loads(response_str)
        except queue.Empty:
            return {"error": "Timeout waiting for response"}
        except json.JSONDecodeError as e:
            return {"error": f"Invalid JSON response: {e}"}
    
    def send_notification(self, method: str, params: dict = None):
        """Send a JSON-RPC notification (no response expected)."""
        notification = {"jsonrpc": "2.0", "method": method}
        if params:
            notification["params"] = params
        try:
            self.process.stdin.write(json.dumps(notification) + "\n")
            self.process.stdin.flush()
        except Exception as e:
            print(f"Failed to send notification: {e}")
    
    def call_tool(self, tool_name: str, arguments: dict) -> dict:
        """Call an MCP tool."""
        return self.send_request("tools/call", {"name": tool_name, "arguments": arguments})
    
    def initialize(self) -> dict:
        """Initialize the MCP session and send initialized notification."""
        response = self.send_request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "path-resolution-test-client", "version": "1.0.0"}
        })
        time.sleep(0.2)
        self.send_notification("notifications/initialized")
        time.sleep(0.3)
        return response
    
    def stop(self):
        """Stop the MCP server process."""
        if self.process:
            try:
                self.process.stdin.close()
                self.process.terminate()
                self.process.wait(timeout=5)
            except Exception:
                self.process.kill()
    
    def get_stderr(self) -> str:
        """Get any stderr output from the server."""
        if self.process and self.process.stderr:
            try:
                return self.process.stderr.read()
            except Exception:
                return ""
        return ""


@dataclass
class ToolTestConfig:
    """Configuration for a tool test."""
    tool_name: str
    arguments: dict
    header: str
    forbidden_patterns: list[str]
    test_description: str
    required_patterns: list[str] | None = None  # Patterns that MUST be in the response


def find_forbidden_patterns(text: str, patterns: list[str]) -> list[str]:
    """Find which forbidden patterns appear in the text."""
    return [p for p in patterns if p.lower() in text.lower()]


def find_missing_patterns(text: str, patterns: list[str] | None) -> list[str]:
    """Find which required patterns are missing from the text."""
    if not patterns:
        return []
    return [p for p in patterns if p.lower() not in text.lower()]


def build_test_details(
    result_text: str,
    found_forbidden: list[str],
    missing_required: list[str],
) -> str:
    """Build the test details message."""
    details = f"Response: {result_text[:150]}..."
    if found_forbidden:
        details = f"Found forbidden patterns: {found_forbidden}\n{details}"
    if missing_required:
        details = f"Missing required patterns: {missing_required}\n{details}"
    return details


def execute_tool_test(client: MCPClient, config: ToolTestConfig) -> bool:
    """Execute the tool test and check results. Assumes client is started."""
    client.initialize()
    
    tool_response = client.call_tool(config.tool_name, config.arguments)
    result_text = extract_result_text(tool_response)
    
    found_forbidden = find_forbidden_patterns(result_text, config.forbidden_patterns)
    missing_required = find_missing_patterns(result_text, config.required_patterns)
    
    passed = len(found_forbidden) == 0 and len(missing_required) == 0
    details = build_test_details(result_text, found_forbidden, missing_required)
    
    print_test(config.test_description, passed, details)
    return passed


def run_tool_test(command: list, env: dict, config: ToolTestConfig) -> bool:
    """
    Run a generic tool test with the given configuration.
    
    This is the common test pattern extracted from the individual test functions.
    It handles:
    - Starting the MCP client
    - Initializing the session
    - Calling the specified tool
    - Checking the response for forbidden patterns
    - Checking the response for required patterns
    - Cleanup
    
    Args:
        command: The command to start the MCP server
        env: Environment variables for the server
        config: Test configuration specifying tool, arguments, and checks
        
    Returns:
        True if the test passed, False otherwise
    """
    print_header(config.header)
    
    client = MCPClient(command, env=env)
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        print_test("MCP server started", True)
        
        return execute_tool_test(client, config)
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def create_static_mode_env() -> dict:
    """Create environment for static mode testing (no CS_MOUNT_PATH)."""
    env = os.environ.copy()
    env.pop('CS_MOUNT_PATH', None)
    return env


def create_test_git_repo() -> tuple[str, str]:
    """
    Create a temporary git repository with a test file.
    
    Returns:
        Tuple of (tmpdir, test_file_path)
    """
    tmpdir = tempfile.mkdtemp(prefix="mcp-path-test-")
    
    subprocess.run(["git", "init"], cwd=tmpdir, capture_output=True)
    subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=tmpdir, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test"], cwd=tmpdir, capture_output=True)
    
    src_dir = os.path.join(tmpdir, "src")
    os.makedirs(src_dir)
    
    test_file = os.path.join(src_dir, "TestFile.java")
    with open(test_file, "w", encoding="utf-8") as f:
        f.write("""public class TestFile {
    public void hello() {
        System.out.println("Hello");
    }
}
""")
    
    subprocess.run(["git", "add", "."], cwd=tmpdir, capture_output=True)
    subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=tmpdir, capture_output=True)
    
    return tmpdir, test_file


def create_test_file_no_git() -> tuple[str, str]:
    """
    Create a temporary file NOT in a git repository.
    
    Returns:
        Tuple of (tmpdir, test_file_path)
    """
    tmpdir = tempfile.mkdtemp(prefix="mcp-no-git-test-")
    
    src_dir = os.path.join(tmpdir, "src")
    os.makedirs(src_dir)
    
    test_file = os.path.join(src_dir, "TestFile.java")
    with open(test_file, "w", encoding="utf-8") as f:
        f.write("""public class TestFile {
    public void hello() {
        System.out.println("Hello");
    }
}
""")
    
    return tmpdir, test_file


def cleanup_test_dir(tmpdir: str):
    """Clean up a temporary test directory."""
    import shutil
    try:
        shutil.rmtree(tmpdir)
    except Exception:
        pass


def create_test_git_worktree() -> tuple[str, str, str]:
    """
    Create a temporary git worktree with a test file.
    
    This creates:
    1. A main git repository with initial commit
    2. A worktree linked to that repository
    3. A test file in the worktree
    
    Returns:
        Tuple of (base_dir, worktree_dir, test_file_path)
        base_dir contains both main repo and worktree, use for cleanup
    """
    # Create base temp directory to hold both main repo and worktree
    base_dir = tempfile.mkdtemp(prefix="mcp-worktree-test-")
    main_dir = os.path.join(base_dir, "main-repo")
    worktree_dir = os.path.join(base_dir, "worktree")
    os.makedirs(main_dir)
    
    # Initialize main repo
    subprocess.run(["git", "init"], cwd=main_dir, capture_output=True, check=True)
    subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=main_dir, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test"], cwd=main_dir, capture_output=True)
    
    # Create and commit a file in main repo
    src_dir = os.path.join(main_dir, "src")
    os.makedirs(src_dir)
    test_file_main = os.path.join(src_dir, "TestFile.java")
    with open(test_file_main, "w", encoding="utf-8") as f:
        f.write("""public class TestFile {
    public void hello() {
        System.out.println("Hello");
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
    
    test_file = os.path.join(worktree_dir, "src", "TestFile.java")
    return base_dir, worktree_dir, test_file


def print_test_summary(results: list[tuple[str, bool]]) -> int:
    """
    Print a summary of test results.
    
    Args:
        results: List of (test_name, passed) tuples
        
    Returns:
        Exit code (0 if all passed, 1 otherwise)
    """
    print_header("Test Summary")
    
    passed = sum(1 for _, p in results if p)
    total = len(results)
    
    for name, result in results:
        print(f"  {'[PASS]' if result else '[FAIL]'}: {name}")
    
    print(f"\n  Total: {passed}/{total} passed")
    
    if passed == total:
        print("\n  All tests passed!")
        return 0
    print(f"\n  {total - passed} test(s) failed!")
    return 1

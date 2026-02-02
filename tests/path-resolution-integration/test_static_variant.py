#!/usr/bin/env python3
"""
Static binary variant path resolution integration test.

This test verifies that the MCP tools that require file path resolution
work correctly in static executable mode (without CS_MOUNT_PATH set).

The tools tested are:
- code_ownership_for_path
- list_technical_debt_hotspots_for_project_file
- list_technical_debt_goals_for_project_file

These tools previously failed with "CS_MOUNT_PATH not defined" error when
running in static executable mode. This test ensures they now work correctly
by using git root detection for path resolution.

Usage: python test_static_variant.py /path/to/cs-mcp
"""

import json
import os
import subprocess
import sys
import tempfile
import threading
import queue
import time

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
    status = "\u2713 PASS" if passed else "\u2717 FAIL"
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


def create_test_git_repo():
    """Create a temporary git repository with a test file."""
    tmpdir = tempfile.mkdtemp(prefix="mcp-path-test-")
    
    # Initialize git repo
    subprocess.run(["git", "init"], cwd=tmpdir, capture_output=True)
    subprocess.run(["git", "config", "user.email", "test@test.com"], cwd=tmpdir, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test"], cwd=tmpdir, capture_output=True)
    
    # Create test file structure
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
    
    # Commit the file
    subprocess.run(["git", "add", "."], cwd=tmpdir, capture_output=True)
    subprocess.run(["git", "commit", "-m", "Initial commit"], cwd=tmpdir, capture_output=True)
    
    return tmpdir, test_file


def cleanup_test_repo(tmpdir: str):
    """Clean up the temporary git repository."""
    import shutil
    try:
        shutil.rmtree(tmpdir)
    except Exception:
        pass


def test_environment_setup(binary_path: str):
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    checks = []
    
    # Check binary
    binary_ok = os.path.exists(binary_path) and os.access(binary_path, os.X_OK)
    checks.append(binary_ok)
    print_test("cs-mcp binary exists and is executable", binary_ok, f"Path: {binary_path}")
    
    # Check that CS_MOUNT_PATH is NOT set (we're testing static mode)
    mount_path = os.getenv('CS_MOUNT_PATH')
    no_mount_ok = mount_path is None
    checks.append(no_mount_ok)
    print_test("CS_MOUNT_PATH is NOT set (static mode)", no_mount_ok, 
               f"Value: {mount_path}" if mount_path else "Not set (correct)")
    
    return all(checks)


def test_mcp_server_starts(binary_path: str):
    """Verify the MCP server starts successfully in static mode."""
    print_header("Test MCP Server Startup (Static Mode)")
    
    # Ensure CS_MOUNT_PATH is not set
    env = os.environ.copy()
    env.pop('CS_MOUNT_PATH', None)
    
    client = MCPClient([binary_path], env=env)
    
    try:
        started = client.start()
        print_test("MCP server process started", started)
        if not started:
            return False
        
        response = client.initialize()
        print_test("MCP server responds to initialize", "result" in response)
        return True
    except Exception as e:
        print_test("MCP server starts", False, str(e))
        return False
    finally:
        client.stop()


def test_code_ownership_no_mount_path_error(binary_path: str, test_file: str):
    """Test that code_ownership_for_path doesn't fail with CS_MOUNT_PATH error."""
    print_header("Test code_ownership_for_path (Static Mode)")
    
    env = os.environ.copy()
    env.pop('CS_MOUNT_PATH', None)
    
    client = MCPClient([binary_path], env=env)
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        print_test("MCP server started", True)
        
        client.initialize()
        
        # Call code_ownership_for_path - it should NOT fail with "CS_MOUNT_PATH not defined"
        tool_response = client.call_tool("code_ownership_for_path", {
            "project_id": 1,
            "path": test_file
        })
        result_text = extract_result_text(tool_response)
        
        # Check that we don't get the CS_MOUNT_PATH error
        has_mount_path_error = "CS_MOUNT_PATH" in result_text
        print_test("No CS_MOUNT_PATH error", not has_mount_path_error, 
                   f"Response: {result_text[:150]}...")
        
        return not has_mount_path_error
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def test_technical_debt_hotspots_no_mount_path_error(binary_path: str, test_file: str):
    """Test that list_technical_debt_hotspots_for_project_file doesn't fail with CS_MOUNT_PATH error."""
    print_header("Test list_technical_debt_hotspots_for_project_file (Static Mode)")
    
    env = os.environ.copy()
    env.pop('CS_MOUNT_PATH', None)
    
    client = MCPClient([binary_path], env=env)
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        print_test("MCP server started", True)
        
        client.initialize()
        
        tool_response = client.call_tool("list_technical_debt_hotspots_for_project_file", {
            "project_id": 1,
            "file_path": test_file
        })
        result_text = extract_result_text(tool_response)
        
        has_mount_path_error = "CS_MOUNT_PATH" in result_text
        print_test("No CS_MOUNT_PATH error", not has_mount_path_error, 
                   f"Response: {result_text[:150]}...")
        
        return not has_mount_path_error
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def test_technical_debt_goals_no_mount_path_error(binary_path: str, test_file: str):
    """Test that list_technical_debt_goals_for_project_file doesn't fail with CS_MOUNT_PATH error."""
    print_header("Test list_technical_debt_goals_for_project_file (Static Mode)")
    
    env = os.environ.copy()
    env.pop('CS_MOUNT_PATH', None)
    
    client = MCPClient([binary_path], env=env)
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        print_test("MCP server started", True)
        
        client.initialize()
        
        tool_response = client.call_tool("list_technical_debt_goals_for_project_file", {
            "project_id": 1,
            "file_path": test_file
        })
        result_text = extract_result_text(tool_response)
        
        has_mount_path_error = "CS_MOUNT_PATH" in result_text
        print_test("No CS_MOUNT_PATH error", not has_mount_path_error, 
                   f"Response: {result_text[:150]}...")
        
        return not has_mount_path_error
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def create_test_file_no_git():
    """Create a temporary file NOT in a git repository."""
    tmpdir = tempfile.mkdtemp(prefix="mcp-no-git-test-")
    
    # Create test file structure (NO git init!)
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


def test_code_ownership_no_git_repo(binary_path: str, test_file: str):
    """Test that code_ownership_for_path works outside a git repository."""
    print_header("Test code_ownership_for_path (No Git Repo)")
    
    env = os.environ.copy()
    env.pop('CS_MOUNT_PATH', None)
    
    client = MCPClient([binary_path], env=env)
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        print_test("MCP server started", True)
        
        client.initialize()
        
        tool_response = client.call_tool("code_ownership_for_path", {
            "project_id": 1,
            "path": test_file
        })
        result_text = extract_result_text(tool_response)
        
        # Should NOT have CS_MOUNT_PATH error
        has_mount_path_error = "CS_MOUNT_PATH" in result_text
        # Should NOT have "not in a git repository" error
        has_git_error = "not in a git repository" in result_text.lower()
        
        passed = not has_mount_path_error and not has_git_error
        print_test("No CS_MOUNT_PATH or git repo error", passed, 
                   f"Response: {result_text[:150]}...")
        
        return passed
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def test_code_ownership_relative_path(binary_path: str):
    """Test that code_ownership_for_path works with a relative path."""
    print_header("Test code_ownership_for_path (Relative Path)")
    
    env = os.environ.copy()
    env.pop('CS_MOUNT_PATH', None)
    
    client = MCPClient([binary_path], env=env)
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        print_test("MCP server started", True)
        
        client.initialize()
        
        # Use a relative path - should work without any git/mount requirements
        tool_response = client.call_tool("code_ownership_for_path", {
            "project_id": 1,
            "path": "src/components/Button.tsx"
        })
        result_text = extract_result_text(tool_response)
        
        # Should NOT have CS_MOUNT_PATH error
        has_mount_path_error = "CS_MOUNT_PATH" in result_text
        # Should NOT have "not in a git repository" error
        has_git_error = "not in a git repository" in result_text.lower()
        
        passed = not has_mount_path_error and not has_git_error
        print_test("No CS_MOUNT_PATH or git repo error with relative path", passed, 
                   f"Response: {result_text[:150]}...")
        
        return passed
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def main():
    if len(sys.argv) < 2:
        print("Usage: python test_static_variant.py /path/to/cs-mcp")
        return 1
    
    binary_path = sys.argv[1]
    
    print("\n" + "="*60)
    print("  Static Binary Path Resolution Integration Tests")
    print("  Testing: cs-mcp binary without CS_MOUNT_PATH")
    print("="*60)
    
    # Create test git repo
    print("\n  Creating temporary git repository for testing...")
    tmpdir, test_file = create_test_git_repo()
    print(f"  Test repo: {tmpdir}")
    print(f"  Test file: {test_file}")
    
    # Create test file outside git repo
    print("\n  Creating temporary file outside git repo...")
    no_git_tmpdir, no_git_test_file = create_test_file_no_git()
    print(f"  Test dir: {no_git_tmpdir}")
    print(f"  Test file: {no_git_test_file}")
    
    try:
        results = [
            ("Environment Setup", test_environment_setup(binary_path)),
            ("MCP Server Startup", test_mcp_server_starts(binary_path)),
            # Tests with file inside git repo
            ("code_ownership_for_path (in git repo)", 
             test_code_ownership_no_mount_path_error(binary_path, test_file)),
            ("list_technical_debt_hotspots_for_project_file (in git repo)", 
             test_technical_debt_hotspots_no_mount_path_error(binary_path, test_file)),
            ("list_technical_debt_goals_for_project_file (in git repo)", 
             test_technical_debt_goals_no_mount_path_error(binary_path, test_file)),
            # Tests outside git repo - should also work now
            ("code_ownership_for_path (no git repo)", 
             test_code_ownership_no_git_repo(binary_path, no_git_test_file)),
            ("code_ownership_for_path (relative path)", 
             test_code_ownership_relative_path(binary_path)),
        ]
        
        print_header("Test Summary")
        
        passed = sum(1 for _, p in results if p)
        total = len(results)
        
        for name, result in results:
            print(f"  {'\u2713 PASS' if result else '\u2717 FAIL'}: {name}")
        
        print(f"\n  Total: {passed}/{total} passed")
        
        if passed == total:
            print("\n  All tests passed! \u2713")
            return 0
        print(f"\n  {total - passed} test(s) failed! \u2717")
        return 1
    
    finally:
        cleanup_test_repo(tmpdir)
        cleanup_test_repo(no_git_tmpdir)


if __name__ == '__main__':
    sys.exit(main())

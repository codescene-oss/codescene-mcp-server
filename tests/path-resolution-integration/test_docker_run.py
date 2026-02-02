#!/usr/bin/env python3
"""
Docker run path resolution integration test.

This test runs on the HOST and tests the actual `docker run` command
that users would use. It verifies that the MCP tools that require file 
path resolution work correctly in Docker mode (with CS_MOUNT_PATH set).

The tools tested are:
- code_ownership_for_path
- list_technical_debt_hotspots_for_project_file
- list_technical_debt_goals_for_project_file

Environment variables (should be set before running):
- DOCKER_IMAGE: Docker image name to test (default: codescene-mcp)
- TEST_DATA_PATH: Path to test data files (will create temp if not set)
"""

import json
import os
import subprocess
import sys
import tempfile
import threading
import queue
import time
import shutil

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


def create_test_data():
    """Create a temporary directory with test files."""
    tmpdir = tempfile.mkdtemp(prefix="mcp-docker-path-test-")
    
    # Create test file
    test_file = os.path.join(tmpdir, "TestFile.java")
    with open(test_file, "w", encoding="utf-8") as f:
        f.write("""public class TestFile {
    public void hello() {
        System.out.println("Hello");
    }
}
""")
    
    return tmpdir


def cleanup_test_data(tmpdir: str):
    """Clean up the temporary directory."""
    try:
        shutil.rmtree(tmpdir)
    except Exception:
        pass


def build_docker_command(test_data_path: str, docker_image: str):
    """Build the docker run command with CS_MOUNT_PATH set."""
    cmd = [
        'docker', 'run', '-i', '--rm',
        '-e', 'CS_MOUNT_PATH=/mount',
        '-e', 'CS_ACCESS_TOKEN=test-token',
        '-v', f'{test_data_path}:/mount:ro',
    ]
    cmd.append(docker_image)
    return cmd


class MCPClient:
    """MCP client that communicates with a Docker container via stdio."""
    
    def __init__(self, command: list):
        self.command = command
        self.process = None
        self.response_queue = queue.Queue()
        self.reader_thread = None
        
    def start(self):
        """Start the Docker container."""
        self.process = subprocess.Popen(
            self.command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            bufsize=1
        )
        self.reader_thread = threading.Thread(target=self._read_responses, daemon=True)
        self.reader_thread.start()
        time.sleep(2)  # Docker containers take longer to start
        return self.process.poll() is None
    
    def _read_responses(self):
        """Read responses from the container in a background thread."""
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
        request_str = json.dumps(request)
        self.process.stdin.write(request_str + '\n')
        self.process.stdin.flush()
        
        try:
            response = self.response_queue.get(timeout=30)
            return json.loads(response)
        except queue.Empty:
            return {"error": "Timeout waiting for response"}
        except json.JSONDecodeError as e:
            return {"error": f"Invalid JSON: {e}"}
    
    def send_notification(self, method: str, params: dict = None):
        """Send a JSON-RPC notification (no response expected)."""
        notification = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params or {}
        }
        self.process.stdin.write(json.dumps(notification) + '\n')
        self.process.stdin.flush()
    
    def initialize(self) -> dict:
        """Initialize the MCP session."""
        response = self.send_request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "path-resolution-docker-test", "version": "1.0"}
        })
        self.send_notification("notifications/initialized")
        time.sleep(0.5)
        return response
    
    def call_tool(self, name: str, arguments: dict) -> dict:
        """Call an MCP tool."""
        return self.send_request("tools/call", {
            "name": name,
            "arguments": arguments
        })
    
    def stop(self):
        """Stop the Docker container."""
        if self.process:
            try:
                self.process.terminate()
                self.process.wait(timeout=5)
            except Exception:
                self.process.kill()
    
    def get_stderr(self) -> str:
        """Get any stderr output from the container."""
        if self.process and self.process.stderr:
            try:
                return self.process.stderr.read()
            except Exception:
                return ""
        return ""


def test_environment_setup(docker_image: str, test_data_path: str):
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    checks = []
    
    # Check Docker image exists
    result = subprocess.run(['docker', 'image', 'inspect', docker_image], capture_output=True)
    image_ok = result.returncode == 0
    checks.append(image_ok)
    print_test("Docker image exists", image_ok, f"Image: {docker_image}")
    
    # Check test data path exists
    path_ok = os.path.exists(test_data_path)
    checks.append(path_ok)
    print_test("Test data path exists", path_ok, f"Path: {test_data_path}")
    
    return all(checks)


def test_docker_run_starts(docker_image: str, test_data_path: str):
    """Verify docker run starts the MCP server with CS_MOUNT_PATH."""
    print_header("Test Docker Run Startup (with CS_MOUNT_PATH)")
    
    cmd = build_docker_command(test_data_path, docker_image)
    print(f"  Command: docker run -i ... {docker_image}")
    
    client = MCPClient(cmd)
    try:
        started = client.start()
        print_test("Docker container started", started)
        if not started:
            stderr = client.get_stderr()
            if stderr:
                print(f"  stderr: {stderr[:200]}")
            return False
        
        response = client.initialize()
        print_test("MCP server responds to initialize", "result" in response)
        return True
    except Exception as e:
        print_test("Docker run starts", False, str(e))
        return False
    finally:
        client.stop()


def test_code_ownership_with_mount_path(docker_image: str, test_data_path: str):
    """Test that code_ownership_for_path works with CS_MOUNT_PATH set."""
    print_header("Test code_ownership_for_path (Docker Mode)")
    
    cmd = build_docker_command(test_data_path, docker_image)
    client = MCPClient(cmd)
    
    # The file path as seen from the host (which gets translated via CS_MOUNT_PATH)
    host_file_path = os.path.join(test_data_path, "TestFile.java")
    
    try:
        if not client.start():
            print_test("Docker container started", False)
            return False
        print_test("Docker container started", True)
        
        client.initialize()
        
        tool_response = client.call_tool("code_ownership_for_path", {
            "project_id": 1,
            "path": host_file_path
        })
        result_text = extract_result_text(tool_response)
        
        # Should not have CS_MOUNT_PATH error (it's set in Docker mode)
        has_mount_path_error = "CS_MOUNT_PATH" in result_text and "not defined" in result_text
        print_test("No CS_MOUNT_PATH error", not has_mount_path_error, 
                   f"Response: {result_text[:150]}...")
        
        return not has_mount_path_error
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def test_technical_debt_hotspots_with_mount_path(docker_image: str, test_data_path: str):
    """Test that list_technical_debt_hotspots_for_project_file works with CS_MOUNT_PATH."""
    print_header("Test list_technical_debt_hotspots_for_project_file (Docker Mode)")
    
    cmd = build_docker_command(test_data_path, docker_image)
    client = MCPClient(cmd)
    
    host_file_path = os.path.join(test_data_path, "TestFile.java")
    
    try:
        if not client.start():
            print_test("Docker container started", False)
            return False
        print_test("Docker container started", True)
        
        client.initialize()
        
        tool_response = client.call_tool("list_technical_debt_hotspots_for_project_file", {
            "project_id": 1,
            "file_path": host_file_path
        })
        result_text = extract_result_text(tool_response)
        
        has_mount_path_error = "CS_MOUNT_PATH" in result_text and "not defined" in result_text
        print_test("No CS_MOUNT_PATH error", not has_mount_path_error, 
                   f"Response: {result_text[:150]}...")
        
        return not has_mount_path_error
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def test_technical_debt_goals_with_mount_path(docker_image: str, test_data_path: str):
    """Test that list_technical_debt_goals_for_project_file works with CS_MOUNT_PATH."""
    print_header("Test list_technical_debt_goals_for_project_file (Docker Mode)")
    
    cmd = build_docker_command(test_data_path, docker_image)
    client = MCPClient(cmd)
    
    host_file_path = os.path.join(test_data_path, "TestFile.java")
    
    try:
        if not client.start():
            print_test("Docker container started", False)
            return False
        print_test("Docker container started", True)
        
        client.initialize()
        
        tool_response = client.call_tool("list_technical_debt_goals_for_project_file", {
            "project_id": 1,
            "file_path": host_file_path
        })
        result_text = extract_result_text(tool_response)
        
        has_mount_path_error = "CS_MOUNT_PATH" in result_text and "not defined" in result_text
        print_test("No CS_MOUNT_PATH error", not has_mount_path_error, 
                   f"Response: {result_text[:150]}...")
        
        return not has_mount_path_error
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        return False
    finally:
        client.stop()


def main():
    docker_image = os.getenv('DOCKER_IMAGE', 'codescene-mcp')
    test_data_path = os.getenv('TEST_DATA_PATH')
    cleanup_needed = False
    
    print("\n" + "="*60)
    print("  Docker Run Path Resolution Integration Tests")
    print("  Testing: docker run with CS_MOUNT_PATH set")
    print("="*60)
    
    # Create test data if not provided
    if not test_data_path:
        print("\n  Creating temporary test data directory...")
        test_data_path = create_test_data()
        cleanup_needed = True
    
    print(f"  Docker image: {docker_image}")
    print(f"  Test data path: {test_data_path}")
    
    try:
        results = [
            ("Environment Setup", test_environment_setup(docker_image, test_data_path)),
            ("Docker Run Startup", test_docker_run_starts(docker_image, test_data_path)),
            ("code_ownership_for_path (Docker mode)", 
             test_code_ownership_with_mount_path(docker_image, test_data_path)),
            ("list_technical_debt_hotspots_for_project_file (Docker mode)", 
             test_technical_debt_hotspots_with_mount_path(docker_image, test_data_path)),
            ("list_technical_debt_goals_for_project_file (Docker mode)", 
             test_technical_debt_goals_with_mount_path(docker_image, test_data_path)),
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
        if cleanup_needed:
            cleanup_test_data(test_data_path)


if __name__ == '__main__':
    sys.exit(main())

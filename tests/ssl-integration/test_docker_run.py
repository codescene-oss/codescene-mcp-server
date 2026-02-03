#!/usr/bin/env python3
"""
Docker run SSL integration test.

This test runs on the HOST and tests the actual `docker run` command
that users would use. It:
1. Runs `docker run -i codescene-mcp` with SSL certs mounted
2. Sends MCP protocol requests via stdio
3. Verifies SSL works end-to-end

Environment variables (set by run-docker-ssl-test.sh):
- CERT_PATH: Path to CA certificate
- DOCKER_IMAGE: Docker image name to test
- DOCKER_NETWORK: Docker network name
- NGINX_HOST: Hostname of nginx container
- TEST_DATA_PATH: Path to test data files
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

# SSL error keywords indicating certificate problems
SSL_ERROR_KEYWORDS = [
    'PKIX path building failed', 'SSLHandshakeException',
    'unable to find valid certification path',
    'certificate verify', 'CERTIFICATE_VERIFY_FAILED',
    'trustAnchors', 'ValidatorException', 'PEM',
    'trustStore', 'PKCS12', 'KeyStoreException'
]

# Auth error keywords indicating successful SSL but failed authentication
AUTH_ERROR_KEYWORDS = ['401', 'license', 'unauthorized', 'reauthorize', 'access token', 'authentication']


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


def has_ssl_error(text: str) -> bool:
    """Check if text contains SSL-related error keywords."""
    text_lower = text.lower()
    return any(kw.lower() in text_lower for kw in SSL_ERROR_KEYWORDS)


def has_auth_error(text: str) -> bool:
    """Check if text contains authentication/license error keywords."""
    text_lower = text.lower()
    return any(kw.lower() in text_lower for kw in AUTH_ERROR_KEYWORDS)


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


def build_docker_command(with_cert: bool = True, wrong_cert_path: str = None):
    """Build the docker run command."""
    image = os.environ['DOCKER_IMAGE']
    network = os.environ['DOCKER_NETWORK']
    nginx_host = os.environ['NGINX_HOST']
    cert_path = os.environ['CERT_PATH']
    test_data_path = os.environ['TEST_DATA_PATH']
    
    cmd = [
        'docker', 'run', '-i', '--rm',
        '--network', network,
        '-e', f'CS_ONPREM_URL=https://{nginx_host}',
        '-e', 'CS_ACCESS_TOKEN=test-token',
        '-e', 'CS_MOUNT_PATH=/mount',
        '-v', f'{test_data_path}:/mount:ro',
    ]
    
    if with_cert:
        actual_cert = wrong_cert_path if wrong_cert_path else cert_path
        cmd.extend([
            '-e', 'REQUESTS_CA_BUNDLE=/certs/ca.crt',
            '-v', f'{actual_cert}:/certs/ca.crt:ro',
        ])
    
    cmd.append(image)
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
            "clientInfo": {"name": "ssl-test", "version": "1.0"}
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


def test_environment_setup():
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    checks = []
    
    # Check required environment variables
    for var in ['DOCKER_IMAGE', 'DOCKER_NETWORK', 'NGINX_HOST', 'CERT_PATH', 'TEST_DATA_PATH']:
        value = os.getenv(var)
        ok = bool(value)
        checks.append(ok)
        print_test(f"{var} is set", ok, f"Value: {value}" if value else "")
    
    # Check cert file exists
    cert_path = os.getenv('CERT_PATH')
    cert_ok = cert_path and os.path.exists(cert_path)
    checks.append(cert_ok)
    print_test("Certificate file exists", cert_ok, f"Path: {cert_path}")
    
    # Check Docker image exists
    image = os.getenv('DOCKER_IMAGE')
    result = subprocess.run(['docker', 'image', 'inspect', image], capture_output=True)
    image_ok = result.returncode == 0
    checks.append(image_ok)
    print_test("Docker image exists", image_ok, f"Image: {image}")
    
    return all(checks)


def test_docker_run_starts():
    """Verify docker run starts the MCP server."""
    print_header("Test Docker Run Startup")
    
    cmd = build_docker_command(with_cert=True)
    print(f"  Command: docker run -i ... {os.environ['DOCKER_IMAGE']}")
    
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


def validate_expected_ssl_error(result_text: str, found_ssl_error: bool, found_auth_error: bool) -> bool:
    """Validate response when SSL error is expected (negative test)."""
    if found_ssl_error:
        print_test("SSL error occurred (expected)", True, f"Got expected SSL error: {result_text[:100]}")
        return True
    if found_auth_error:
        print_test("SSL error occurred", False, "Got auth error instead - SSL unexpectedly worked")
        return False
    print_test("SSL error occurred", False, f"Expected SSL error but got: {result_text[:100]}")
    return False


def validate_expected_success(result_text: str, tool_response: dict, 
                               found_ssl_error: bool, found_auth_error: bool) -> bool:
    """Validate response when SSL should work (positive test)."""
    if found_ssl_error:
        print_test("No SSL errors in response", False, f"Found SSL error: {result_text[:200]}")
        return False
    print_test("No SSL errors in response", True)
    
    if found_auth_error:
        print_test("CLI connected but auth failed (expected)", True, 
                  f"Auth error (proves SSL worked): {result_text[:100]}")
        return True
    if "error" in tool_response:
        print_test("Tool returned error", False, f"Protocol error: {tool_response.get('error', '')[:100]}")
        return False
    print_test("Tool returned result", True, f"Result: {result_text[:100]}")
    return True


def validate_ssl_response(result_text: str, tool_response: dict, expect_ssl_error: bool) -> bool:
    """Validate the response based on whether we expect SSL errors or not."""
    found_ssl_error = has_ssl_error(result_text)
    found_auth_error = has_auth_error(result_text)
    
    if expect_ssl_error:
        return validate_expected_ssl_error(result_text, found_ssl_error, found_auth_error)
    return validate_expected_success(result_text, tool_response, found_ssl_error, found_auth_error)


def handle_test_exception(e: Exception, expect_ssl_error: bool) -> bool:
    """Handle exceptions during test execution."""
    error_str = str(e).lower()
    ssl_in_error = 'ssl' in error_str or 'certificate' in error_str or 'pem' in error_str
    
    if expect_ssl_error and ssl_in_error:
        print_test("SSL error occurred (expected)", True, str(e)[:100])
        return True
    print_test("Test execution", False, str(e))
    return False


def run_ssl_tool_test(test_name: str, with_cert: bool = True, 
                      wrong_cert_path: str = None, expect_ssl_error: bool = False):
    """Run an MCP tool test with the given configuration."""
    print_header(test_name)
    
    cmd = build_docker_command(with_cert=with_cert, wrong_cert_path=wrong_cert_path)
    client = MCPClient(cmd)
    
    try:
        if not client.start():
            print_test("Docker container started", False)
            return False
        print_test("Docker container started", True)
        
        init_response = client.initialize()
        print_test("Session initialized", "result" in init_response, "Response received")
        
        tool_response = client.call_tool("code_health_score", {"file_path": "/mount/OrderProcessor.java"})
        result_text = extract_result_text(tool_response)
        print(f"  Result text: {result_text[:150]}...")
        
        return validate_ssl_response(result_text, tool_response, expect_ssl_error)
        
    except Exception as e:
        return handle_test_exception(e, expect_ssl_error)
    finally:
        client.stop()


def test_mcp_tool_invocation():
    """Test invoking an MCP tool with valid SSL cert."""
    return run_ssl_tool_test(
        "Test MCP Tool Invocation (with valid SSL cert)",
        with_cert=True,
        expect_ssl_error=False
    )


def test_mcp_tool_without_ssl_cert():
    """Test that CLI fails with SSL error when no cert is provided."""
    return run_ssl_tool_test(
        "Test MCP Tool Invocation (WITHOUT SSL cert - expect failure)",
        with_cert=False,
        expect_ssl_error=True
    )


def test_mcp_tool_with_wrong_cert():
    """Test that CLI fails with SSL error when wrong cert is provided."""
    wrong_cert_content = """-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJAKHBfpegPjMCMA0GCSqGSIb3DQEBCwUAMBExDzANBgNVBAMMBndy
b25nMTAeFw0yNDAxMDEwMDAwMDBaFw0yNTAxMDEwMDAwMDBaMBExDzANBgNVBAMM
BndyZeWuZzEwXDANBgkqhkiG9w0BAQEFAANLADBIAkEA0Z3VS5JJcds3xKFLEpzs
-----END CERTIFICATE-----
"""
    
    with tempfile.NamedTemporaryFile(mode='w', suffix='.pem', delete=False) as f:
        f.write(wrong_cert_content)
        wrong_cert_path = f.name
    
    try:
        return run_ssl_tool_test(
            "Test MCP Tool Invocation (with WRONG SSL cert - expect failure)",
            with_cert=True,
            wrong_cert_path=wrong_cert_path,
            expect_ssl_error=True
        )
    finally:
        try:
            os.unlink(wrong_cert_path)
        except Exception:
            pass


def main():
    print("\n" + "="*60)
    print("  Docker Run SSL Integration Tests")
    print("  Testing: docker run -i codescene-mcp with SSL certs")
    print("="*60)
    
    results = [
        ("Environment Setup", test_environment_setup()),
        ("Docker Run Startup", test_docker_run_starts()),
        ("MCP Tool Invocation (valid cert)", test_mcp_tool_invocation()),
        ("MCP Tool Invocation (no cert)", test_mcp_tool_without_ssl_cert()),
        ("MCP Tool Invocation (wrong cert)", test_mcp_tool_with_wrong_cert()),
    ]
    
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


if __name__ == '__main__':
    sys.exit(main())

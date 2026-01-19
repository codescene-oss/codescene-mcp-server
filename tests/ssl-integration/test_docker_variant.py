#!/usr/bin/env python3
"""
Docker variant SSL integration test.

This test runs inside a container that matches the actual MCP Docker deployment.
It verifies that:
1. The MCP server starts correctly
2. MCP protocol requests work via stdio
3. Tool invocations that use the CLI work with SSL
4. No SSL certificate errors occur

The test container has the same setup as the production Docker image:
- Same Python version and dependencies  
- Same CodeScene CLI installation
- Same MCP server code
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
    status = "✓ PASS" if passed else "✗ FAIL"
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
    # Try content array first
    content = result.get("content", [])
    if content and isinstance(content, list):
        return content[0].get("text", "")
    # Fall back to structuredContent
    structured = result.get("structuredContent", {})
    return structured.get("result", "")


def check_path_exists(path: str, name: str) -> bool:
    """Check if path exists and print test result."""
    exists = os.path.exists(path)
    print_test(f"{name} exists", exists, f"Path: {path}")
    return exists


def check_env_var(var_name: str, description: str) -> bool:
    """Check if environment variable is set and print test result."""
    value = os.getenv(var_name)
    if value:
        print_test(f"{description}", True, f"{var_name}: {value}")
        return True
    print_test(f"{description}", False)
    return False


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
            "clientInfo": {"name": "ssl-test-client", "version": "1.0.0"}
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


def test_environment_setup():
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    checks = [
        check_path_exists(os.getenv('REQUESTS_CA_BUNDLE', '/certs/ca.crt'), "CA certificate"),
        check_env_var('CS_ONPREM_URL', "CS_ONPREM_URL is set"),
        check_path_exists('/root/.local/bin/cs', "CS CLI"),
        check_path_exists('/mount/OrderProcessor.java', "Test file"),
        check_path_exists('/app/src/cs_mcp_server.py', "MCP server script"),
    ]
    return all(checks)


def test_ssl_args_generation():
    """Test that SSL args are correctly generated."""
    print_header("Test SSL Arguments Generation")
    
    sys.path.insert(0, '/app/src')
    from utils.platform_details import get_ssl_cli_args
    from utils.code_health_analysis import _is_cs_cli_command
    
    args = get_ssl_cli_args()
    checks = []
    
    # Check arg count
    checks.append(len(args) == 3)
    print_test("SSL args list has 3 elements", checks[-1], f"Got: {len(args)} args")
    
    # Check truststore file exists
    truststore_arg = next((a for a in args if '-Djavax.net.ssl.trustStore=' in a), None)
    if truststore_arg:
        truststore_path = truststore_arg.split('=', 1)[1]
        checks.append(os.path.exists(truststore_path))
        print_test("Truststore file created", checks[-1], f"Path: {truststore_path}")
    else:
        checks.append(False)
        print_test("Truststore arg present", False)
    
    # Check CLI detection
    cli_detected = _is_cs_cli_command('/root/.local/bin/cs')
    checks.append(cli_detected)
    print_test("CS CLI command detection works", cli_detected)
    
    return all(checks)


def test_cli_ssl_injection():
    """Test that SSL args are injected into CLI commands."""
    print_header("Test CLI SSL Args Injection")
    
    sys.path.insert(0, '/app/src')
    from unittest import mock
    from utils.code_health_analysis import run_local_tool
    
    captured_command = []
    
    def mock_run(command, **kwargs):
        captured_command.extend(command)
        result = mock.MagicMock()
        result.returncode = 0
        result.stdout = '{"score": 8.5}'
        return result
    
    with mock.patch('utils.code_health_analysis.subprocess.run', side_effect=mock_run):
        try:
            run_local_tool(['/root/.local/bin/cs', 'review', 'test.py', '--output-format=json'])
        except Exception:
            pass
    
    command_str = ' '.join(captured_command)
    checks = []
    
    checks.append('-Djavax.net.ssl.trustStore=' in command_str)
    print_test("SSL trustStore arg injected", checks[-1])
    
    checks.append('-Djavax.net.ssl.trustStoreType=PKCS12' in command_str)
    print_test("SSL trustStoreType arg injected", checks[-1])
    
    # Verify SSL args come before subcommand
    if len(captured_command) >= 5:
        ssl_indices = [i for i, arg in enumerate(captured_command) if '-Djavax.net.ssl' in arg]
        review_idx = next((i for i, arg in enumerate(captured_command) if arg == 'review'), -1)
        order_correct = ssl_indices and review_idx > 0 and all(idx < review_idx for idx in ssl_indices)
        checks.append(order_correct)
        print_test("SSL args come before subcommand", order_correct)
    
    return all(checks)


def test_mcp_server_startup():
    """Test that the MCP server starts and responds."""
    print_header("Test MCP Server Startup")
    
    command = ['python', '/app/src/cs_mcp_server.py']
    print(f"  Command: {' '.join(command)}")
    
    client = MCPClient(command)
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


def run_ssl_tool_test(test_name: str, env_modifier=None, expect_ssl_error=False):
    """
    Run an MCP tool test with the given environment configuration.
    
    Args:
        test_name: Name of the test for output
        env_modifier: Function to modify environment dict, or None for default
        expect_ssl_error: If True, expect SSL errors; if False, expect auth errors
    
    Returns:
        True if test passed, False otherwise
    """
    print_header(test_name)
    
    env = os.environ.copy()
    if env_modifier:
        env_modifier(env)
    
    command = ['python', '/app/src/cs_mcp_server.py']
    client = MCPClient(command, env=env)
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        print_test("MCP server started", True)
        
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


def test_mcp_tool_invocation():
    """Test invoking an MCP tool with valid SSL cert."""
    return run_ssl_tool_test(
        "Test MCP Tool Invocation (with valid SSL cert)",
        env_modifier=None,
        expect_ssl_error=False
    )


def test_mcp_tool_without_ssl_cert():
    """Test that CLI fails with SSL error when no cert is provided."""
    def remove_certs(env):
        env.pop('REQUESTS_CA_BUNDLE', None)
        env.pop('SSL_CERT_FILE', None)
        env.pop('CURL_CA_BUNDLE', None)
        env['CS_SSL_CERT_PATH'] = '/nonexistent/cert.pem'
    
    return run_ssl_tool_test(
        "Test MCP Tool Invocation (WITHOUT SSL cert - expect failure)",
        env_modifier=remove_certs,
        expect_ssl_error=True
    )


def test_mcp_tool_with_wrong_cert():
    """Test that CLI fails with SSL error when wrong cert is provided."""
    wrong_cert_content = """-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJAKHBfpegPjMCMA0GCSqGSIb3DQEBCwUAMBExDzANBgNVBAMMBndy
b25nMTAeFw0yNDAxMDEwMDAwMDBaFw0yNTAxMDEwMDAwMDBaMBExDzANBgNVBAMM
BndyZeWuZzEwXDANBgkqhkiG9w0BAQEFAANLADBIAkEA0Z3VS5JJcds3xKFLEpzs
TpGqT3gKH1234fakecertificatecontentABCDEFGHIJKLMNOPQRSTUVWXYZ
-----END CERTIFICATE-----
"""
    
    with tempfile.NamedTemporaryFile(mode='w', suffix='.pem', delete=False) as f:
        f.write(wrong_cert_content)
        wrong_cert_path = f.name
    
    def use_wrong_cert(env):
        env['REQUESTS_CA_BUNDLE'] = wrong_cert_path
        env['SSL_CERT_FILE'] = wrong_cert_path
    
    try:
        return run_ssl_tool_test(
            "Test MCP Tool Invocation (with WRONG SSL cert - expect failure)",
            env_modifier=use_wrong_cert,
            expect_ssl_error=True
        )
    finally:
        try:
            os.unlink(wrong_cert_path)
        except Exception:
            pass


def main():
    print("\n" + "="*60)
    print("  Docker Variant SSL Integration Test")
    print("  Testing: MCP Docker deployment with SSL certificates")
    print("="*60)
    print("\n  This test runs inside a container matching the Docker deployment")
    
    results = [
        ("Environment Setup", test_environment_setup()),
        ("SSL Args Generation", test_ssl_args_generation()),
        ("CLI SSL Args Injection", test_cli_ssl_injection()),
        ("MCP Server Startup", test_mcp_server_startup()),
        ("MCP Tool Invocation (valid cert)", test_mcp_tool_invocation()),
        ("MCP Tool Invocation (no cert)", test_mcp_tool_without_ssl_cert()),
        ("MCP Tool Invocation (wrong cert)", test_mcp_tool_with_wrong_cert()),
    ]
    
    print_header("Test Summary")
    
    passed = sum(1 for _, p in results if p)
    total = len(results)
    
    for name, result in results:
        print(f"  {'✓ PASS' if result else '✗ FAIL'}: {name}")
    
    print(f"\n  Total: {passed}/{total} passed")
    
    if passed == total:
        print("\n  Docker variant tests passed! ✓")
        return 0
    print(f"\n  {total - passed} test(s) failed! ✗")
    return 1


if __name__ == '__main__':
    sys.exit(main())

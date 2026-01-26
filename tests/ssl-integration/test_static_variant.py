#!/usr/bin/env python3
"""
Static binary variant SSL integration test.

This test:
1. Takes the cs-mcp binary path as argument
2. Runs it as a subprocess
3. Sends MCP protocol requests to invoke tools
4. Verifies SSL works end-to-end with valid cert
5. Verifies SSL fails without cert (negative test)
6. Verifies SSL fails with wrong cert (negative test)

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


def test_environment_setup(binary_path: str):
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    checks = []
    
    # Check binary
    binary_ok = os.path.exists(binary_path) and os.access(binary_path, os.X_OK)
    checks.append(binary_ok)
    print_test("cs-mcp binary exists and is executable", binary_ok, f"Path: {binary_path}")
    
    # Check CA bundle
    ca_bundle = os.getenv('REQUESTS_CA_BUNDLE')
    ca_ok = ca_bundle and os.path.exists(ca_bundle)
    checks.append(ca_ok)
    print_test("REQUESTS_CA_BUNDLE is set and file exists", ca_ok, f"Path: {ca_bundle}")
    
    # Check onprem URL
    onprem_url = os.getenv('CS_ONPREM_URL')
    url_ok = bool(onprem_url)
    checks.append(url_ok)
    print_test("CS_ONPREM_URL is set", url_ok, f"URL: {onprem_url}" if onprem_url else "")
    
    return all(checks)


def test_mcp_server_starts(binary_path: str):
    """Verify the MCP server starts successfully."""
    print_header("Test MCP Server Startup")
    
    print(f"  Binary: {binary_path}")
    client = MCPClient([binary_path])
    
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


def run_ssl_tool_test(binary_path: str, test_name: str, env_modifier=None, expect_ssl_error=False):
    """
    Run an MCP tool test with the given environment configuration.
    
    Args:
        binary_path: Path to cs-mcp binary
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
    
    client = MCPClient([binary_path], env=env)
    test_file = "/tmp/test_ssl.py"
    
    try:
        # Create test file
        with open(test_file, "w", encoding="utf-8") as f:
            f.write("def hello():\n    print('Hello')\n")
        
        if not client.start():
            print_test("MCP server started", False)
            return False
        print_test("MCP server started", True)
        
        init_response = client.initialize()
        print_test("Session initialized", "result" in init_response, "Response received")
        
        tool_response = client.call_tool("code_health_score", {"file_path": test_file})
        result_text = extract_result_text(tool_response)
        print(f"  Result text: {result_text[:150]}...")
        
        return validate_ssl_response(result_text, tool_response, expect_ssl_error)
        
    except Exception as e:
        return handle_test_exception(e, expect_ssl_error)
    finally:
        client.stop()
        try:
            os.unlink(test_file)
        except Exception:
            pass


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


def test_mcp_tool_invocation(binary_path: str):
    """Test invoking an MCP tool with valid SSL cert."""
    return run_ssl_tool_test(
        binary_path,
        "Test MCP Tool Invocation (with valid SSL cert)",
        env_modifier=None,
        expect_ssl_error=False
    )


def test_mcp_tool_without_ssl_cert(binary_path: str):
    """Test that CLI fails with SSL error when no cert is provided."""
    def remove_certs(env):
        env.pop('REQUESTS_CA_BUNDLE', None)
        env.pop('SSL_CERT_FILE', None)
        env.pop('CURL_CA_BUNDLE', None)
        env['CS_SSL_CERT_PATH'] = '/nonexistent/cert.pem'
    
    return run_ssl_tool_test(
        binary_path,
        "Test MCP Tool Invocation (WITHOUT SSL cert - expect failure)",
        env_modifier=remove_certs,
        expect_ssl_error=True
    )


def test_mcp_tool_with_wrong_cert(binary_path: str):
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
            binary_path,
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
    if len(sys.argv) < 2:
        print("Usage: python test_static_variant.py /path/to/cs-mcp")
        return 1
    
    binary_path = sys.argv[1]
    
    print("\n" + "="*60)
    print("  Static Binary SSL Integration Tests")
    print("  Testing: cs-mcp binary with embedded CLI")
    print("="*60)
    
    results = [
        ("Environment Setup", test_environment_setup(binary_path)),
        ("MCP Server Startup", test_mcp_server_starts(binary_path)),
        ("MCP Tool Invocation (valid cert)", test_mcp_tool_invocation(binary_path)),
        ("MCP Tool Invocation (no cert)", test_mcp_tool_without_ssl_cert(binary_path)),
        ("MCP Tool Invocation (wrong cert)", test_mcp_tool_with_wrong_cert(binary_path)),
    ]
    
    print_header("Test Summary")
    
    passed = sum(1 for _, p in results if p)
    total = len(results)
    
    for name, result in results:
        print(f"  {'✓ PASS' if result else '✗ FAIL'}: {name}")
    
    print(f"\n  Total: {passed}/{total} passed")
    
    if passed == total:
        print("\n  All tests passed! ✓")
        return 0
    print(f"\n  {total - passed} test(s) failed! ✗")
    return 1


if __name__ == '__main__':
    sys.exit(main())

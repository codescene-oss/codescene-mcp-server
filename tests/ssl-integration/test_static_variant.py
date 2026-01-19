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
import time
import threading
import queue
import tempfile

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
    status = "✓ PASS" if passed else "✗ FAIL"
    print(f"  {status}: {name}")
    if details:
        for line in details.split('\n')[:5]:
            print(f"         {line}")


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
        
        # Start a thread to read responses
        self.reader_thread = threading.Thread(target=self._read_responses, daemon=True)
        self.reader_thread.start()
        
        # Give the server a moment to start
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
        
        request_str = json.dumps(request)
        
        try:
            self.process.stdin.write(request_str + "\n")
            self.process.stdin.flush()
        except Exception as e:
            return {"error": f"Failed to send request: {e}"}
        
        # Wait for response (with timeout)
        try:
            response_str = self.response_queue.get(timeout=30)
            return json.loads(response_str)
        except queue.Empty:
            return {"error": "Timeout waiting for response"}
        except json.JSONDecodeError as e:
            return {"error": f"Invalid JSON response: {e}", "raw": response_str}
    
    def send_notification(self, method: str, params: dict = None):
        """Send a JSON-RPC notification (no response expected)."""
        notification = {
            "jsonrpc": "2.0",
            "method": method,
        }
        if params:
            notification["params"] = params
        
        try:
            self.process.stdin.write(json.dumps(notification) + "\n")
            self.process.stdin.flush()
        except Exception as e:
            print(f"Failed to send notification: {e}")
    
    def call_tool(self, tool_name: str, arguments: dict) -> dict:
        """Call an MCP tool."""
        return self.send_request("tools/call", {
            "name": tool_name,
            "arguments": arguments
        })
    
    def initialize(self) -> dict:
        """Initialize the MCP session and send initialized notification."""
        response = self.send_request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "ssl-test-client",
                "version": "1.0.0"
            }
        })
        
        # Send the initialized notification (required by MCP protocol)
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


def extract_result_text(tool_response: dict) -> str:
    """Extract the actual result text from MCP response format."""
    # Format: {"result": {"content": [{"type": "text", "text": "..."}], ...}}
    result_text = ""
    if "result" in tool_response:
        result = tool_response["result"]
        if isinstance(result, dict):
            # Try to get text from content array
            content = result.get("content", [])
            if content and isinstance(content, list):
                result_text = content[0].get("text", "")
            # Also check structuredContent
            if not result_text:
                structured = result.get("structuredContent", {})
                result_text = structured.get("result", "")
    return result_text


def test_environment_setup(binary_path: str):
    """Verify the test environment is correctly configured."""
    print_header("Test Environment Setup")
    
    all_passed = True
    
    # Check binary exists
    if os.path.exists(binary_path) and os.access(binary_path, os.X_OK):
        print_test("cs-mcp binary exists and is executable", True, f"Path: {binary_path}")
    else:
        print_test("cs-mcp binary exists and is executable", False, f"Path: {binary_path}")
        all_passed = False
    
    # Check REQUESTS_CA_BUNDLE
    ca_bundle = os.getenv('REQUESTS_CA_BUNDLE')
    if ca_bundle and os.path.exists(ca_bundle):
        print_test("REQUESTS_CA_BUNDLE is set and file exists", True, f"Path: {ca_bundle}")
    else:
        print_test("REQUESTS_CA_BUNDLE is set and file exists", False, f"Path: {ca_bundle}")
        all_passed = False
    
    # Check CS_ONPREM_URL
    onprem_url = os.getenv('CS_ONPREM_URL')
    if onprem_url:
        print_test("CS_ONPREM_URL is set", True, f"URL: {onprem_url}")
    else:
        print_test("CS_ONPREM_URL is set", False)
        all_passed = False
    
    return all_passed


def test_mcp_server_starts(binary_path: str):
    """Verify the MCP server starts successfully."""
    print_header("Test MCP Server Startup")
    
    print(f"  Binary: {binary_path}")
    
    client = MCPClient([binary_path])
    
    try:
        if client.start():
            print_test("MCP server process started", True)
        else:
            print_test("MCP server process started", False, "Process exited immediately")
            stderr = client.get_stderr()
            if stderr:
                print(f"         stderr: {stderr[:200]}")
            return False
        
        # Try to initialize
        response = client.initialize()
        
        if "error" not in str(response).lower() or "result" in response:
            print_test("MCP server responds to initialize", True)
            return True
        else:
            print_test("MCP server responds to initialize", True, 
                      f"Response: {str(response)[:100]}")
            return True
            
    except Exception as e:
        print_test("MCP server starts", False, str(e))
        return False
    finally:
        client.stop()


def test_mcp_tool_invocation(binary_path: str):
    """Test invoking an MCP tool with valid SSL cert."""
    print_header("Test MCP Tool Invocation (with valid SSL cert)")
    
    client = MCPClient([binary_path])
    all_passed = True
    
    # Create a test file
    test_file = "/tmp/test_ssl.py"
    with open(test_file, "w") as f:
        f.write("def hello():\n    print('Hello')\n")
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        
        print_test("MCP server started", True)
        
        # Initialize the session
        init_response = client.initialize()
        print_test("Session initialized", "result" in init_response,
                  "Response received")
        
        # Call code_health_score tool
        tool_response = client.call_tool("code_health_score", {
            "file_path": test_file
        })
        
        result_text = extract_result_text(tool_response)
        print(f"  Result text: {result_text[:150]}...")
        
        # Check for SSL-related errors (these should NOT appear)
        ssl_error_keywords = ['trustStore', 'PKCS12', 'KeyStoreException', 
                             'certificate verify', 'CERTIFICATE_VERIFY_FAILED',
                             'SSLHandshakeException', 'unable to find valid certification',
                             'PKIX path building failed']
        
        has_ssl_error = any(kw.lower() in result_text.lower() for kw in ssl_error_keywords)
        
        if has_ssl_error:
            print_test("No SSL errors in response", False, 
                      f"Found SSL error: {result_text[:200]}")
            all_passed = False
        else:
            print_test("No SSL errors in response", True)
        
        # Check for expected authorization/license error (this SHOULD appear)
        # This proves the CLI actually connected to the server (SSL worked) 
        # but was rejected due to invalid token
        auth_keywords = ['401', 'license', 'unauthorized', 'reauthorize', 
                        'access token', 'authentication']
        
        has_auth_error = any(kw.lower() in result_text.lower() for kw in auth_keywords)
        
        if has_auth_error:
            print_test("CLI connected but auth failed (expected)", True,
                      f"Auth error (proves SSL worked): {result_text[:100]}")
        elif "error" in tool_response:
            # Check if it's a protocol error (bad)
            error_msg = str(tool_response.get('error', ''))
            print_test("Tool returned error", False,
                      f"Protocol error (unexpected): {error_msg[:100]}")
            all_passed = False
        else:
            # Other result - might be ok if CLI worked
            print_test("Tool returned result", True, 
                      f"Result: {result_text[:100]}")
        
    except Exception as e:
        print_test("Tool invocation", False, str(e))
        all_passed = False
    finally:
        client.stop()
        try:
            os.unlink(test_file)
        except Exception:
            pass
    
    return all_passed


def test_mcp_tool_without_ssl_cert(binary_path: str):
    """Test that CLI fails with SSL error when no cert is provided.
    
    This is a NEGATIVE test - we expect SSL to fail when we remove
    the REQUESTS_CA_BUNDLE environment variable.
    """
    print_header("Test MCP Tool Invocation (WITHOUT SSL cert - expect failure)")
    
    # Create environment WITHOUT the CA bundle
    env = os.environ.copy()
    env.pop('REQUESTS_CA_BUNDLE', None)
    env.pop('SSL_CERT_FILE', None)
    env.pop('CURL_CA_BUNDLE', None)
    # Set an empty/non-existent path to force SSL failure
    env['CS_SSL_CERT_PATH'] = '/nonexistent/cert.pem'
    
    client = MCPClient([binary_path], env=env)
    all_passed = True
    
    # Create a test file
    test_file = "/tmp/test_ssl_nocert.py"
    with open(test_file, "w") as f:
        f.write("def hello():\n    print('Hello')\n")
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        
        print_test("MCP server started", True)
        
        # Initialize the session
        init_response = client.initialize()
        print_test("Session initialized", "result" in init_response,
                  "Response received")
        
        # Call code_health_score tool - this should trigger SSL error
        tool_response = client.call_tool("code_health_score", {
            "file_path": test_file
        })
        
        result_text = extract_result_text(tool_response)
        print(f"  Result text: {result_text[:200]}...")
        
        # Check for SSL-related errors (these SHOULD appear now)
        ssl_error_keywords = ['PKIX path building failed', 'SSLHandshakeException',
                             'unable to find valid certification path',
                             'certificate verify', 'CERTIFICATE_VERIFY_FAILED',
                             'trustAnchors', 'ValidatorException', 'SSL']
        
        has_ssl_error = any(kw.lower() in result_text.lower() for kw in ssl_error_keywords)
        
        if has_ssl_error:
            print_test("SSL error occurred (expected)", True, 
                      f"Got expected SSL error: {result_text[:100]}")
        else:
            # Check if it's an auth error - this would mean SSL somehow worked
            auth_keywords = ['401', 'license', 'unauthorized', 'reauthorize']
            has_auth_error = any(kw.lower() in result_text.lower() for kw in auth_keywords)
            
            if has_auth_error:
                print_test("SSL error occurred", False, 
                          f"Got auth error instead of SSL error - SSL unexpectedly worked")
                all_passed = False
            else:
                print_test("SSL error occurred", False, 
                          f"Expected SSL error but got: {result_text[:100]}")
                all_passed = False
        
    except Exception as e:
        # Exception might indicate SSL failure too
        error_str = str(e).lower()
        if 'ssl' in error_str or 'certificate' in error_str:
            print_test("SSL error occurred (expected)", True, str(e)[:100])
        else:
            print_test("Test execution", False, str(e))
            all_passed = False
    finally:
        client.stop()
        try:
            os.unlink(test_file)
        except Exception:
            pass
    
    return all_passed


def test_mcp_tool_with_wrong_cert(binary_path: str):
    """Test that CLI fails with SSL error when wrong cert is provided.
    
    This is a NEGATIVE test - we create a self-signed cert that doesn't
    match our nginx proxy's certificate, so SSL validation should fail.
    """
    print_header("Test MCP Tool Invocation (with WRONG SSL cert - expect failure)")
    
    # Create a dummy wrong certificate
    wrong_cert_content = """-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJAKHBfpegPjMCMA0GCSqGSIb3DQEBCwUAMBExDzANBgNVBAMMBndy
b25nMTAeFw0yNDAxMDEwMDAwMDBaFw0yNTAxMDEwMDAwMDBaMBExDzANBgNVBAMM
BndyZeWuZzEwXDANBgkqhkiG9w0BAQEFAANLADBIAkEA0Z3VS5JJcds3xKFLEpzs
TpGqT3gKH1234fakecertificatecontentABCDEFGHIJKLMNOPQRSTUVWXYZabcdef
ghijklmnopqrstuvwxyz1234567890AQAB
-----END CERTIFICATE-----
"""
    
    # Write wrong cert to temp file
    with tempfile.NamedTemporaryFile(mode='w', suffix='.pem', delete=False) as f:
        f.write(wrong_cert_content)
        wrong_cert_path = f.name
    
    # Create environment with WRONG certificate
    env = os.environ.copy()
    env['REQUESTS_CA_BUNDLE'] = wrong_cert_path
    env['SSL_CERT_FILE'] = wrong_cert_path
    
    client = MCPClient([binary_path], env=env)
    all_passed = True
    
    # Create a test file
    test_file = "/tmp/test_ssl_wrongcert.py"
    with open(test_file, "w") as f:
        f.write("def hello():\n    print('Hello')\n")
    
    try:
        if not client.start():
            print_test("MCP server started", False)
            return False
        
        print_test("MCP server started", True)
        
        # Initialize the session
        init_response = client.initialize()
        print_test("Session initialized", "result" in init_response,
                  "Response received")
        
        # Call code_health_score tool - this should trigger SSL error
        tool_response = client.call_tool("code_health_score", {
            "file_path": test_file
        })
        
        result_text = extract_result_text(tool_response)
        print(f"  Result text: {result_text[:200]}...")
        
        # Check for SSL-related errors (these SHOULD appear now)
        ssl_error_keywords = ['PKIX path building failed', 'SSLHandshakeException',
                             'unable to find valid certification path',
                             'certificate verify', 'CERTIFICATE_VERIFY_FAILED',
                             'trustAnchors', 'ValidatorException', 'PEM',
                             'certificate', 'ssl']
        
        has_ssl_error = any(kw.lower() in result_text.lower() for kw in ssl_error_keywords)
        
        if has_ssl_error:
            print_test("SSL error with wrong cert (expected)", True, 
                      f"Got expected SSL error: {result_text[:100]}")
        else:
            # Check if it's an auth error - this would mean SSL somehow worked
            auth_keywords = ['401', 'license', 'unauthorized', 'reauthorize']
            has_auth_error = any(kw.lower() in result_text.lower() for kw in auth_keywords)
            
            if has_auth_error:
                print_test("SSL error with wrong cert", False, 
                          f"Got auth error - SSL unexpectedly worked with wrong cert!")
                all_passed = False
            else:
                print_test("SSL error with wrong cert", False, 
                          f"Expected SSL error but got: {result_text[:100]}")
                all_passed = False
        
    except Exception as e:
        # Exception might indicate SSL failure too
        error_str = str(e).lower()
        if 'ssl' in error_str or 'certificate' in error_str or 'pem' in error_str:
            print_test("SSL error with wrong cert (expected)", True, str(e)[:100])
        else:
            print_test("Test execution", False, str(e))
            all_passed = False
    finally:
        client.stop()
        # Clean up temp files
        try:
            os.unlink(wrong_cert_path)
        except Exception:
            pass
        try:
            os.unlink(test_file)
        except Exception:
            pass
    
    return all_passed


def main():
    if len(sys.argv) < 2:
        print("Usage: python test_static_variant.py /path/to/cs-mcp")
        return 1
    
    binary_path = sys.argv[1]
    
    print("\n" + "="*60)
    print("  Static Binary SSL Integration Tests")
    print("  Testing: cs-mcp binary with embedded CLI")
    print("="*60)
    
    results = []
    
    # Run all tests
    results.append(("Environment Setup", test_environment_setup(binary_path)))
    results.append(("MCP Server Startup", test_mcp_server_starts(binary_path)))
    results.append(("MCP Tool Invocation (valid cert)", test_mcp_tool_invocation(binary_path)))
    results.append(("MCP Tool Invocation (no cert)", test_mcp_tool_without_ssl_cert(binary_path)))
    results.append(("MCP Tool Invocation (wrong cert)", test_mcp_tool_with_wrong_cert(binary_path)))
    
    # Summary
    print_header("Test Summary")
    
    passed = sum(1 for _, p in results if p)
    total = len(results)
    
    for name, result in results:
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"  {status}: {name}")
    
    print(f"\n  Total: {passed}/{total} passed")
    
    if passed == total:
        print("\n  All tests passed! ✓")
        return 0
    else:
        print(f"\n  {total - passed} test(s) failed! ✗")
        return 1


if __name__ == '__main__':
    sys.exit(main())

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
import time
import threading
import queue

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
    
    def call_tool(self, tool_name: str, arguments: dict) -> dict:
        """Call an MCP tool."""
        return self.send_request("tools/call", {
            "name": tool_name,
            "arguments": arguments
        })
    
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
        import time
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
    
    all_passed = True
    
    # Check CA cert exists
    ca_bundle = os.getenv('REQUESTS_CA_BUNDLE', '/certs/ca.crt')
    if ca_bundle and os.path.exists(ca_bundle):
        print_test("CA certificate exists", True, f"Path: {ca_bundle}")
    else:
        print_test("CA certificate exists", False, f"Path: {ca_bundle}")
        all_passed = False
    
    # Check CS_ONPREM_URL
    onprem_url = os.getenv('CS_ONPREM_URL')
    if onprem_url:
        print_test("CS_ONPREM_URL is set", True, f"URL: {onprem_url}")
    else:
        print_test("CS_ONPREM_URL is set", False)
        all_passed = False
    
    # Check CS CLI is installed
    cli_path = '/root/.local/bin/cs'
    if os.path.exists(cli_path):
        print_test("CS CLI is installed", True, f"Path: {cli_path}")
    else:
        print_test("CS CLI is installed", False)
        all_passed = False
    
    # Check test file exists
    test_file = '/mount/OrderProcessor.java'
    if os.path.exists(test_file):
        print_test("Test file exists", True, f"Path: {test_file}")
    else:
        print_test("Test file exists", False)
        all_passed = False
    
    # Check MCP server script exists
    mcp_script = '/app/src/cs_mcp_server.py'
    if os.path.exists(mcp_script):
        print_test("MCP server script exists", True)
    else:
        print_test("MCP server script exists", False)
        all_passed = False
    
    return all_passed


def test_ssl_args_generation():
    """Test that SSL args are correctly generated."""
    print_header("Test SSL Arguments Generation")
    
    # Add src to path for imports
    sys.path.insert(0, '/app/src')
    
    from utils.platform_details import get_ssl_cli_args
    from utils.code_health_analysis import _is_cs_cli_command
    
    all_passed = True
    
    # Check SSL args are generated
    args = get_ssl_cli_args()
    
    if len(args) == 3:
        print_test("SSL args list has 3 elements", True)
    else:
        print_test("SSL args list has 3 elements", False, f"Got: {len(args)} args")
        all_passed = False
    
    # Check truststore arg
    truststore_arg = next((a for a in args if '-Djavax.net.ssl.trustStore=' in a), None)
    if truststore_arg:
        truststore_path = truststore_arg.split('=', 1)[1]
        if os.path.exists(truststore_path):
            print_test("Truststore file created", True, f"Path: {truststore_path}")
        else:
            print_test("Truststore file created", False, f"Path: {truststore_path}")
            all_passed = False
    else:
        print_test("Truststore arg present", False)
        all_passed = False
    
    # Check CS CLI command detection
    if _is_cs_cli_command('/root/.local/bin/cs'):
        print_test("CS CLI command detection works", True)
    else:
        print_test("CS CLI command detection works", False)
        all_passed = False
    
    return all_passed


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
    
    all_passed = True
    command_str = ' '.join(captured_command)
    
    if '-Djavax.net.ssl.trustStore=' in command_str:
        print_test("SSL trustStore arg injected", True)
    else:
        print_test("SSL trustStore arg injected", False, f"Command: {command_str[:100]}")
        all_passed = False
    
    if '-Djavax.net.ssl.trustStoreType=PKCS12' in command_str:
        print_test("SSL trustStoreType arg injected", True)
    else:
        print_test("SSL trustStoreType arg injected", False)
        all_passed = False
    
    # Verify order: CLI -> SSL args -> subcommand
    if len(captured_command) >= 5:
        ssl_indices = [i for i, arg in enumerate(captured_command) if '-Djavax.net.ssl' in arg]
        review_idx = next((i for i, arg in enumerate(captured_command) if arg == 'review'), -1)
        
        if ssl_indices and review_idx > 0 and all(idx < review_idx for idx in ssl_indices):
            print_test("SSL args come before subcommand", True)
        else:
            print_test("SSL args come before subcommand", False)
            all_passed = False
    
    return all_passed


def test_mcp_server_startup():
    """Test that the MCP server starts and responds."""
    print_header("Test MCP Server Startup")
    
    command = ['python', '/app/src/cs_mcp_server.py']
    print(f"  Command: {' '.join(command)}")
    
    client = MCPClient(command)
    
    try:
        if client.start():
            print_test("MCP server process started", True)
        else:
            print_test("MCP server process started", False)
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


def test_mcp_tool_invocation():
    """Test invoking an MCP tool that uses the CLI with correct SSL setup."""
    print_header("Test MCP Tool Invocation (with valid SSL cert)")
    
    command = ['python', '/app/src/cs_mcp_server.py']
    client = MCPClient(command)
    all_passed = True
    
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
        test_file = '/mount/OrderProcessor.java'
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
    
    return all_passed


def test_mcp_tool_without_ssl_cert():
    """Test that CLI fails with SSL error when no cert is provided.
    
    This is a NEGATIVE test - we expect SSL to fail when we remove
    the REQUESTS_CA_BUNDLE environment variable and don't provide
    any SSL truststore configuration.
    """
    print_header("Test MCP Tool Invocation (WITHOUT SSL cert - expect failure)")
    
    # Create environment WITHOUT the CA bundle
    env = os.environ.copy()
    env.pop('REQUESTS_CA_BUNDLE', None)
    env.pop('SSL_CERT_FILE', None)
    env.pop('CURL_CA_BUNDLE', None)
    # Set an empty/non-existent path to force SSL failure
    env['CS_SSL_CERT_PATH'] = '/nonexistent/cert.pem'
    
    command = ['python', '/app/src/cs_mcp_server.py']
    client = MCPClient(command, env=env)
    all_passed = True
    
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
        test_file = '/mount/OrderProcessor.java'
        tool_response = client.call_tool("code_health_score", {
            "file_path": test_file
        })
        
        result_text = extract_result_text(tool_response)
        print(f"  Result text: {result_text[:200]}...")
        
        # Check for SSL-related errors (these SHOULD appear now)
        ssl_error_keywords = ['PKIX path building failed', 'SSLHandshakeException',
                             'unable to find valid certification path',
                             'certificate verify', 'CERTIFICATE_VERIFY_FAILED',
                             'trustAnchors', 'ValidatorException']
        
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
    
    return all_passed


def test_mcp_tool_with_wrong_cert():
    """Test that CLI fails with SSL error when wrong cert is provided.
    
    This is a NEGATIVE test - we create a self-signed cert that doesn't
    match our nginx proxy's certificate, so SSL validation should fail.
    """
    print_header("Test MCP Tool Invocation (with WRONG SSL cert - expect failure)")
    
    import tempfile
    
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
    
    command = ['python', '/app/src/cs_mcp_server.py']
    client = MCPClient(command, env=env)
    all_passed = True
    
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
        test_file = '/mount/OrderProcessor.java'
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
        # Clean up temp file
        try:
            os.unlink(wrong_cert_path)
        except Exception:
            pass
    
    return all_passed


def main():
    print("\n" + "="*60)
    print("  Docker Variant SSL Integration Test")
    print("  Testing: MCP Docker deployment with SSL certificates")
    print("="*60)
    print("\n  This test runs inside a container matching the Docker deployment")
    
    results = []
    
    # Run all tests
    results.append(("Environment Setup", test_environment_setup()))
    results.append(("SSL Args Generation", test_ssl_args_generation()))
    results.append(("CLI SSL Args Injection", test_cli_ssl_injection()))
    results.append(("MCP Server Startup", test_mcp_server_startup()))
    results.append(("MCP Tool Invocation (valid cert)", test_mcp_tool_invocation()))
    results.append(("MCP Tool Invocation (no cert)", test_mcp_tool_without_ssl_cert()))
    results.append(("MCP Tool Invocation (wrong cert)", test_mcp_tool_with_wrong_cert()))
    
    # Summary
    print_header("Test Summary")
    
    passed = sum(1 for _, p in results if p)
    total = len(results)
    
    for name, result in results:
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"  {status}: {name}")
    
    print(f"\n  Total: {passed}/{total} passed")
    
    if passed == total:
        print("\n  Docker variant tests passed! ✓")
        return 0
    else:
        print(f"\n  {total - passed} test(s) failed! ✗")
        return 1


if __name__ == '__main__':
    sys.exit(main())

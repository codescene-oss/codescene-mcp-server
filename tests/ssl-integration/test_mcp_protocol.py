#!/usr/bin/env python3
"""Test script to understand MCP protocol requirements."""

import subprocess
import json
import select
import time
import sys


def create_mcp_process():
    """Start the MCP server process."""
    return subprocess.Popen(
        ['python', 'src/cs_mcp_server.py'],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        cwd='/Users/asko/Code/CodeScene/mcp'
    )


def send_request(proc, req):
    """Send a JSON-RPC request to the MCP server."""
    req_str = json.dumps(req)
    print(f">>> Sending: {req_str[:100]}...")
    proc.stdin.write(req_str + '\n')
    proc.stdin.flush()


def receive_response(proc, timeout=10):
    """Wait for and receive a response from the MCP server."""
    time.sleep(0.5)
    if select.select([proc.stdout], [], [], timeout)[0]:
        line = proc.stdout.readline().strip()
        print(f"<<< Received: {line[:200]}...")
        return line
    print("<<< No response")
    return None


def send_and_recv(proc, req, wait_response=True):
    """Send a request and optionally wait for response."""
    send_request(proc, req)
    if not wait_response:
        return None
    return receive_response(proc)


def initialize_session(proc):
    """Send initialize request and initialized notification."""
    init = send_and_recv(proc, {
        'jsonrpc': '2.0',
        'id': 1,
        'method': 'initialize',
        'params': {
            'protocolVersion': '2024-11-05',
            'capabilities': {},
            'clientInfo': {'name': 'test', 'version': '1.0'}
        }
    })

    send_and_recv(proc, {
        'jsonrpc': '2.0',
        'method': 'notifications/initialized'
    }, wait_response=False)
    print("Sent initialized notification")
    time.sleep(0.5)
    return init


def print_tools_summary(tools_resp):
    """Parse and print summary of available tools."""
    if not tools_resp:
        return
    data = json.loads(tools_resp)
    if 'result' not in data or 'tools' not in data['result']:
        return
    tools = data['result']['tools']
    print(f"\nFound {len(tools)} tools:")
    for tool in tools[:3]:
        print(f"  - {tool['name']}")
        if 'inputSchema' in tool:
            print(f"    Schema: {json.dumps(tool['inputSchema'])[:100]}...")


def test_code_health_call(proc):
    """Test calling the code_health_score tool."""
    print("\n--- Testing tools/call ---")
    call_resp = send_and_recv(proc, {
        'jsonrpc': '2.0',
        'id': 3,
        'method': 'tools/call',
        'params': {
            'name': 'code_health_score',
            'arguments': {
                'file_path': '/Users/asko/Code/CodeScene/mcp/src/test_data/OrderProcessor.java'
            }
        }
    })
    if call_resp:
        print(f"\nFull response: {call_resp}")


def main():
    proc = create_mcp_process()

    try:
        initialize_session(proc)

        tools_resp = send_and_recv(proc, {
            'jsonrpc': '2.0',
            'id': 2,
            'method': 'tools/list',
            'params': {}
        })
        print_tools_summary(tools_resp)

        test_code_health_call(proc)

    finally:
        proc.terminate()
        stderr = proc.stderr.read()
        if stderr:
            print(f"\nStderr: {stderr[:500]}")


if __name__ == '__main__':
    main()

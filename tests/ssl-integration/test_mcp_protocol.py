#!/usr/bin/env python3
"""Test script to understand MCP protocol requirements."""

import subprocess
import json
import select
import time
import sys

def main():
    proc = subprocess.Popen(
        ['python', 'src/cs_mcp_server.py'],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        cwd='/Users/asko/Code/CodeScene/mcp'
    )

    def send_and_recv(req, wait_response=True):
        req_str = json.dumps(req)
        print(f">>> Sending: {req_str[:100]}...")
        proc.stdin.write(req_str + '\n')
        proc.stdin.flush()
        if not wait_response:
            return None
        time.sleep(0.5)
        if select.select([proc.stdout], [], [], 10)[0]:
            line = proc.stdout.readline().strip()
            print(f"<<< Received: {line[:200]}...")
            return line
        print("<<< No response")
        return None

    try:
        # Initialize
        init = send_and_recv({
            'jsonrpc': '2.0',
            'id': 1,
            'method': 'initialize',
            'params': {
                'protocolVersion': '2024-11-05',
                'capabilities': {},
                'clientInfo': {'name': 'test', 'version': '1.0'}
            }
        })

        # Send initialized notification
        send_and_recv({
            'jsonrpc': '2.0',
            'method': 'notifications/initialized'
        }, wait_response=False)
        print("Sent initialized notification")
        time.sleep(0.5)

        # List tools
        tools_resp = send_and_recv({
            'jsonrpc': '2.0',
            'id': 2,
            'method': 'tools/list',
            'params': {}
        })
        
        if tools_resp:
            data = json.loads(tools_resp)
            if 'result' in data and 'tools' in data['result']:
                print(f"\nFound {len(data['result']['tools'])} tools:")
                for tool in data['result']['tools'][:3]:
                    print(f"  - {tool['name']}")
                    if 'inputSchema' in tool:
                        print(f"    Schema: {json.dumps(tool['inputSchema'])[:100]}...")

        # Call code_health_score with proper params
        print("\n--- Testing tools/call ---")
        call_resp = send_and_recv({
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

    finally:
        proc.terminate()
        # Check stderr
        stderr = proc.stderr.read()
        if stderr:
            print(f"\nStderr: {stderr[:500]}")

if __name__ == '__main__':
    main()

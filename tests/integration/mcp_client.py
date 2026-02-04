#!/usr/bin/env python3
"""
MCP Client for integration tests.

This module provides an MCPClient class for communicating with MCP servers
via stdio using the JSON-RPC protocol.
"""

import json
import os
import queue
import subprocess
import threading
import time
from typing import Optional

# MCP Protocol message IDs
_msg_id = 0


def next_msg_id() -> int:
    """Get the next message ID for MCP requests."""
    global _msg_id
    _msg_id += 1
    return _msg_id


class MCPClient:
    """MCP client that communicates with the server via stdio."""
    
    def __init__(self, command: list[str], env: Optional[dict[str, str]] = None, cwd: Optional[str] = None):
        """
        Initialize MCP client.
        
        Args:
            command: Command to start the MCP server
            env: Environment variables (defaults to current environment)
            cwd: Working directory for the process
        """
        self.command = command
        self.env = env or os.environ.copy()
        self.cwd = cwd
        self.process: Optional[subprocess.Popen] = None
        self.response_queue: queue.Queue = queue.Queue()
        self.reader_thread: Optional[threading.Thread] = None
        self.stderr_lines: list[str] = []
        
    def start(self) -> bool:
        """
        Start the MCP server process.
        
        Returns:
            True if server started successfully
        """
        try:
            self.process = subprocess.Popen(
                self.command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=self.env,
                cwd=self.cwd,
                text=True,
                encoding="utf-8",
                bufsize=1
            )
            self.reader_thread = threading.Thread(target=self._read_responses, daemon=True)
            self.reader_thread.start()
            self.stderr_thread = threading.Thread(target=self._read_stderr, daemon=True)
            self.stderr_thread.start()
            time.sleep(1)
            return self.process.poll() is None
        except Exception as e:
            print(f"Failed to start MCP server: {e}")
            return False
    
    def _read_responses(self) -> None:
        """Read responses from the server in a background thread."""
        try:
            while self.process and self.process.poll() is None:
                line = self.process.stdout.readline()
                if line:
                    self.response_queue.put(line.strip())
        except Exception as e:
            self.response_queue.put(f"ERROR: {e}")
    
    def _read_stderr(self) -> None:
        """Read stderr from the server in a background thread."""
        try:
            while self.process and self.process.poll() is None:
                line = self.process.stderr.readline()
                if line:
                    self.stderr_lines.append(line.strip())
        except Exception:
            pass
    
    def send_request(self, method: str, params: Optional[dict] = None, timeout: int = 30) -> dict:
        """
        Send a JSON-RPC request and wait for response.
        
        Args:
            method: The JSON-RPC method to call
            params: Parameters for the method
            timeout: Timeout in seconds
            
        Returns:
            Response dictionary
        """
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
            response_str = self.response_queue.get(timeout=timeout)
            return json.loads(response_str)
        except queue.Empty:
            stderr = "\n".join(self.stderr_lines[-10:]) if self.stderr_lines else "No stderr"
            return {"error": f"Timeout waiting for response. Recent stderr:\n{stderr}"}
        except json.JSONDecodeError as e:
            return {"error": f"Invalid JSON response: {e}"}
    
    def send_notification(self, method: str, params: Optional[dict] = None) -> None:
        """Send a JSON-RPC notification (no response expected)."""
        notification = {"jsonrpc": "2.0", "method": method}
        if params:
            notification["params"] = params
        try:
            self.process.stdin.write(json.dumps(notification) + "\n")
            self.process.stdin.flush()
        except Exception as e:
            print(f"Failed to send notification: {e}")
    
    def call_tool(self, tool_name: str, arguments: dict, timeout: int = 30) -> dict:
        """
        Call an MCP tool.
        
        Args:
            tool_name: Name of the tool to call
            arguments: Arguments for the tool
            timeout: Timeout in seconds
            
        Returns:
            Tool response dictionary
        """
        return self.send_request("tools/call", {"name": tool_name, "arguments": arguments}, timeout=timeout)
    
    def initialize(self) -> dict:
        """Initialize the MCP session and send initialized notification."""
        response = self.send_request("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "integration-test-client", "version": "1.0.0"}
        })
        time.sleep(0.2)
        self.send_notification("notifications/initialized")
        time.sleep(0.3)
        return response
    
    def stop(self) -> None:
        """Stop the MCP server process."""
        if self.process:
            try:
                self.process.stdin.close()
                self.process.terminate()
                self.process.wait(timeout=5)
            except Exception:
                try:
                    self.process.kill()
                except Exception:
                    pass
    
    def get_stderr(self) -> str:
        """Get stderr output from the server."""
        return "\n".join(self.stderr_lines)

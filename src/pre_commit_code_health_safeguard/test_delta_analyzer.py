import json
import os
import unittest
from unittest import mock
from fastmcp import FastMCP
from .delta_analyzer import PreCommitCodeHealthSafeguard


class TestPreCommitCodeHealthSafeguard(unittest.TestCase):
    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/my/git/path"})
    def test_pre_commit_code_health_safeguard(self):
        def mock_run_local_tool(cli_command, path, extra_env=None):
            return json.dumps([{
                'name': 'test.tsx'
            }])

        self.instance = PreCommitCodeHealthSafeguard(FastMCP("Test"), {
            'run_local_tool_fn': mock_run_local_tool
        })

        expected = {
            "results": [
                {"name": "test.tsx", "verdict": "unknown", "findings": []}
            ],
            "quality_gates": "passed"
        }

        result = self.instance.pre_commit_code_health_safeguard("/my/git/path")

        self.assertEqual(json.dumps(expected), result)

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/my/git/path"})
    def test_pre_commit_code_health_safeguard_invalid_response(self):
        def mock_run_local_tool(cli_command, path, extra_env=None):
            return "string output"

        self.instance = PreCommitCodeHealthSafeguard(FastMCP("Test"), {
            'run_local_tool_fn': mock_run_local_tool
        })

        expected = """Error: Invalid JSON input: Expecting value: line 1 column 1 (char 0)
Input: string output"""
        result = self.instance.pre_commit_code_health_safeguard("/my/git/path")

        self.assertEqual(expected, result)

    def test_pre_commit_code_health_safeguard_local_binary(self):
        """Test that local/native binary mode works without CS_MOUNT_PATH."""
        # Ensure CS_MOUNT_PATH is NOT set (simulating native binary)
        os.environ.pop("CS_MOUNT_PATH", None)
        
        captured_path = []
        
        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_path.append(path)
            return json.dumps([{
                'name': 'test.tsx'
            }])

        self.instance = PreCommitCodeHealthSafeguard(FastMCP("Test"), {
            'run_local_tool_fn': mock_run_local_tool
        })

        expected = {
            "results": [
                {"name": "test.tsx", "verdict": "unknown", "findings": []}
            ],
            "quality_gates": "passed"
        }

        result = self.instance.pre_commit_code_health_safeguard("/my/local/git/path")

        self.assertEqual(json.dumps(expected), result)
        # Verify the path was passed directly without Docker translation
        self.assertEqual(captured_path[-1], "/my/local/git/path")

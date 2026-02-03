import json
import os
import tempfile
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


class TestPreCommitCodeHealthSafeguardWorktree(unittest.TestCase):
    """Tests for pre-commit safeguard git worktree support in static mode."""

    def setUp(self):
        self._env = dict(os.environ)
        # Create a temp worktree-like structure
        self.temp_dir = os.path.realpath(tempfile.mkdtemp())
        
        # Write .git file pointing to main repo's worktrees
        self.worktree_gitdir = '/path/to/main/.git/worktrees/feature'
        with open(os.path.join(self.temp_dir, '.git'), 'w') as f:
            f.write(f'gitdir: {self.worktree_gitdir}')

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)
        import shutil
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_safeguard_local_sets_git_dir_for_worktree(self):
        """Test that local safeguard sets GIT_DIR for worktree environments."""
        os.environ.pop("CS_MOUNT_PATH", None)
        
        captured_extra_env = []
        
        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_extra_env.append(extra_env)
            return json.dumps([{'name': 'test.tsx'}])

        instance = PreCommitCodeHealthSafeguard(FastMCP("Test"), {
            'run_local_tool_fn': mock_run_local_tool
        })

        instance.pre_commit_code_health_safeguard(self.temp_dir)

        # Verify GIT_DIR was passed in extra_env
        self.assertEqual(len(captured_extra_env), 1)
        extra_env = captured_extra_env[0]
        self.assertIsNotNone(extra_env)
        self.assertIn('GIT_DIR', extra_env)
        self.assertEqual(self.worktree_gitdir, extra_env['GIT_DIR'])

    def test_safeguard_local_no_extra_env_for_regular_repo(self):
        """Test that local safeguard doesn't set GIT_DIR for regular repos."""
        os.environ.pop("CS_MOUNT_PATH", None)
        
        # Create a regular repo with .git directory
        regular_repo = os.path.join(self.temp_dir, 'regular')
        os.makedirs(os.path.join(regular_repo, '.git'))
        
        captured_extra_env = []
        
        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_extra_env.append(extra_env)
            return json.dumps([{'name': 'test.tsx'}])

        instance = PreCommitCodeHealthSafeguard(FastMCP("Test"), {
            'run_local_tool_fn': mock_run_local_tool
        })

        instance.pre_commit_code_health_safeguard(regular_repo)

        # Verify extra_env was None (no GIT_DIR needed)
        self.assertEqual(len(captured_extra_env), 1)
        self.assertIsNone(captured_extra_env[0])

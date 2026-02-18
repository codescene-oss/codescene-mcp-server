import json
import os
import tempfile
import unittest
from unittest import mock

from fastmcp import FastMCP

from .change_set_analyzer import AnalyzeChangeSet


class TestAnalyzeChangeSet(unittest.TestCase):
    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/my/git/path"})
    def test_analyze_change_set_docker(self):
        def mock_run_local_tool(cli_command, path, extra_env=None):
            return json.dumps([{"name": "test.tsx"}])

        self.instance = AnalyzeChangeSet(FastMCP("Test"), {"run_local_tool_fn": mock_run_local_tool})

        expected = {
            "results": [{"name": "test.tsx", "verdict": "unknown", "findings": []}],
            "quality_gates": "passed",
        }

        result = self.instance.analyze_change_set("main", "/my/git/path")

        self.assertEqual(json.dumps(expected), result)

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/my/git/path"})
    def test_analyze_change_set_invalid_response(self):
        def mock_run_local_tool(cli_command, path, extra_env=None):
            return "string output"

        self.instance = AnalyzeChangeSet(FastMCP("Test"), {"run_local_tool_fn": mock_run_local_tool})

        expected = """Error: Invalid JSON input: Expecting value: line 1 column 1 (char 0)
Input: string output"""
        result = self.instance.analyze_change_set("main", "/my/git/path")

        self.assertEqual(expected, result)

    def test_analyze_change_set_local_binary(self):
        """Test that local/native binary mode works without CS_MOUNT_PATH."""
        os.environ.pop("CS_MOUNT_PATH", None)

        captured_path = []

        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_path.append(path)
            return json.dumps([{"name": "test.tsx"}])

        self.instance = AnalyzeChangeSet(FastMCP("Test"), {"run_local_tool_fn": mock_run_local_tool})

        expected = {
            "results": [{"name": "test.tsx", "verdict": "unknown", "findings": []}],
            "quality_gates": "passed",
        }

        result = self.instance.analyze_change_set("main", "/my/local/git/path")

        self.assertEqual(json.dumps(expected), result)
        self.assertEqual(captured_path[-1], "/my/local/git/path")

    def test_analyze_change_set_passes_base_ref_in_cli_command(self):
        """Test that base_ref is included in the CLI command sent to the tool."""
        os.environ.pop("CS_MOUNT_PATH", None)

        captured_commands = []

        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_commands.append(cli_command)
            return json.dumps([{"name": "test.tsx"}])

        self.instance = AnalyzeChangeSet(FastMCP("Test"), {"run_local_tool_fn": mock_run_local_tool})

        self.instance.analyze_change_set("develop", "/my/local/git/path")

        # The CLI command should contain the base_ref
        cli_command = captured_commands[-1]
        self.assertIn("develop", cli_command)
        self.assertIn("delta", cli_command)
        self.assertIn("--output-format=json", cli_command)

    def test_analyze_change_set_empty_output_passes(self):
        """Test that empty output (no code health impact) results in passed quality gates."""
        os.environ.pop("CS_MOUNT_PATH", None)

        def mock_run_local_tool(cli_command, path, extra_env=None):
            return ""

        self.instance = AnalyzeChangeSet(FastMCP("Test"), {"run_local_tool_fn": mock_run_local_tool})

        expected = {"results": [], "quality_gates": "passed"}

        result = self.instance.analyze_change_set("main", "/my/local/git/path")

        self.assertEqual(json.dumps(expected), result)


class TestAnalyzeChangeSetWorktree(unittest.TestCase):
    """Tests for analyze_change_set git worktree support."""

    def setUp(self):
        self._env = dict(os.environ)
        self.temp_dir = os.path.realpath(tempfile.mkdtemp())

        # Write .git file pointing to main repo's worktrees
        self.worktree_gitdir = "/path/to/main/.git/worktrees/feature"
        with open(os.path.join(self.temp_dir, ".git"), "w") as f:
            f.write(f"gitdir: {self.worktree_gitdir}")

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)
        import shutil

        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_analyze_local_sets_git_dir_for_worktree(self):
        """Test that local analysis sets GIT_DIR for worktree environments."""
        os.environ.pop("CS_MOUNT_PATH", None)

        captured_extra_env = []

        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_extra_env.append(extra_env)
            return json.dumps([{"name": "test.tsx"}])

        instance = AnalyzeChangeSet(FastMCP("Test"), {"run_local_tool_fn": mock_run_local_tool})

        instance.analyze_change_set("main", self.temp_dir)

        self.assertEqual(len(captured_extra_env), 1)
        extra_env = captured_extra_env[0]
        self.assertIsNotNone(extra_env)
        self.assertIn("GIT_DIR", extra_env)
        self.assertEqual(self.worktree_gitdir, extra_env["GIT_DIR"])

    def test_analyze_local_no_extra_env_for_regular_repo(self):
        """Test that local analysis doesn't set GIT_DIR for regular repos."""
        os.environ.pop("CS_MOUNT_PATH", None)

        regular_repo = os.path.join(self.temp_dir, "regular")
        os.makedirs(os.path.join(regular_repo, ".git"))

        captured_extra_env = []

        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_extra_env.append(extra_env)
            return json.dumps([{"name": "test.tsx"}])

        instance = AnalyzeChangeSet(FastMCP("Test"), {"run_local_tool_fn": mock_run_local_tool})

        instance.analyze_change_set("main", regular_repo)

        self.assertEqual(len(captured_extra_env), 1)
        self.assertIsNone(captured_extra_env[0])

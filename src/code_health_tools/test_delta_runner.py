import json
import os
import tempfile
import unittest
from unittest import mock

from code_health_tools.delta_runner import run_delta_cli

SAMPLE_CLI_COMMAND = ["cs", "delta", "--output-format=json"]

SINGLE_FILE_OUTPUT = json.dumps([{"name": "test.tsx"}])
SINGLE_FILE_EXPECTED = {
    "results": [{"name": "test.tsx", "verdict": "unknown", "findings": []}],
    "quality_gates": "passed",
}


def mock_run_returning(output):
    """Create a mock run_local_tool_fn that returns a fixed output."""

    def mock_run_local_tool(cli_command, path, extra_env=None):
        return output

    return mock_run_local_tool


class TestRunDeltaCliDocker(unittest.TestCase):
    """Tests for run_delta_cli in Docker mode (CS_MOUNT_PATH set)."""

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/my/git/path"})
    def test_docker_returns_parsed_results(self):
        result = run_delta_cli(SAMPLE_CLI_COMMAND, "/my/git/path", mock_run_returning(SINGLE_FILE_OUTPUT))

        self.assertEqual(json.dumps(SINGLE_FILE_EXPECTED), result)

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/my/git/path"})
    def test_docker_sets_safe_directory(self):
        captured_calls = []

        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_calls.append({"command": cli_command, "path": path, "extra_env": extra_env})
            return SINGLE_FILE_OUTPUT

        run_delta_cli(SAMPLE_CLI_COMMAND, "/my/git/path", mock_run_local_tool)

        # First call should be git config safe.directory
        self.assertEqual(len(captured_calls), 2)
        self.assertEqual(captured_calls[0]["command"][0], "git")
        self.assertIn("safe.directory", captured_calls[0]["command"])

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/my/git/path"})
    def test_docker_invalid_json_returns_error(self):
        result = run_delta_cli(SAMPLE_CLI_COMMAND, "/my/git/path", mock_run_returning("string output"))

        self.assertTrue(result.startswith("Error:"))
        self.assertIn("Invalid JSON input", result)


class TestRunDeltaCliLocal(unittest.TestCase):
    """Tests for run_delta_cli in local/native mode (no CS_MOUNT_PATH)."""

    def setUp(self):
        os.environ.pop("CS_MOUNT_PATH", None)

    def test_local_returns_parsed_results(self):
        result = run_delta_cli(
            SAMPLE_CLI_COMMAND, "/my/local/git/path", mock_run_returning(SINGLE_FILE_OUTPUT)
        )

        self.assertEqual(json.dumps(SINGLE_FILE_EXPECTED), result)

    def test_local_passes_path_directly(self):
        captured_paths = []

        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_paths.append(path)
            return SINGLE_FILE_OUTPUT

        run_delta_cli(SAMPLE_CLI_COMMAND, "/my/local/git/path", mock_run_local_tool)

        self.assertEqual(captured_paths[-1], "/my/local/git/path")

    def test_local_empty_output_passes(self):
        result = run_delta_cli(SAMPLE_CLI_COMMAND, "/some/path", mock_run_returning(""))

        expected = {"results": [], "quality_gates": "passed"}
        self.assertEqual(json.dumps(expected), result)

    def test_local_invalid_json_returns_error(self):
        result = run_delta_cli(SAMPLE_CLI_COMMAND, "/some/path", mock_run_returning("string output"))

        self.assertTrue(result.startswith("Error:"))
        self.assertIn("Invalid JSON input", result)


class TestRunDeltaCliWorktree(unittest.TestCase):
    """Tests for run_delta_cli git worktree support."""

    def setUp(self):
        self._env = dict(os.environ)
        self.temp_dir = os.path.realpath(tempfile.mkdtemp())

        self.worktree_gitdir = "/path/to/main/.git/worktrees/feature"
        with open(os.path.join(self.temp_dir, ".git"), "w") as f:
            f.write(f"gitdir: {self.worktree_gitdir}")

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)
        import shutil

        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_local_sets_git_dir_for_worktree(self):
        os.environ.pop("CS_MOUNT_PATH", None)

        captured_extra_env = []

        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_extra_env.append(extra_env)
            return SINGLE_FILE_OUTPUT

        run_delta_cli(SAMPLE_CLI_COMMAND, self.temp_dir, mock_run_local_tool)

        self.assertEqual(len(captured_extra_env), 1)
        extra_env = captured_extra_env[0]
        self.assertIsNotNone(extra_env)
        self.assertIn("GIT_DIR", extra_env)
        self.assertEqual(self.worktree_gitdir, extra_env["GIT_DIR"])

    def test_local_no_extra_env_for_regular_repo(self):
        os.environ.pop("CS_MOUNT_PATH", None)

        regular_repo = os.path.join(self.temp_dir, "regular")
        os.makedirs(os.path.join(regular_repo, ".git"))

        captured_extra_env = []

        def mock_run_local_tool(cli_command, path, extra_env=None):
            captured_extra_env.append(extra_env)
            return SINGLE_FILE_OUTPUT

        run_delta_cli(SAMPLE_CLI_COMMAND, regular_repo, mock_run_local_tool)

        self.assertEqual(len(captured_extra_env), 1)
        self.assertIsNone(captured_extra_env[0])

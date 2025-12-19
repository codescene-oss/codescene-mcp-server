import os
import unittest
from unittest import mock
from pathlib import Path
from utils.platform_details import WindowsPlatformDetails, UnixPlatformDetails, get_platform_details


class TestCsCliPath(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    @mock.patch('utils.code_health_analysis.Path.exists')
    @mock.patch('os.access')
    def test_returns_bundled_cs_path_when_exists_and_executable(self, mock_access, mock_exists):
        from utils.code_health_analysis import cs_cli_path

        mock_exists.return_value = True
        mock_access.return_value = True
        platform_details = get_platform_details()

        result = cs_cli_path(platform_details)

        # Should return a path ending with either 'cs' or 'cs.exe' depending on platform
        self.assertTrue(result.endswith('cs') or result.endswith('cs.exe'))
        self.assertIn('src', result)

    @mock.patch('utils.code_health_analysis.sys')
    @mock.patch('utils.code_health_analysis.Path.exists')
    @mock.patch('os.access')
    def test_returns_bundled_cs_exe_path_on_windows(self, mock_access, mock_exists, mock_sys):
        from utils.code_health_analysis import cs_cli_path

        mock_sys.platform = "win32"
        mock_exists.return_value = True
        mock_access.return_value = True
        platform_details = WindowsPlatformDetails()

        result = cs_cli_path(platform_details)

        self.assertTrue(result.endswith('cs.exe'))
        self.assertIn('src', result)

    @mock.patch('utils.code_health_analysis.Path.exists')
    @mock.patch('os.access')
    @mock.patch('os.chmod')
    def test_sets_executable_permission_when_bundled_cs_not_executable(self, mock_chmod, mock_access, mock_exists):
        from utils.code_health_analysis import cs_cli_path

        mock_exists.return_value = True
        mock_access.return_value = False
        platform_details = get_platform_details()

        result = cs_cli_path(platform_details)

        mock_chmod.assert_called_once()
        # Should return a path ending with either 'cs' or 'cs.exe' depending on platform
        self.assertTrue(result.endswith('cs') or result.endswith('cs.exe'))

    @mock.patch('utils.code_health_analysis.Path.exists')
    def test_returns_env_cs_cli_path_when_bundled_not_exists(self, mock_exists):
        from utils.code_health_analysis import cs_cli_path

        mock_exists.return_value = False
        os.environ["CS_CLI_PATH"] = "/custom/path/to/cs"
        platform_details = get_platform_details()

        result = cs_cli_path(platform_details)

        self.assertEqual(result, "/custom/path/to/cs")

    @mock.patch('utils.code_health_analysis.Path.exists')
    def test_returns_default_path_when_no_bundled_and_no_env(self, mock_exists):
        from utils.code_health_analysis import cs_cli_path

        mock_exists.return_value = False
        os.environ.pop("CS_CLI_PATH", None)
        platform_details = get_platform_details()

        result = cs_cli_path(platform_details)

        # Should return docker default path
        self.assertEqual(result, '/root/.local/bin/cs')


class TestMakeCsCliReviewCommandFor(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    @mock.patch('utils.code_health_analysis.cs_cli_path')
    def test_returns_command_without_path_adaptation_when_no_mount_path(self, mock_cli_path):
        from utils.code_health_analysis import make_cs_cli_review_command_for

        mock_cli_path.return_value = "/path/to/cs"
        os.environ.pop("CS_MOUNT_PATH", None)

        result = make_cs_cli_review_command_for("review", "/project/src/foo.py")

        self.assertEqual(result, ["/path/to/cs", "review", "/project/src/foo.py", "--output-format=json"])

    @mock.patch('utils.code_health_analysis.cs_cli_path')
    @mock.patch('utils.code_health_analysis.adapt_mounted_file_path_inside_docker')
    def test_adapts_path_when_mount_path_set(self, mock_adapt, mock_cli_path):
        from utils.code_health_analysis import make_cs_cli_review_command_for

        mock_cli_path.return_value = "/path/to/cs"
        mock_adapt.return_value = "/mount/src/foo.py"
        os.environ["CS_MOUNT_PATH"] = "/project"

        result = make_cs_cli_review_command_for("review", "/project/src/foo.py")

        mock_adapt.assert_called_once_with("/project/src/foo.py")
        self.assertEqual(result, ["/path/to/cs", "review", "/mount/src/foo.py", "--output-format=json"])

    @mock.patch('utils.code_health_analysis.cs_cli_path')
    def test_supports_different_cli_commands(self, mock_cli_path):
        from utils.code_health_analysis import make_cs_cli_review_command_for

        mock_cli_path.return_value = "/path/to/cs"
        os.environ.pop("CS_MOUNT_PATH", None)

        result = make_cs_cli_review_command_for("delta", "/project/src/foo.py")

        self.assertEqual(result, ["/path/to/cs", "delta", "/project/src/foo.py", "--output-format=json"])


class TestCsCliReviewCommandFor(unittest.TestCase):
    @mock.patch('utils.code_health_analysis.make_cs_cli_review_command_for')
    def test_calls_make_with_review_command(self, mock_make):
        from utils.code_health_analysis import cs_cli_review_command_for

        mock_make.return_value = ["/path/to/cs", "review", "/foo.py", "--output-format=json"]

        result = cs_cli_review_command_for("/foo.py")

        mock_make.assert_called_once_with("review", "/foo.py", None)
        self.assertEqual(result, ["/path/to/cs", "review", "/foo.py", "--output-format=json"])


if __name__ == "__main__":
    unittest.main()

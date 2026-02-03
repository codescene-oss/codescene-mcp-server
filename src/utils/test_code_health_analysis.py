import builtins
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path

from errors import CodeSceneCliError
from utils.platform_details import WindowsPlatformDetails, UnixPlatformDetails, get_platform_details
from utils.code_health_analysis import (
    find_git_root,
    run_local_tool,
    run_cs_cli,
    analyze_code,
    cs_cli_path,
    make_cs_cli_review_command_for,
    cs_cli_review_command_for,
    _try_nuitka_cli_path,
)


class TestFindGitRoot(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)
        # Create a temp directory structure with .git
        # Use realpath to resolve symlinks (macOS /var -> /private/var)
        self.temp_dir = os.path.realpath(tempfile.mkdtemp())
        self.git_dir = os.path.join(self.temp_dir, '.git')
        os.makedirs(self.git_dir)
        self.sub_dir = os.path.join(self.temp_dir, 'src')
        os.makedirs(self.sub_dir)
        self.test_file = os.path.join(self.sub_dir, 'file.py')
        with open(self.test_file, 'w', encoding='utf-8') as f:
            f.write('# test file')

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)
        # Clean up temp directory
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_find_git_root_from_file(self):
        result = find_git_root(self.test_file)
        
        self.assertEqual(self.temp_dir, result)
    
    def test_find_git_root_from_directory(self):
        result = find_git_root(self.sub_dir)
        
        self.assertEqual(self.temp_dir, result)
    
    def test_find_git_root_raises_when_not_in_repo(self):
        # Create a temp dir without .git
        temp_no_git = tempfile.mkdtemp()
        test_file = os.path.join(temp_no_git, 'file.py')
        with open(test_file, 'w', encoding='utf-8') as f:
            f.write('# test')
        
        try:
            with self.assertRaises(CodeSceneCliError) as context:
                find_git_root(test_file)
            
            self.assertIn('Not in a git repository', str(context.exception))
        finally:
            shutil.rmtree(temp_no_git, ignore_errors=True)


class TestRunLocalTool(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    @mock.patch('utils.code_health_analysis.subprocess.run')
    @mock.patch('utils.code_health_analysis.get_platform_details')
    def test_run_local_tool_sets_cs_context(self, mock_platform, mock_run):
        mock_platform_instance = mock.MagicMock()
        mock_platform_instance.get_java_options.return_value = ''
        mock_platform_instance.configure_environment.side_effect = lambda x: x
        mock_platform.return_value = mock_platform_instance
        
        mock_result = mock.MagicMock()
        mock_result.returncode = 0
        mock_result.stdout = 'output'
        mock_run.return_value = mock_result
        
        run_local_tool(['echo', 'test'])
        
        # Check that CS_CONTEXT was set in the env passed to subprocess
        call_kwargs = mock_run.call_args[1]
        self.assertEqual('mcp-server', call_kwargs['env']['CS_CONTEXT'])
    
    @mock.patch('utils.code_health_analysis.subprocess.run')
    @mock.patch('utils.code_health_analysis.get_ssl_cli_args')
    @mock.patch('utils.code_health_analysis.get_platform_details')
    def test_run_local_tool_injects_ssl_args_for_cs_cli(self, mock_platform, mock_ssl_args, mock_run):
        mock_platform_instance = mock.MagicMock()
        mock_platform_instance.configure_environment.side_effect = lambda x: x
        mock_platform.return_value = mock_platform_instance
        
        # Mock SSL args
        mock_ssl_args.return_value = ['-Djavax.net.ssl.trustStore=/tmp/test.p12', '-Djavax.net.ssl.trustStoreType=PKCS12']
        
        mock_result = mock.MagicMock()
        mock_result.returncode = 0
        mock_result.stdout = 'output'
        mock_run.return_value = mock_result
        
        # Run with a CS CLI command
        run_local_tool(['/path/to/cs', 'review', 'file.py'])
        
        # Verify SSL args were injected after CLI path but before subcommand
        call_args = mock_run.call_args[0][0]
        self.assertEqual('/path/to/cs', call_args[0])
        self.assertEqual('-Djavax.net.ssl.trustStore=/tmp/test.p12', call_args[1])
        self.assertEqual('-Djavax.net.ssl.trustStoreType=PKCS12', call_args[2])
        self.assertEqual('review', call_args[3])
        self.assertEqual('file.py', call_args[4])
    
    @mock.patch('utils.code_health_analysis.subprocess.run')
    @mock.patch('utils.code_health_analysis.get_ssl_cli_args')
    @mock.patch('utils.code_health_analysis.get_platform_details')
    def test_run_local_tool_does_not_inject_ssl_args_for_non_cs_commands(self, mock_platform, mock_ssl_args, mock_run):
        mock_platform_instance = mock.MagicMock()
        mock_platform_instance.configure_environment.side_effect = lambda x: x
        mock_platform.return_value = mock_platform_instance
        
        # Mock SSL args
        mock_ssl_args.return_value = ['-Djavax.net.ssl.trustStore=/tmp/test.p12']
        
        mock_result = mock.MagicMock()
        mock_result.returncode = 0
        mock_result.stdout = 'output'
        mock_run.return_value = mock_result
        
        # Run with a non-CS CLI command (like git)
        run_local_tool(['git', 'status'])
        
        # Verify SSL args were NOT injected
        call_args = mock_run.call_args[0][0]
        self.assertEqual(['git', 'status'], call_args)
    
    @mock.patch('utils.code_health_analysis.subprocess.run')
    @mock.patch('utils.code_health_analysis.get_platform_details')
    def test_run_local_tool_raises_on_nonzero_return(self, mock_platform, mock_run):
        mock_platform_instance = mock.MagicMock()
        mock_platform_instance.get_java_options.return_value = ''
        mock_platform_instance.configure_environment.side_effect = lambda x: x
        mock_platform.return_value = mock_platform_instance
        
        mock_result = mock.MagicMock()
        mock_result.returncode = 1
        mock_result.stderr = 'error message'
        mock_run.return_value = mock_result
        
        with self.assertRaises(CodeSceneCliError) as context:
            run_local_tool(['failing', 'command'])
        
        self.assertIn('CLI command failed', str(context.exception))
    
    @mock.patch('utils.code_health_analysis.subprocess.run')
    @mock.patch('utils.code_health_analysis.get_platform_details')
    def test_run_local_tool_sets_onprem_url_when_present(self, mock_platform, mock_run):
        os.environ['CS_ONPREM_URL'] = 'https://onprem.example.com'
        
        mock_platform_instance = mock.MagicMock()
        mock_platform_instance.get_java_options.return_value = ''
        mock_platform_instance.configure_environment.side_effect = lambda x: x
        mock_platform.return_value = mock_platform_instance
        
        mock_result = mock.MagicMock()
        mock_result.returncode = 0
        mock_result.stdout = 'output'
        mock_run.return_value = mock_result
        
        run_local_tool(['echo', 'test'])
        
        call_kwargs = mock_run.call_args[1]
        self.assertEqual('https://onprem.example.com', call_kwargs['env']['CS_ONPREM_URL'])

    @mock.patch('utils.code_health_analysis.get_platform_details')
    def test_run_local_tool_handles_utf8_output(self, mock_platform):
        """Test that CLI output containing UTF-8 characters is handled correctly.
        
        Regression test for: 'ascii' codec can't decode byte 0xe2 error
        when source files contain UTF-8 characters (emojis, en-dashes, etc.)
        """
        mock_platform_instance = mock.MagicMock()
        mock_platform_instance.configure_environment.side_effect = lambda x: x
        mock_platform.return_value = mock_platform_instance
        
        # Use printf to output UTF-8 characters (more portable than echo)
        # Test characters: warning emoji, en-dash, curly quote
        utf8_test_string = "Test: \u26a0\ufe0f \u2013 \u201cquoted\u201d"
        
        if sys.platform == 'win32':
            # Windows: use python to echo the string
            result = run_local_tool(['python', '-c', f'print("{utf8_test_string}")'])
        else:
            # Unix: use printf which handles UTF-8 well
            result = run_local_tool(['printf', '%s', utf8_test_string])
        
        # Verify UTF-8 characters are preserved
        self.assertIn('\u26a0', result)  # warning sign
        self.assertIn('\u2013', result)  # en-dash
        self.assertIn('\u201c', result)  # left curly quote


class TestRunCsCli(unittest.TestCase):
    def test_run_cs_cli_handles_file_not_found(self):
        def raise_file_not_found():
            raise FileNotFoundError()
        
        result = run_cs_cli(raise_file_not_found)
        
        self.assertIn("Error:", result)
        self.assertIn("CodeScene CLI tool", result)
    
    def test_run_cs_cli_handles_called_process_error(self):
        def raise_called_process_error():
            raise subprocess.CalledProcessError(1, 'cs', stderr='process failed')
        
        result = run_cs_cli(raise_called_process_error)
        
        self.assertIn("Error:", result)
    
    def test_run_cs_cli_handles_generic_exception(self):
        def raise_generic():
            raise ValueError('something went wrong')
        
        result = run_cs_cli(raise_generic)
        
        self.assertIn("Error:", result)
        self.assertIn("something went wrong", result)


class TestAnalyzeCode(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    @mock.patch('utils.code_health_analysis.run_local_tool')
    @mock.patch('utils.code_health_analysis.cs_cli_review_command_for')
    @mock.patch('utils.code_health_analysis.find_git_root')
    def test_analyze_code_without_mount_path(self, mock_find_git_root, mock_cli_command, mock_run):
        os.environ.pop('CS_MOUNT_PATH', None)
        mock_find_git_root.return_value = '/project'
        mock_cli_command.return_value = ['cs', 'review', 'src/file.py', '--output-format=json']
        mock_run.return_value = '{"score": 8.5}'
        
        result = analyze_code('/project/src/file.py')
        
        mock_find_git_root.assert_called_once_with('/project/src/file.py')
        self.assertEqual('{"score": 8.5}', result)
    
    @mock.patch('utils.code_health_analysis.run_local_tool')
    @mock.patch('utils.code_health_analysis.cs_cli_review_command_for')
    def test_analyze_code_with_mount_path(self, mock_cli_command, mock_run):
        os.environ['CS_MOUNT_PATH'] = '/project'
        mock_cli_command.return_value = ['cs', 'review', '/mount/file.py', '--output-format=json']
        mock_run.return_value = '{"score": 9.0}'
        
        result = analyze_code('/project/src/file.py')
        
        self.assertEqual('{"score": 9.0}', result)


class TestCsCliPath(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    @mock.patch('utils.code_health_analysis.Path.exists')
    @mock.patch('os.access')
    def test_returns_bundled_cs_path_when_exists_and_executable(self, mock_access, mock_exists):
        mock_exists.return_value = True
        mock_access.return_value = True
        platform_details = get_platform_details()

        result = cs_cli_path(platform_details)

        # Should return a path ending with either 'cs' or 'cs.exe' depending on platform
        self.assertTrue(result.endswith('cs') or result.endswith('cs.exe'))
        # Should be an absolute path
        self.assertTrue(os.path.isabs(result))

    @mock.patch('utils.code_health_analysis.sys')
    @mock.patch('utils.code_health_analysis.Path.exists')
    @mock.patch('os.access')
    def test_returns_bundled_cs_exe_path_on_windows(self, mock_access, mock_exists, mock_sys):
        mock_sys.platform = "win32"
        mock_exists.return_value = True
        mock_access.return_value = True
        platform_details = WindowsPlatformDetails()

        result = cs_cli_path(platform_details)

        self.assertTrue(result.endswith('cs.exe'))
        # Should be an absolute path
        self.assertTrue(os.path.isabs(result))

    @mock.patch('utils.code_health_analysis.Path.exists')
    @mock.patch('os.access')
    @mock.patch('os.chmod')
    def test_sets_executable_permission_when_bundled_cs_not_executable(self, mock_chmod, mock_access, mock_exists):
        mock_exists.return_value = True
        mock_access.return_value = False
        platform_details = get_platform_details()

        result = cs_cli_path(platform_details)

        mock_chmod.assert_called_once()
        # Should return a path ending with either 'cs' or 'cs.exe' depending on platform
        self.assertTrue(result.endswith('cs') or result.endswith('cs.exe'))

    @mock.patch('utils.code_health_analysis.Path.exists')
    def test_returns_env_cs_cli_path_when_bundled_not_exists(self, mock_exists):
        mock_exists.return_value = False
        os.environ["CS_CLI_PATH"] = "/custom/path/to/cs"
        platform_details = get_platform_details()

        result = cs_cli_path(platform_details)

        self.assertEqual(result, "/custom/path/to/cs")

    @mock.patch('utils.code_health_analysis.Path.exists')
    def test_returns_default_path_when_no_bundled_and_no_env(self, mock_exists):
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
        mock_cli_path.return_value = "/path/to/cs"
        os.environ.pop("CS_MOUNT_PATH", None)

        result = make_cs_cli_review_command_for("review", "/project/src/foo.py")

        self.assertEqual(result, ["/path/to/cs", "review", "/project/src/foo.py", "--output-format=json"])

    @mock.patch('utils.code_health_analysis.cs_cli_path')
    @mock.patch('utils.code_health_analysis.adapt_mounted_file_path_inside_docker')
    def test_adapts_path_when_mount_path_set(self, mock_adapt, mock_cli_path):
        mock_cli_path.return_value = "/path/to/cs"
        mock_adapt.return_value = "/mount/src/foo.py"
        os.environ["CS_MOUNT_PATH"] = "/project"

        result = make_cs_cli_review_command_for("review", "/project/src/foo.py")

        mock_adapt.assert_called_once_with("/project/src/foo.py")
        self.assertEqual(result, ["/path/to/cs", "review", "/mount/src/foo.py", "--output-format=json"])

    @mock.patch('utils.code_health_analysis.cs_cli_path')
    def test_supports_different_cli_commands(self, mock_cli_path):
        mock_cli_path.return_value = "/path/to/cs"
        os.environ.pop("CS_MOUNT_PATH", None)

        result = make_cs_cli_review_command_for("delta", "/project/src/foo.py")

        self.assertEqual(result, ["/path/to/cs", "delta", "/project/src/foo.py", "--output-format=json"])


class TestCsCliReviewCommandFor(unittest.TestCase):
    @mock.patch('utils.code_health_analysis.make_cs_cli_review_command_for')
    def test_calls_make_with_review_command(self, mock_make):
        mock_make.return_value = ["/path/to/cs", "review", "/foo.py", "--output-format=json"]

        result = cs_cli_review_command_for("/foo.py")

        mock_make.assert_called_once_with("review", "/foo.py", None)
        self.assertEqual(result, ["/path/to/cs", "review", "/foo.py", "--output-format=json"])


class TestTryNuitkaCliPath(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    @mock.patch('os.chmod')
    @mock.patch('os.access')
    def test_returns_nuitka_path_when_compiled_and_binary_exists(self, mock_access, mock_chmod):
        """Test that Nuitka binary path is returned when running in Nuitka environment."""
        # Create a mock __compiled__ object with containing_dir
        mock_compiled = mock.MagicMock()
        mock_compiled.containing_dir = '/nuitka/dist'

        # Temporarily inject __compiled__ into builtins
        original_compiled = getattr(builtins, '__compiled__', None)
        builtins.__compiled__ = mock_compiled  # type: ignore[attr-defined]

        try:
            with mock.patch('utils.code_health_analysis.Path') as MockPath:
                mock_path_instance = mock.MagicMock()
                mock_path_instance.exists.return_value = True
                MockPath.return_value.__truediv__ = mock.MagicMock(return_value=mock_path_instance)

                mock_access.return_value = True

                result = _try_nuitka_cli_path('cs')

                self.assertEqual(result, str(mock_path_instance))
        finally:
            # Restore original state
            if original_compiled is None:
                delattr(builtins, '__compiled__')
            else:
                builtins.__compiled__ = original_compiled  # type: ignore[attr-defined]

    def test_returns_none_when_not_in_nuitka_environment(self):
        """Test that None is returned when not running in Nuitka environment."""
        result = _try_nuitka_cli_path('cs')

        self.assertIsNone(result)

    @mock.patch('os.chmod')
    @mock.patch('os.access')
    def test_sets_executable_when_nuitka_binary_not_executable(self, mock_access, mock_chmod):
        """Test that chmod is called when Nuitka binary exists but is not executable."""
        mock_compiled = mock.MagicMock()
        mock_compiled.containing_dir = '/nuitka/dist'

        original_compiled = getattr(builtins, '__compiled__', None)
        builtins.__compiled__ = mock_compiled  # type: ignore[attr-defined]

        try:
            with mock.patch('utils.code_health_analysis.Path') as MockPath:
                mock_path_instance = mock.MagicMock()
                mock_path_instance.exists.return_value = True
                MockPath.return_value.__truediv__ = mock.MagicMock(return_value=mock_path_instance)

                mock_access.return_value = False  # Not executable

                result = _try_nuitka_cli_path('cs')

                mock_chmod.assert_called_once_with(mock_path_instance, 0o755)
                self.assertEqual(result, str(mock_path_instance))
        finally:
            if original_compiled is None:
                delattr(builtins, '__compiled__')
            else:
                builtins.__compiled__ = original_compiled  # type: ignore[attr-defined]


class TestCsCliPathNuitkaIntegration(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    @mock.patch('utils.code_health_analysis._try_bundled_cli_path')
    @mock.patch('utils.code_health_analysis._try_nuitka_cli_path')
    def test_returns_nuitka_path_when_available(self, mock_nuitka, mock_bundled):
        """Test that cs_cli_path returns Nuitka path when available."""
        os.environ.pop('CS_CLI_PATH', None)
        os.environ.pop('CS_MOUNT_PATH', None)

        mock_nuitka.return_value = '/nuitka/dist/cs'
        mock_bundled.return_value = None

        mock_platform = mock.MagicMock()
        mock_platform.get_cli_binary_name.return_value = 'cs'

        result = cs_cli_path(mock_platform)

        self.assertEqual(result, '/nuitka/dist/cs')
        mock_bundled.assert_not_called()  # Should short-circuit before checking bundled


class TestAnalyzeCodeWorktree(unittest.TestCase):
    """Tests for analyze_code() git worktree support in static mode."""

    def setUp(self):
        self._env = dict(os.environ)
        # Create a temp worktree-like structure
        self.temp_dir = os.path.realpath(tempfile.mkdtemp())
        
        # Create worktree directory with .git file
        self.worktree_dir = os.path.join(self.temp_dir, 'worktree')
        os.makedirs(self.worktree_dir)
        
        # Write .git file pointing to main repo's worktrees
        self.worktree_gitdir = '/path/to/main/.git/worktrees/feature'
        with open(os.path.join(self.worktree_dir, '.git'), 'w') as f:
            f.write(f'gitdir: {self.worktree_gitdir}')
        
        self.src_dir = os.path.join(self.worktree_dir, 'src')
        os.makedirs(self.src_dir)
        self.test_file = os.path.join(self.src_dir, 'file.py')
        with open(self.test_file, 'w') as f:
            f.write('# test')

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    @mock.patch('utils.code_health_analysis.run_local_tool')
    @mock.patch('utils.code_health_analysis.cs_cli_review_command_for')
    def test_analyze_code_sets_git_dir_for_worktree(self, mock_cli_command, mock_run):
        os.environ.pop('CS_MOUNT_PATH', None)
        mock_cli_command.return_value = ['cs', 'review', 'src/file.py', '--output-format=json']
        mock_run.return_value = '{"score": 8.5}'
        
        analyze_code(self.test_file)
        
        # Verify GIT_DIR was passed in extra_env
        mock_run.assert_called_once()
        call_args = mock_run.call_args
        extra_env = call_args.kwargs.get('extra_env')
        
        self.assertIsNotNone(extra_env)
        self.assertIn('GIT_DIR', extra_env)
        self.assertEqual(self.worktree_gitdir, extra_env['GIT_DIR'])

    @mock.patch('utils.code_health_analysis.run_local_tool')
    @mock.patch('utils.code_health_analysis.cs_cli_review_command_for')
    def test_analyze_code_no_extra_env_for_regular_repo(self, mock_cli_command, mock_run):
        # Create regular repo (with .git directory, not file)
        regular_repo = os.path.join(self.temp_dir, 'regular')
        os.makedirs(os.path.join(regular_repo, '.git'))
        os.makedirs(os.path.join(regular_repo, 'src'))
        regular_file = os.path.join(regular_repo, 'src', 'file.py')
        with open(regular_file, 'w') as f:
            f.write('# test')
        
        os.environ.pop('CS_MOUNT_PATH', None)
        mock_cli_command.return_value = ['cs', 'review', 'src/file.py', '--output-format=json']
        mock_run.return_value = '{"score": 8.5}'
        
        analyze_code(regular_file)
        
        # Verify extra_env was None (no GIT_DIR needed)
        mock_run.assert_called_once()
        call_args = mock_run.call_args
        extra_env = call_args.kwargs.get('extra_env')
        
        self.assertIsNone(extra_env)


if __name__ == "__main__":
    unittest.main()

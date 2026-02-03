import os
import unittest
from unittest import mock
from errors import CodeSceneCliError
from .docker_path_adapter import adapt_mounted_file_path_inside_docker, get_relative_file_path_for_api


class TestAdaptMountedFilePathInsideDocker(unittest.TestCase):
    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    def assert_maps(self, mount, user_input, expected):
        os.environ["CS_MOUNT_PATH"] = mount
        self.assertEqual(adapt_mounted_file_path_inside_docker(user_input), expected)

    def test_mappings(self):
        cases = [
            # mount                 user_input                          expected
            ("/mnt/project",        "/mnt/project/src/foo.py",          "/mount/src/foo.py"),
            ("/mnt/project/",       "/mnt/project/src/foo.py",          "/mount/src/foo.py"),
            ("/mnt/project",        "/mnt/project",                     "/mount"),
            ("/mnt/project",        "/mnt/project/",                    "/mount"),
            ("/",                   "/src/foo.py",                      "/mount/src/foo.py"),
            ("C:\\code\\project",   "C:\\code\\project\\src\\foo.py",   "/mount/src/foo.py"),
            ("c:\\code\\ööproject", "c:\\code\\ööproject\\src\\föö.py", "/mount/src/föö.py"),
            # User path longer than mount path (positive case)
            ("/mnt/project",        "/mnt/project/src/foo.py/bar.py",   "/mount/src/foo.py/bar.py"),
        ]
        for mount, user_input, expected in cases:
            with self.subTest(mount=mount, user_input=user_input):
                self.assert_maps(mount, user_input, expected)

    def test_not_under_mount_raises(self):
        os.environ["CS_MOUNT_PATH"] = "/mnt/project"
        with self.assertRaises(CodeSceneCliError):
            adapt_mounted_file_path_inside_docker("/other/foo.py")

    def test_missing_env_raises(self):
        os.environ.pop("CS_MOUNT_PATH", None)
        with self.assertRaises(CodeSceneCliError):
            adapt_mounted_file_path_inside_docker("/mnt/project/src/foo.py")

    def test_erronous_mount_path_is_diagnosed(self):
        """
        The user is responsible for providing the mount path in the MCP config.
        A simple typo will wreck it, so the error message is important in order
        for users to self-diagnose the problem.

        Note there's a table-driven test just below that checks the error reporting.
        However, I still like to keep this test as a more verbose example on how 
        it looks and works.
        """
        LOWER_CASE_MOUNT_PATH = "c:\\git\\myproject"
        UPPER_CASE_USER_INPUT_PATH = "c:\\Git\\myproject"
        os.environ["CS_MOUNT_PATH"] = LOWER_CASE_MOUNT_PATH
        user_input = UPPER_CASE_USER_INPUT_PATH

        with self.assertRaises(CodeSceneCliError) as context:
            adapt_mounted_file_path_inside_docker(user_input)

        error_message = str(context.exception)
        expected_message = (
            "file_path is not under CS_MOUNT_PATH: '/C/Git/myproject'. "
            "Path mismatch at segment 2: 'Git' (input) vs 'git' (mount). "
            "Check for case sensitivity or typos. "
            "To fix: ensure your CS_MOUNT_PATH matches the input path exactly."
        )
        self.assertEqual(error_message, expected_message)

    def test_path_mismatch_reporting(self):
        """
        Table-driven test for error scenarios in adapt_mounted_file_path_inside_docker.
        Each case specifies mount path, user input, and expected error message (full or partial).
        """
        cases = [
            # Case sensitivity mismatch
            {
                "mount": "c:\git\myproject",
                "user_input": "c:\Git\myproject",
                "expected_msg": "Path mismatch at segment 2: 'Git' (input) vs 'git' (mount)."
            },
            # Path not under mount at all
            {
                "mount": "/mnt/project",
                "user_input": "/other/foo.py",
                "expected_msg": "file_path is not under CS_MOUNT_PATH"
            },
            # Typo in segment
            {
                "mount": "/mnt/projct",
                "user_input": "/mnt/project/src/foo.py",
                "expected_msg": "Path mismatch at segment 2: 'project' (input) vs 'projct' (mount)."
            },
            # Windows drive letter mismatch
            {
                "mount": "C:\\code\\project",
                "user_input": "D:\\code\\project\\src\\foo.py",
                "expected_msg": "Path mismatch at segment 1: 'D' (input) vs 'C' (mount)."
            },
            # Mount path longer than user path (negative case)
            {
                "mount": "/mnt/project/src/foo.py/bar.py",
                "user_input": "/mnt/project",
                "expected_msg": "Path mismatch at segment 3: '<none>' (input) vs 'src' (mount)."
            },
            # User path is root, mount path is a subpath (negative)
            {
                "mount": "/foo.py",
                "user_input": "/",
                "expected_msg": "Path mismatch at segment 1: '<none>' (input) vs 'foo.py' (mount)."
            },
            # Mount path is a prefix, but not a true parent (negative)
            {
                "mount": "/mnt/pro",
                "user_input": "/mnt/project/foo.py",
                "expected_msg": "Path mismatch at segment 2: 'project' (input) vs 'pro' (mount)."
            },
            # Negative unicode/typo case
            {
                "mount": "c:\\code\\ööproject",
                "user_input": "c:\\code\\ööprojeckt\\src\\föö.py",
                "expected_msg": "Path mismatch at segment 3: 'ööprojeckt' (input) vs 'ööproject' (mount)."
            },
            # Extra segments in the middle (negative)
            {
                "mount": "/mnt/project/foo.py",
                "user_input": "/mnt/project/bar.py/baz.py",
                "expected_msg": "Path mismatch at segment 3: 'bar.py' (input) vs 'foo.py' (mount)."
            },
        ]
        for case in cases:
            with self.subTest(mount=case["mount"], user_input=case["user_input"]):
                os.environ["CS_MOUNT_PATH"] = case["mount"]
                with self.assertRaises(CodeSceneCliError) as context:
                    adapt_mounted_file_path_inside_docker(case["user_input"])
                error_message = str(context.exception)
                self.assertIn(case["expected_msg"], error_message)

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/mnt/project"})
    def test_relative_path_raises(self):
        with self.assertRaises(CodeSceneCliError):
            adapt_mounted_file_path_inside_docker("src/foo.py")


class TestWorktreeGitdirAdapter(unittest.TestCase):
    """Tests for git worktree support in Docker path adaptation."""

    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    def test_read_worktree_gitdir_from_file(self):
        """Test reading gitdir from a worktree .git file."""
        from .docker_path_adapter import _read_worktree_gitdir
        import tempfile
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.git', delete=False, encoding='utf-8') as f:
            f.write("gitdir: /path/to/main/repo/.git/worktrees/my-branch\n")
            f.flush()
            
            result = _read_worktree_gitdir(f.name)
            self.assertEqual(result, "/path/to/main/repo/.git/worktrees/my-branch")
        
        import os
        os.unlink(f.name)

    def test_read_worktree_gitdir_with_utf8_content(self):
        """Test that gitdir files with UTF-8 characters are read correctly.
        
        Regression test for: 'ascii' codec can't decode byte 0xe2 error
        when source files contain UTF-8 characters (emojis, en-dashes, etc.)
        """
        from .docker_path_adapter import _read_worktree_gitdir
        import tempfile
        
        # Test various UTF-8 characters that commonly cause issues
        utf8_test_cases = [
            # Path with emoji (warning sign)
            ("gitdir: /path/to/repo/\u26a0\ufe0f-warning/.git/worktrees/branch\n",
             "/path/to/repo/\u26a0\ufe0f-warning/.git/worktrees/branch"),
            # Path with en-dash (U+2013)
            ("gitdir: /path/to/repo/2023\u20132024/.git/worktrees/branch\n",
             "/path/to/repo/2023\u20132024/.git/worktrees/branch"),
            # Path with curly quotes (U+201C and U+201D)
            ("gitdir: /path/to/repo/\u201cquoted\u201d/.git/worktrees/branch\n",
             "/path/to/repo/\u201cquoted\u201d/.git/worktrees/branch"),
            # Path with various Unicode characters
            ("gitdir: /path/to/repo/f\u00f6\u00f6-b\u00e4r-\u00f1/.git/worktrees/branch\n",
             "/path/to/repo/f\u00f6\u00f6-b\u00e4r-\u00f1/.git/worktrees/branch"),
        ]
        
        for gitdir_content, expected in utf8_test_cases:
            with self.subTest(gitdir_content=gitdir_content):
                with tempfile.NamedTemporaryFile(mode='w', suffix='.git', 
                                                  delete=False, encoding='utf-8') as f:
                    f.write(gitdir_content)
                    f.flush()
                    
                    result = _read_worktree_gitdir(f.name)
                    self.assertEqual(result, expected)
                
                import os
                os.unlink(f.name)

    def test_read_worktree_gitdir_returns_none_for_directory(self):
        """Test that reading gitdir from a directory returns None."""
        from .docker_path_adapter import _read_worktree_gitdir
        import tempfile
        
        with tempfile.TemporaryDirectory() as tmpdir:
            result = _read_worktree_gitdir(tmpdir)
            self.assertIsNone(result)

    def test_read_worktree_gitdir_returns_none_for_nonexistent(self):
        """Test that reading gitdir from nonexistent path returns None."""
        from .docker_path_adapter import _read_worktree_gitdir
        
        result = _read_worktree_gitdir("/nonexistent/path/.git")
        self.assertIsNone(result)

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/Users/david/workspace"})
    def test_adapt_worktree_gitdir_for_docker(self):
        """Test full worktree gitdir path translation."""
        from .docker_path_adapter import adapt_worktree_gitdir_for_docker
        import tempfile
        import os as os_module
        
        # Create a temp directory structure simulating a mounted worktree
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create a mock .git file with a gitdir pointer
            worktree_dir = os_module.path.join(tmpdir, "worktree")
            os_module.makedirs(worktree_dir)
            git_file = os_module.path.join(worktree_dir, ".git")
            
            with open(git_file, 'w', encoding='utf-8') as f:
                f.write("gitdir: /Users/david/workspace/main-repo/.git/worktrees/my-branch\n")
            
            # The function should translate the host path in .git file
            result = adapt_worktree_gitdir_for_docker(worktree_dir)
            self.assertEqual(result, "/mount/main-repo/.git/worktrees/my-branch")

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/Users/david/workspace"})
    def test_adapt_worktree_gitdir_returns_none_for_regular_repo(self):
        """Test that regular repos (with .git directory) return None."""
        from .docker_path_adapter import adapt_worktree_gitdir_for_docker
        import tempfile
        import os as os_module
        
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create a .git directory (regular repo, not worktree)
            git_dir = os_module.path.join(tmpdir, ".git")
            os_module.makedirs(git_dir)
            
            result = adapt_worktree_gitdir_for_docker(tmpdir)
            self.assertIsNone(result)

    def test_adapt_worktree_gitdir_edge_cases(self):
        """Test worktree gitdir translation edge cases via table-driven tests."""
        from .docker_path_adapter import adapt_worktree_gitdir_for_docker
        import tempfile
        import os as os_module
        
        cases = [
            # (mount_path, gitdir_content, expected_result, description)
            (
                "/Users/david/project-a",
                "gitdir: /Users/david/project-b/.git/worktrees/branch\n",
                None,
                "gitdir outside mount path returns None"
            ),
            (
                "C:\\Users\\david\\workspace",
                "gitdir: C:\\Users\\david\\workspace\\main-repo\\.git\\worktrees\\my-branch\n",
                "/mount/main-repo/.git/worktrees/my-branch",
                "Windows-style paths are translated correctly"
            ),
        ]
        
        for mount_path, gitdir_content, expected, description in cases:
            with self.subTest(description=description):
                with mock.patch.dict(os.environ, {"CS_MOUNT_PATH": mount_path}):
                    with tempfile.TemporaryDirectory() as tmpdir:
                        worktree_dir = os_module.path.join(tmpdir, "worktree")
                        os_module.makedirs(worktree_dir)
                        git_file = os_module.path.join(worktree_dir, ".git")
                        
                        with open(git_file, 'w', encoding='utf-8') as f:
                            f.write(gitdir_content)
                        
                        result = adapt_worktree_gitdir_for_docker(worktree_dir)
                        self.assertEqual(result, expected)


class TestGetWorktreeGitdir(unittest.TestCase):
    """Tests for get_worktree_gitdir() supporting static mode worktree detection."""

    def test_get_worktree_gitdir_returns_gitdir_for_worktree(self):
        """Test that get_worktree_gitdir returns gitdir path for a worktree."""
        from .docker_path_adapter import get_worktree_gitdir
        import tempfile
        import os as os_module
        
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create .git file (not directory) with gitdir content
            git_file = os_module.path.join(tmpdir, '.git')
            with open(git_file, 'w', encoding='utf-8') as f:
                f.write('gitdir: /path/to/main/.git/worktrees/feature')
            
            result = get_worktree_gitdir(tmpdir)
            
            self.assertEqual('/path/to/main/.git/worktrees/feature', result)

    def test_get_worktree_gitdir_returns_none_for_regular_repo(self):
        """Test that get_worktree_gitdir returns None for regular git repo."""
        from .docker_path_adapter import get_worktree_gitdir
        import tempfile
        import os as os_module
        
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create .git directory (regular repo)
            git_dir = os_module.path.join(tmpdir, '.git')
            os_module.makedirs(git_dir)
            
            result = get_worktree_gitdir(tmpdir)
            
            self.assertIsNone(result)

    def test_get_worktree_gitdir_returns_none_for_no_git(self):
        """Test that get_worktree_gitdir returns None when no .git exists."""
        from .docker_path_adapter import get_worktree_gitdir
        import tempfile
        
        with tempfile.TemporaryDirectory() as tmpdir:
            result = get_worktree_gitdir(tmpdir)
            
            self.assertIsNone(result)

    def test_get_worktree_gitdir_handles_windows_paths(self):
        """Test that get_worktree_gitdir handles Windows-style paths in .git file."""
        from .docker_path_adapter import get_worktree_gitdir
        import tempfile
        import os as os_module
        
        with tempfile.TemporaryDirectory() as tmpdir:
            git_file = os_module.path.join(tmpdir, '.git')
            with open(git_file, 'w', encoding='utf-8') as f:
                f.write('gitdir: C:\\workspace\\stargate\\.git\\worktrees\\ip_ps')
            
            result = get_worktree_gitdir(tmpdir)
            
            self.assertEqual('C:\\workspace\\stargate\\.git\\worktrees\\ip_ps', result)


class TestGetRelativeFilePathForApi(unittest.TestCase):
    """Tests for get_relative_file_path_for_api supporting both Docker and static modes."""

    def setUp(self):
        self._env = dict(os.environ)

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._env)

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/mnt/project"})
    def test_docker_mode_returns_relative_path(self):
        """In Docker mode, should use adapt_mounted_file_path and strip /mount/ prefix."""
        result = get_relative_file_path_for_api("/mnt/project/src/foo.py")
        self.assertEqual(result, "src/foo.py")

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/mnt/project"})
    def test_docker_mode_nested_path(self):
        """In Docker mode with nested paths, should work correctly."""
        result = get_relative_file_path_for_api("/mnt/project/src/components/Button.tsx")
        self.assertEqual(result, "src/components/Button.tsx")

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "C:\\code\\project"})
    def test_docker_mode_windows_path(self):
        """In Docker mode with Windows paths, should work correctly."""
        result = get_relative_file_path_for_api("C:\\code\\project\\src\\foo.py")
        self.assertEqual(result, "src/foo.py")

    def test_static_mode_uses_git_root(self):
        """In static mode (no CS_MOUNT_PATH), should use git root to compute relative path."""
        import tempfile
        import os as os_module
        
        # Ensure CS_MOUNT_PATH is not set
        os.environ.pop("CS_MOUNT_PATH", None)
        
        # Create a temp git repo structure
        with tempfile.TemporaryDirectory() as tmpdir:
            git_dir = os_module.path.join(tmpdir, ".git")
            os_module.makedirs(git_dir)
            src_dir = os_module.path.join(tmpdir, "src")
            os_module.makedirs(src_dir)
            test_file = os_module.path.join(src_dir, "foo.py")
            with open(test_file, 'w') as f:
                f.write("# test")
            
            result = get_relative_file_path_for_api(test_file)
            self.assertEqual(result, "src/foo.py")

    def test_static_mode_nested_directories(self):
        """In static mode, should handle nested directory structures."""
        import tempfile
        import os as os_module
        
        os.environ.pop("CS_MOUNT_PATH", None)
        
        with tempfile.TemporaryDirectory() as tmpdir:
            git_dir = os_module.path.join(tmpdir, ".git")
            os_module.makedirs(git_dir)
            nested_dir = os_module.path.join(tmpdir, "src", "components", "ui")
            os_module.makedirs(nested_dir)
            test_file = os_module.path.join(nested_dir, "Button.tsx")
            with open(test_file, 'w') as f:
                f.write("// test")
            
            result = get_relative_file_path_for_api(test_file)
            self.assertEqual(result, "src/components/ui/Button.tsx")

    def test_static_mode_not_in_git_repo_returns_path_as_is(self):
        """In static mode with file outside git repo, should return path unchanged."""
        import tempfile
        
        os.environ.pop("CS_MOUNT_PATH", None)
        
        # Create a temp file NOT in a git repo
        with tempfile.NamedTemporaryFile(delete=False) as f:
            test_file = f.name
        
        try:
            result = get_relative_file_path_for_api(test_file)
            # Should return the path unchanged when not in a git repo
            self.assertEqual(result, test_file)
        finally:
            import os as os_module
            os_module.unlink(test_file)

    def test_static_mode_relative_path_returns_as_is(self):
        """In static mode with relative path, should return unchanged."""
        os.environ.pop("CS_MOUNT_PATH", None)
        
        result = get_relative_file_path_for_api("src/foo.py")
        self.assertEqual(result, "src/foo.py")

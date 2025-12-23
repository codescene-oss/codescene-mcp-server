import os
import unittest
from unittest import mock
from errors import CodeSceneCliError
from .docker_path_adapter import adapt_mounted_file_path_inside_docker


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
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.git', delete=False) as f:
            f.write("gitdir: /path/to/main/repo/.git/worktrees/my-branch\n")
            f.flush()
            
            result = _read_worktree_gitdir(f.name)
            self.assertEqual(result, "/path/to/main/repo/.git/worktrees/my-branch")
        
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
            
            with open(git_file, 'w') as f:
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

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/Users/david/project-a"})
    def test_adapt_worktree_gitdir_returns_none_when_gitdir_outside_mount(self):
        """Test that gitdir pointing outside mount path returns None."""
        from .docker_path_adapter import adapt_worktree_gitdir_for_docker
        import tempfile
        import os as os_module
        
        with tempfile.TemporaryDirectory() as tmpdir:
            worktree_dir = os_module.path.join(tmpdir, "worktree")
            os_module.makedirs(worktree_dir)
            git_file = os_module.path.join(worktree_dir, ".git")
            
            # gitdir points to a different project outside the mount
            with open(git_file, 'w') as f:
                f.write("gitdir: /Users/david/project-b/.git/worktrees/branch\n")
            
            result = adapt_worktree_gitdir_for_docker(worktree_dir)
            self.assertIsNone(result)

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "C:\\Users\\david\\workspace"})
    def test_adapt_worktree_gitdir_with_windows_paths(self):
        """Test worktree gitdir translation with Windows-style paths."""
        from .docker_path_adapter import adapt_worktree_gitdir_for_docker
        import tempfile
        import os as os_module
        
        with tempfile.TemporaryDirectory() as tmpdir:
            worktree_dir = os_module.path.join(tmpdir, "worktree")
            os_module.makedirs(worktree_dir)
            git_file = os_module.path.join(worktree_dir, ".git")
            
            # Simulate Windows-style gitdir path in the .git file
            with open(git_file, 'w') as f:
                f.write("gitdir: C:\\Users\\david\\workspace\\main-repo\\.git\\worktrees\\my-branch\n")
            
            result = adapt_worktree_gitdir_for_docker(worktree_dir)
            self.assertEqual(result, "/mount/main-repo/.git/worktrees/my-branch")

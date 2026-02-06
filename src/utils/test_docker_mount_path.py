import os
import shutil
import tempfile
import unittest
from unittest import mock

from errors import CodeSceneCliError

from .docker_path_adapter import (
    adapt_mounted_file_path_inside_docker,
    get_relative_file_path_for_api,
    get_relative_path_from_git_root,
)


def create_git_worktree_file(tmpdir: str, gitdir_content: str) -> str:
    """Create a .git file (worktree style) with the given gitdir content."""
    git_file = os.path.join(tmpdir, ".git")
    with open(git_file, "w", encoding="utf-8") as f:
        f.write(gitdir_content)
    return git_file


def create_git_directory(tmpdir: str) -> str:
    """Create a .git directory (regular repo style)."""
    git_dir = os.path.join(tmpdir, ".git")
    os.makedirs(git_dir)
    return git_dir


def create_temp_git_repo_with_file(tmpdir: str, relative_path: str, content: str = "# test") -> str:
    """Create a git repo structure with a file at the given relative path."""
    create_git_directory(tmpdir)
    file_path = os.path.join(tmpdir, relative_path)
    os.makedirs(os.path.dirname(file_path), exist_ok=True)
    with open(file_path, "w") as f:
        f.write(content)
    return file_path


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
            ("/mnt/project", "/mnt/project/src/foo.py", "/mount/src/foo.py"),
            ("/mnt/project/", "/mnt/project/src/foo.py", "/mount/src/foo.py"),
            ("/mnt/project", "/mnt/project", "/mount"),
            ("/mnt/project", "/mnt/project/", "/mount"),
            ("/", "/src/foo.py", "/mount/src/foo.py"),
            (
                "C:\\code\\project",
                "C:\\code\\project\\src\\foo.py",
                "/mount/src/foo.py",
            ),
            (
                "c:\\code\\ööproject",
                "c:\\code\\ööproject\\src\\föö.py",
                "/mount/src/föö.py",
            ),
            # User path longer than mount path (positive case)
            (
                "/mnt/project",
                "/mnt/project/src/foo.py/bar.py",
                "/mount/src/foo.py/bar.py",
            ),
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
                "mount": r"c:\git\myproject",
                "user_input": r"c:\Git\myproject",
                "expected_msg": "Path mismatch at segment 2: 'Git' (input) vs 'git' (mount).",
            },
            # Path not under mount at all
            {
                "mount": "/mnt/project",
                "user_input": "/other/foo.py",
                "expected_msg": "file_path is not under CS_MOUNT_PATH",
            },
            # Typo in segment
            {
                "mount": "/mnt/projct",
                "user_input": "/mnt/project/src/foo.py",
                "expected_msg": "Path mismatch at segment 2: 'project' (input) vs 'projct' (mount).",
            },
            # Windows drive letter mismatch
            {
                "mount": "C:\\code\\project",
                "user_input": "D:\\code\\project\\src\\foo.py",
                "expected_msg": "Path mismatch at segment 1: 'D' (input) vs 'C' (mount).",
            },
            # Mount path longer than user path (negative case)
            {
                "mount": "/mnt/project/src/foo.py/bar.py",
                "user_input": "/mnt/project",
                "expected_msg": "Path mismatch at segment 3: '<none>' (input) vs 'src' (mount).",
            },
            # User path is root, mount path is a subpath (negative)
            {
                "mount": "/foo.py",
                "user_input": "/",
                "expected_msg": "Path mismatch at segment 1: '<none>' (input) vs 'foo.py' (mount).",
            },
            # Mount path is a prefix, but not a true parent (negative)
            {
                "mount": "/mnt/pro",
                "user_input": "/mnt/project/foo.py",
                "expected_msg": "Path mismatch at segment 2: 'project' (input) vs 'pro' (mount).",
            },
            # Negative unicode/typo case
            {
                "mount": "c:\\code\\ööproject",
                "user_input": "c:\\code\\ööprojeckt\\src\\föö.py",
                "expected_msg": "Path mismatch at segment 3: 'ööprojeckt' (input) vs 'ööproject' (mount).",
            },
            # Extra segments in the middle (negative)
            {
                "mount": "/mnt/project/foo.py",
                "user_input": "/mnt/project/bar.py/baz.py",
                "expected_msg": "Path mismatch at segment 3: 'bar.py' (input) vs 'foo.py' (mount).",
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

        with tempfile.NamedTemporaryFile(mode="w", suffix=".git", delete=False, encoding="utf-8") as f:
            f.write("gitdir: /path/to/main/repo/.git/worktrees/my-branch\n")
            f.flush()

            result = _read_worktree_gitdir(f.name)
            self.assertEqual(result, "/path/to/main/repo/.git/worktrees/my-branch")

        os.unlink(f.name)

    def test_read_worktree_gitdir_with_utf8_content(self):
        """Test that gitdir files with UTF-8 characters are read correctly.

        Regression test for: 'ascii' codec can't decode byte 0xe2 error
        when source files contain UTF-8 characters (emojis, en-dashes, etc.)
        """
        from .docker_path_adapter import _read_worktree_gitdir

        # Test various UTF-8 characters that commonly cause issues
        utf8_test_cases = [
            # Path with emoji (warning sign)
            (
                "gitdir: /path/to/repo/\u26a0\ufe0f-warning/.git/worktrees/branch\n",
                "/path/to/repo/\u26a0\ufe0f-warning/.git/worktrees/branch",
            ),
            # Path with en-dash (U+2013)
            (
                "gitdir: /path/to/repo/2023\u20132024/.git/worktrees/branch\n",
                "/path/to/repo/2023\u20132024/.git/worktrees/branch",
            ),
            # Path with curly quotes (U+201C and U+201D)
            (
                "gitdir: /path/to/repo/\u201cquoted\u201d/.git/worktrees/branch\n",
                "/path/to/repo/\u201cquoted\u201d/.git/worktrees/branch",
            ),
            # Path with various Unicode characters
            (
                "gitdir: /path/to/repo/f\u00f6\u00f6-b\u00e4r-\u00f1/.git/worktrees/branch\n",
                "/path/to/repo/f\u00f6\u00f6-b\u00e4r-\u00f1/.git/worktrees/branch",
            ),
        ]

        for gitdir_content, expected in utf8_test_cases:
            with self.subTest(gitdir_content=gitdir_content):
                with tempfile.NamedTemporaryFile(mode="w", suffix=".git", delete=False, encoding="utf-8") as f:
                    f.write(gitdir_content)
                    f.flush()

                    result = _read_worktree_gitdir(f.name)
                    self.assertEqual(result, expected)

                os.unlink(f.name)

    def test_read_worktree_gitdir_returns_none_for_directory(self):
        """Test that reading gitdir from a directory returns None."""
        from .docker_path_adapter import _read_worktree_gitdir

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

        with tempfile.TemporaryDirectory() as tmpdir:
            worktree_dir = os.path.join(tmpdir, "worktree")
            os.makedirs(worktree_dir)
            create_git_worktree_file(
                worktree_dir,
                "gitdir: /Users/david/workspace/main-repo/.git/worktrees/my-branch\n",
            )

            result = adapt_worktree_gitdir_for_docker(worktree_dir)
            self.assertEqual(result, "/mount/main-repo/.git/worktrees/my-branch")

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/Users/david/workspace"})
    def test_adapt_worktree_gitdir_returns_none_for_regular_repo(self):
        """Test that regular repos (with .git directory) return None."""
        from .docker_path_adapter import adapt_worktree_gitdir_for_docker

        with tempfile.TemporaryDirectory() as tmpdir:
            create_git_directory(tmpdir)
            result = adapt_worktree_gitdir_for_docker(tmpdir)
            self.assertIsNone(result)

    def test_adapt_worktree_gitdir_edge_cases(self):
        """Test worktree gitdir translation edge cases via table-driven tests."""
        from .docker_path_adapter import adapt_worktree_gitdir_for_docker

        cases = [
            # (mount_path, gitdir_content, expected_result, description)
            (
                "/Users/david/project-a",
                "gitdir: /Users/david/project-b/.git/worktrees/branch\n",
                None,
                "gitdir outside mount path returns None",
            ),
            (
                "C:\\Users\\david\\workspace",
                "gitdir: C:\\Users\\david\\workspace\\main-repo\\.git\\worktrees\\my-branch\n",
                "/mount/main-repo/.git/worktrees/my-branch",
                "Windows-style paths are translated correctly",
            ),
        ]

        for mount_path, gitdir_content, expected, description in cases:
            with self.subTest(description=description), mock.patch.dict(os.environ, {"CS_MOUNT_PATH": mount_path}), tempfile.TemporaryDirectory() as tmpdir:
                worktree_dir = os.path.join(tmpdir, "worktree")
                os.makedirs(worktree_dir)
                create_git_worktree_file(worktree_dir, gitdir_content)

                result = adapt_worktree_gitdir_for_docker(worktree_dir)
                self.assertEqual(result, expected)


class TestGetWorktreeGitdir(unittest.TestCase):
    """Tests for get_worktree_gitdir() supporting static mode worktree detection."""

    def test_get_worktree_gitdir_returns_gitdir_for_worktree(self):
        """Test that get_worktree_gitdir returns gitdir path for a worktree."""
        from .docker_path_adapter import get_worktree_gitdir

        with tempfile.TemporaryDirectory() as tmpdir:
            create_git_worktree_file(tmpdir, "gitdir: /path/to/main/.git/worktrees/feature")
            result = get_worktree_gitdir(tmpdir)
            self.assertEqual("/path/to/main/.git/worktrees/feature", result)

    def test_get_worktree_gitdir_returns_none_for_regular_repo(self):
        """Test that get_worktree_gitdir returns None for regular git repo."""
        from .docker_path_adapter import get_worktree_gitdir

        with tempfile.TemporaryDirectory() as tmpdir:
            create_git_directory(tmpdir)
            result = get_worktree_gitdir(tmpdir)
            self.assertIsNone(result)

    def test_get_worktree_gitdir_returns_none_for_no_git(self):
        """Test that get_worktree_gitdir returns None when no .git exists."""
        from .docker_path_adapter import get_worktree_gitdir

        with tempfile.TemporaryDirectory() as tmpdir:
            result = get_worktree_gitdir(tmpdir)
            self.assertIsNone(result)

    def test_get_worktree_gitdir_handles_windows_paths(self):
        """Test that get_worktree_gitdir handles Windows-style paths in .git file."""
        from .docker_path_adapter import get_worktree_gitdir

        with tempfile.TemporaryDirectory() as tmpdir:
            create_git_worktree_file(tmpdir, "gitdir: C:\\workspace\\stargate\\.git\\worktrees\\ip_ps")
            result = get_worktree_gitdir(tmpdir)
            self.assertEqual("C:\\workspace\\stargate\\.git\\worktrees\\ip_ps", result)


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
        os.environ.pop("CS_MOUNT_PATH", None)

        with tempfile.TemporaryDirectory() as tmpdir:
            test_file = create_temp_git_repo_with_file(tmpdir, "src/foo.py")
            result = get_relative_file_path_for_api(test_file)
            self.assertEqual(result, "src/foo.py")

    def test_static_mode_nested_directories(self):
        """In static mode, should handle nested directory structures."""
        os.environ.pop("CS_MOUNT_PATH", None)

        with tempfile.TemporaryDirectory() as tmpdir:
            test_file = create_temp_git_repo_with_file(tmpdir, "src/components/ui/Button.tsx", "// test")
            result = get_relative_file_path_for_api(test_file)
            self.assertEqual(result, "src/components/ui/Button.tsx")

    def test_static_mode_not_in_git_repo_returns_path_as_is(self):
        """In static mode with file outside git repo, should return path unchanged."""
        os.environ.pop("CS_MOUNT_PATH", None)

        # Create a temp file NOT in a git repo
        with tempfile.NamedTemporaryFile(delete=False) as f:
            test_file = f.name

        try:
            result = get_relative_file_path_for_api(test_file)
            # Should return the path unchanged when not in a git repo
            self.assertEqual(result, test_file)
        finally:
            os.unlink(test_file)

    def test_static_mode_relative_path_returns_as_is(self):
        """In static mode with relative path, should return unchanged."""
        os.environ.pop("CS_MOUNT_PATH", None)

        result = get_relative_file_path_for_api("src/foo.py")
        self.assertEqual(result, "src/foo.py")


class TestGetRelativePathFromGitRoot(unittest.TestCase):
    """Tests for get_relative_path_from_git_root - the fix for 'not in subpath' errors."""

    def setUp(self):
        # Use realpath to resolve symlinks (macOS /var -> /private/var)
        self.temp_dir = os.path.realpath(tempfile.mkdtemp())
        # Create git repo structure
        os.makedirs(os.path.join(self.temp_dir, ".git"))
        os.makedirs(os.path.join(self.temp_dir, "src", "utils"))
        self.test_file = os.path.join(self.temp_dir, "src", "utils", "file.py")
        with open(self.test_file, "w") as f:
            f.write("# test")

    def tearDown(self):
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_absolute_path_returns_relative(self):
        """Test that absolute path is correctly converted to relative."""
        result = get_relative_path_from_git_root(self.test_file, self.temp_dir)
        # Normalize path separators for cross-platform compatibility
        self.assertEqual(result.replace("\\", "/"), "src/utils/file.py")

    def test_relative_path_resolved_correctly(self):
        """Test that relative path is resolved before computing relative path.

        This is the core fix for the 'not in subpath' bug - when file_path is
        relative but git_root is absolute, the resolution should handle this.
        """
        old_cwd = os.getcwd()
        try:
            os.chdir(self.temp_dir)
            # Use relative path from repo root
            result = get_relative_path_from_git_root("src/utils/file.py", self.temp_dir)
            self.assertEqual(result.replace("\\", "/"), "src/utils/file.py")
        finally:
            os.chdir(old_cwd)

    def test_relative_path_with_dot_prefix(self):
        """Test relative path with ./ prefix (common shell usage)."""
        old_cwd = os.getcwd()
        try:
            os.chdir(self.temp_dir)
            result = get_relative_path_from_git_root("./src/utils/file.py", self.temp_dir)
            self.assertEqual(result.replace("\\", "/"), "src/utils/file.py")
        finally:
            os.chdir(old_cwd)

    def test_relative_path_from_subdirectory(self):
        """Test relative path from a subdirectory with ../ references."""
        old_cwd = os.getcwd()
        try:
            os.chdir(os.path.join(self.temp_dir, "src", "utils"))
            # Create another file in a sibling directory
            services_dir = os.path.join(self.temp_dir, "src", "services")
            os.makedirs(services_dir)
            other_file = os.path.join(services_dir, "other.py")
            with open(other_file, "w") as f:
                f.write("# other")

            # Reference the file with ../
            result = get_relative_path_from_git_root("../services/other.py", self.temp_dir)
            self.assertEqual(result.replace("\\", "/"), "src/services/other.py")
        finally:
            os.chdir(old_cwd)

    def test_path_outside_git_root_raises_error(self):
        """Test that file outside git root raises CodeSceneCliError with details."""
        outside_file = "/tmp/outside.py"
        with self.assertRaises(CodeSceneCliError) as context:
            get_relative_path_from_git_root(outside_file, self.temp_dir)

        error_msg = str(context.exception)
        self.assertIn("is not under git root", error_msg)
        self.assertIn(outside_file, error_msg)

    def test_windows_style_backslashes(self):
        """Test handling of Windows-style backslashes in paths."""
        old_cwd = os.getcwd()
        try:
            os.chdir(self.temp_dir)
            # Windows-style path with backslashes
            windows_path = "src\\utils\\file.py"
            result = get_relative_path_from_git_root(windows_path, self.temp_dir)
            # Should normalize to forward slashes or handle consistently
            self.assertIn("file.py", result)
            self.assertIn("src", result)
            self.assertIn("utils", result)
        finally:
            os.chdir(old_cwd)

    def test_mixed_slashes(self):
        """Test handling of mixed forward/backslashes (common on Windows)."""
        old_cwd = os.getcwd()
        try:
            os.chdir(self.temp_dir)
            # Mixed slashes
            mixed_path = "src\\utils/file.py"
            result = get_relative_path_from_git_root(mixed_path, self.temp_dir)
            self.assertIn("file.py", result)
        finally:
            os.chdir(old_cwd)

    def test_git_root_with_trailing_slash(self):
        """Test that trailing slash in git_root is handled."""
        result = get_relative_path_from_git_root(self.test_file, self.temp_dir + os.sep)
        self.assertEqual(result.replace("\\", "/"), "src/utils/file.py")

    def test_deeply_nested_path(self):
        """Test deeply nested directory structures."""
        deep_dir = os.path.join(self.temp_dir, "src", "main", "java", "com", "example")
        os.makedirs(deep_dir)
        deep_file = os.path.join(deep_dir, "Test.java")
        with open(deep_file, "w") as f:
            f.write("// test")

        result = get_relative_path_from_git_root(deep_file, self.temp_dir)
        self.assertEqual(result.replace("\\", "/"), "src/main/java/com/example/Test.java")

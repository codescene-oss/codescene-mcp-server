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
                "mount": "c:\\git\\myproject",
                "user_input": "c:\\Git\\myproject",
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

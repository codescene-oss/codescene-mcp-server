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
            ("/mnt/project", "/mnt/project/src/foo.py", "/mount/src/foo.py"),
            ("/mnt/project/", "/mnt/project/src/foo.py", "/mount/src/foo.py"),
            ("/mnt/project", "/mnt/project", "/mount"),
            ("/mnt/project", "/mnt/project/", "/mount"),
            ("/", "/src/foo.py", "/mount/src/foo.py"),
            ("C:\\code\\project", "C:\\code\\project\\src\\foo.py", "/mount/src/foo.py"),
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

    @mock.patch.dict(os.environ, {"CS_MOUNT_PATH": "/mnt/project"})
    def test_relative_path_raises(self):
        with self.assertRaises(CodeSceneCliError):
            adapt_mounted_file_path_inside_docker("src/foo.py")

"""Tests for the onefile cleanup module."""

import contextlib
import logging
import logging.handlers
import os
import shutil
import tempfile
import unittest
from unittest.mock import patch

from utils.onefile_cleanup import (
    _ONEFILE_DIR_PATTERN,
    _cleanup_stale_onefile_dirs,
    _pid_is_alive,
    cleanup_stale_onefile_dirs_async,
)


class TestOnefileDirPattern(unittest.TestCase):
    """Verify the regex matches expected Nuitka onefile directory names."""

    def test_matches_standard_onefile_dir(self):
        m = _ONEFILE_DIR_PATTERN.match("onefile_12345_1699852985_413382")
        self.assertIsNotNone(m)
        self.assertEqual(m.group(1), "12345")

    def test_matches_large_pid_and_timestamp(self):
        m = _ONEFILE_DIR_PATTERN.match("onefile_9999999_9999999999_0")
        self.assertIsNotNone(m)

    def test_matches_real_world_example(self):
        m = _ONEFILE_DIR_PATTERN.match("onefile_10360_1773513624_413382")
        self.assertIsNotNone(m)
        self.assertEqual(m.group(1), "10360")

    def test_rejects_non_onefile_dir(self):
        self.assertIsNone(_ONEFILE_DIR_PATTERN.match("some_other_dir"))

    def test_rejects_partial_match(self):
        self.assertIsNone(_ONEFILE_DIR_PATTERN.match("onefile_12345"))

    def test_rejects_two_components_only(self):
        # Old assumed format — should no longer match.
        self.assertIsNone(_ONEFILE_DIR_PATTERN.match("onefile_12345_6789"))

    def test_rejects_four_components(self):
        self.assertIsNone(
            _ONEFILE_DIR_PATTERN.match("onefile_12345_6789_111_extra")
        )

    def test_rejects_non_numeric_pid(self):
        self.assertIsNone(
            _ONEFILE_DIR_PATTERN.match("onefile_abc_12345_6789")
        )


class TestPidIsAlive(unittest.TestCase):
    """Verify PID liveness checking."""

    def test_current_process_is_alive(self):
        self.assertTrue(_pid_is_alive(os.getpid()))

    def test_nonexistent_pid_is_not_alive(self):
        # Use a PID that is valid but extremely unlikely to be running.
        # macOS max PID is 99998, Linux default max is 32768 (can be raised).
        self.assertFalse(_pid_is_alive(99998))

    def test_overflow_pid_is_not_alive(self):
        # A PID exceeding platform limits should be treated as not alive.
        self.assertFalse(_pid_is_alive(4_000_000_000))

    def test_permission_error_treated_as_alive(self):
        with patch("utils.onefile_cleanup.os.kill", side_effect=PermissionError):
            self.assertTrue(_pid_is_alive(1))

    def test_generic_os_error_treated_as_dead(self):
        with patch(
            "utils.onefile_cleanup.os.kill", side_effect=OSError("Unexpected")
        ):
            self.assertFalse(_pid_is_alive(42))

    def test_process_lookup_error_treated_as_dead(self):
        with patch(
            "utils.onefile_cleanup.os.kill", side_effect=ProcessLookupError
        ):
            self.assertFalse(_pid_is_alive(42))

    def test_kill_success_treated_as_alive(self):
        with patch("utils.onefile_cleanup.os.kill", return_value=None):
            self.assertTrue(_pid_is_alive(42))


class TestCleanupStaleOnefileDirs(unittest.TestCase):
    """Verify cleanup removes only stale onefile dirs."""

    def setUp(self):
        self.tmp_dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.tmp_dir, ignore_errors=True)

    def _make_dir(self, name):
        """Create a subdirectory with a payload file and return its path."""
        path = os.path.join(self.tmp_dir, name)
        os.makedirs(path, exist_ok=True)
        with open(os.path.join(path, "payload.so"), "w") as f:
            f.write("")
        return path

    def _run_cleanup(self, **extra_patches):
        """Run _cleanup_stale_onefile_dirs with tempdir patched to self.tmp_dir.

        *extra_patches* are forwarded to ``unittest.mock.patch`` and stacked
        on top of the base gettempdir patch.
        """
        stack = contextlib.ExitStack()
        stack.enter_context(patch(
            "utils.onefile_cleanup.tempfile.gettempdir",
            return_value=self.tmp_dir,
        ))
        for target, kwargs in extra_patches.items():
            stack.enter_context(patch(target, **kwargs))
        with stack:
            _cleanup_stale_onefile_dirs()

    def test_removes_stale_directory(self):
        stale = self._make_dir("onefile_4000000000_1699852985_413382")
        self._run_cleanup()
        self.assertFalse(os.path.exists(stale))

    def test_preserves_directory_for_running_pid(self):
        live = self._make_dir(f"onefile_{os.getpid()}_1699852985_413382")
        self._run_cleanup()
        self.assertTrue(os.path.exists(live))

    def test_ignores_non_onefile_directories(self):
        other = self._make_dir("some_other_temp_dir")
        self._run_cleanup()
        self.assertTrue(os.path.exists(other))

    def test_handles_permission_error_gracefully(self):
        stale = self._make_dir("onefile_4000000000_1699852985_413382")
        self._run_cleanup(**{
            "utils.onefile_cleanup.shutil.rmtree": {
                "side_effect": OSError("Permission denied"),
            },
        })
        # Directory still exists because rmtree was mocked to fail.
        self.assertTrue(os.path.exists(stale))

    def test_handles_unreadable_tempdir(self):
        with patch(
            "utils.onefile_cleanup.tempfile.gettempdir",
            return_value="/nonexistent/path",
        ):
            _cleanup_stale_onefile_dirs()  # should not raise

    def test_removes_multiple_stale_preserves_live(self):
        """Mixed scenario: several stale dirs, one live dir, one non-onefile dir."""
        stale_1 = self._make_dir("onefile_4000000001_1699852985_100")
        stale_2 = self._make_dir("onefile_4000000002_1699852985_200")
        live = self._make_dir(f"onefile_{os.getpid()}_1699852985_300")
        unrelated = self._make_dir("other_temp_dir")

        self._run_cleanup()

        self.assertFalse(os.path.exists(stale_1))
        self.assertFalse(os.path.exists(stale_2))
        self.assertTrue(os.path.exists(live))
        self.assertTrue(os.path.exists(unrelated))

    def test_logs_removed_directory(self):
        self._make_dir("onefile_4000000000_1699852985_413382")

        with patch(
            "utils.onefile_cleanup.tempfile.gettempdir",
            return_value=self.tmp_dir,
        ), self.assertLogs("utils.onefile_cleanup", level=logging.DEBUG) as cm:
            _cleanup_stale_onefile_dirs()

        self.assertTrue(
            any("Removed stale onefile directory" in msg for msg in cm.output)
        )

    def test_no_log_when_nothing_to_clean(self):
        # Only a non-matching directory present.
        self._make_dir("regular_dir")

        logger = logging.getLogger("utils.onefile_cleanup")
        handler = logging.handlers.MemoryHandler(capacity=100)
        logger.addHandler(handler)
        try:
            self._run_cleanup()
            handler.flush()
            messages = [r.getMessage() for r in handler.buffer]
            self.assertFalse(
                any("Removed stale onefile directory" in m for m in messages)
            )
        finally:
            logger.removeHandler(handler)

    def test_empty_tempdir(self):
        """An empty temp directory should not cause errors."""
        self._run_cleanup()  # should not raise

    def test_listdir_os_error_is_handled(self):
        """OSError from os.listdir (e.g. permission denied on /tmp) is swallowed."""
        with patch(
            "utils.onefile_cleanup.tempfile.gettempdir",
            return_value="/some/path",
        ), patch(
            "utils.onefile_cleanup.os.listdir",
            side_effect=OSError("Permission denied"),
        ):
            _cleanup_stale_onefile_dirs()  # should not raise


class TestCleanupStaleOnefileDirsAsync(unittest.TestCase):
    """Verify the async wrapper respects environment checks."""

    def _reset_env_cache(self):
        """Reset the cached environment so get_environment() re-evaluates."""
        import utils.environment as env_mod

        env_mod._cached_environment = None

    def setUp(self):
        self._reset_env_cache()

    def tearDown(self):
        self._reset_env_cache()

    @contextlib.contextmanager
    def _simulate_environment(self, *, is_nuitka, docker_mount=None):
        """Set up environment patches and yield the mock Thread class."""
        env_patches = [
            patch("utils.environment._is_nuitka_environment", return_value=is_nuitka),
            patch.dict(os.environ, {}, clear=False),
            patch("utils.onefile_cleanup.threading.Thread"),
        ]
        if docker_mount is not None:
            env_patches.insert(1, patch.dict(
                os.environ, {"CS_MOUNT_PATH": docker_mount}
            ))

        stack = contextlib.ExitStack()
        for p in env_patches:
            mock_or_ctx = stack.enter_context(p)
        # The last patch is threading.Thread — that's our mock.
        mock_thread = mock_or_ctx
        with stack:
            if docker_mount is None:
                os.environ.pop("CS_MOUNT_PATH", None)
            yield mock_thread

    def test_noop_when_not_nuitka(self):
        with self._simulate_environment(is_nuitka=False) as mock_thread:
            cleanup_stale_onefile_dirs_async()
            mock_thread.assert_not_called()

    def test_starts_thread_when_nuitka(self):
        with self._simulate_environment(is_nuitka=True) as mock_thread:
            mock_instance = mock_thread.return_value
            cleanup_stale_onefile_dirs_async()
            mock_thread.assert_called_once_with(
                target=_cleanup_stale_onefile_dirs, daemon=True
            )
            mock_instance.start.assert_called_once()

    def test_noop_when_docker(self):
        with self._simulate_environment(
            is_nuitka=False, docker_mount="/some/path"
        ) as mock_thread:
            cleanup_stale_onefile_dirs_async()
            mock_thread.assert_not_called()

"""Tests for the onefile cleanup module."""

import os
from unittest.mock import patch

from utils.onefile_cleanup import (
    _ONEFILE_DIR_PATTERN,
    _cleanup_stale_onefile_dirs,
    _pid_is_alive,
    cleanup_stale_onefile_dirs_async,
)


class TestOnefileDirPattern:
    """Verify the regex matches expected Nuitka onefile directory names."""

    def test_matches_standard_onefile_dir(self):
        m = _ONEFILE_DIR_PATTERN.match("onefile_12345_1699852985_413382")
        assert m is not None
        assert m.group(1) == "12345"

    def test_matches_large_pid_and_timestamp(self):
        m = _ONEFILE_DIR_PATTERN.match("onefile_9999999_9999999999_0")
        assert m is not None

    def test_matches_real_world_example(self):
        m = _ONEFILE_DIR_PATTERN.match("onefile_10360_1773513624_413382")
        assert m is not None
        assert m.group(1) == "10360"

    def test_rejects_non_onefile_dir(self):
        assert _ONEFILE_DIR_PATTERN.match("some_other_dir") is None

    def test_rejects_partial_match(self):
        assert _ONEFILE_DIR_PATTERN.match("onefile_12345") is None

    def test_rejects_two_components_only(self):
        # Old assumed format — should no longer match
        assert _ONEFILE_DIR_PATTERN.match("onefile_12345_6789") is None

    def test_rejects_four_components(self):
        assert _ONEFILE_DIR_PATTERN.match("onefile_12345_6789_111_extra") is None

    def test_rejects_non_numeric_pid(self):
        assert _ONEFILE_DIR_PATTERN.match("onefile_abc_12345_6789") is None


class TestPidIsAlive:
    """Verify PID liveness checking."""

    def test_current_process_is_alive(self):
        assert _pid_is_alive(os.getpid()) is True

    def test_nonexistent_pid_is_not_alive(self):
        # Use a PID that is valid but extremely unlikely to be running.
        # macOS max PID is 99998, Linux default max is 32768 (can be raised).
        assert _pid_is_alive(99998) is False

    def test_overflow_pid_is_not_alive(self):
        # A PID exceeding platform limits should be treated as not alive.
        assert _pid_is_alive(4_000_000_000) is False

    def test_permission_error_treated_as_alive(self):
        with patch("utils.onefile_cleanup.os.kill", side_effect=PermissionError):
            assert _pid_is_alive(1) is True


class TestCleanupStaleOnefileDirs:
    """Verify cleanup removes only stale onefile dirs."""

    def test_removes_stale_directory(self, tmp_path):
        stale_dir = tmp_path / "onefile_4000000000_1699852985_413382"
        stale_dir.mkdir()
        (stale_dir / "some_file.so").touch()

        with patch("utils.onefile_cleanup.tempfile.gettempdir", return_value=str(tmp_path)):
            _cleanup_stale_onefile_dirs()

        assert not stale_dir.exists()

    def test_preserves_directory_for_running_pid(self, tmp_path):
        live_dir = tmp_path / f"onefile_{os.getpid()}_1699852985_413382"
        live_dir.mkdir()
        (live_dir / "some_file.so").touch()

        with patch("utils.onefile_cleanup.tempfile.gettempdir", return_value=str(tmp_path)):
            _cleanup_stale_onefile_dirs()

        assert live_dir.exists()

    def test_ignores_non_onefile_directories(self, tmp_path):
        other_dir = tmp_path / "some_other_temp_dir"
        other_dir.mkdir()

        with patch("utils.onefile_cleanup.tempfile.gettempdir", return_value=str(tmp_path)):
            _cleanup_stale_onefile_dirs()

        assert other_dir.exists()

    def test_handles_permission_error_gracefully(self, tmp_path):
        stale_dir = tmp_path / "onefile_4000000000_1699852985_413382"
        stale_dir.mkdir()

        with (
            patch("utils.onefile_cleanup.tempfile.gettempdir", return_value=str(tmp_path)),
            patch("utils.onefile_cleanup.shutil.rmtree", side_effect=OSError("Permission denied")),
        ):
            # Should not raise
            _cleanup_stale_onefile_dirs()

        # Directory still exists because rmtree was mocked to fail
        assert stale_dir.exists()

    def test_handles_unreadable_tempdir(self):
        with patch(
            "utils.onefile_cleanup.tempfile.gettempdir",
            return_value="/nonexistent/path",
        ):
            # Should not raise
            _cleanup_stale_onefile_dirs()


class TestCleanupStaleOnefileDirsAsync:
    """Verify the async wrapper respects environment checks."""

    def _reset_env_cache(self):
        """Reset the cached environment so get_environment() re-evaluates."""
        import utils.environment as env_mod

        env_mod._cached_environment = None

    def test_noop_when_not_nuitka(self):
        self._reset_env_cache()
        with (
            patch("utils.environment._is_nuitka_environment", return_value=False),
            patch.dict(os.environ, {}, clear=False),
            patch("utils.onefile_cleanup.threading.Thread") as mock_thread,
        ):
            os.environ.pop("CS_MOUNT_PATH", None)
            cleanup_stale_onefile_dirs_async()
            mock_thread.assert_not_called()
        self._reset_env_cache()

    def test_starts_thread_when_nuitka(self):
        self._reset_env_cache()
        with (
            patch("utils.environment._is_nuitka_environment", return_value=True),
            patch.dict(os.environ, {}, clear=False),
            patch("utils.onefile_cleanup.threading.Thread") as mock_thread,
        ):
            os.environ.pop("CS_MOUNT_PATH", None)
            mock_instance = mock_thread.return_value
            cleanup_stale_onefile_dirs_async()
            mock_thread.assert_called_once_with(
                target=_cleanup_stale_onefile_dirs, daemon=True
            )
            mock_instance.start.assert_called_once()
        self._reset_env_cache()

    def test_noop_when_docker(self):
        self._reset_env_cache()
        with (
            patch.dict(os.environ, {"CS_MOUNT_PATH": "/some/path"}),
            patch("utils.onefile_cleanup.threading.Thread") as mock_thread,
        ):
            cleanup_stale_onefile_dirs_async()
            mock_thread.assert_not_called()
        self._reset_env_cache()

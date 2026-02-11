import os
import threading
import time
import unittest
from unittest.mock import MagicMock, patch

from utils.version_checker import VersionChecker, VersionInfo, with_version_check


class TestGetLatestVersion(unittest.TestCase):
    @patch("requests.get")
    def test_successful_fetch(self, mock_get):
        mock_response = MagicMock()
        mock_response.json.return_value = {"tag_name": "MCP-1.2.3"}
        mock_get.return_value = mock_response

        version = VersionChecker.get_latest_version()
        self.assertEqual(version, "MCP-1.2.3")

    @patch("requests.get")
    def test_failed_fetch(self, mock_get):
        mock_get.side_effect = Exception("Network error")

        version = VersionChecker.get_latest_version()
        self.assertIsNone(version)


class TestVersionChecker(unittest.TestCase):
    def setUp(self):
        self.checker = VersionChecker(cache_duration=3600)

    @patch("utils.version_checker.__version__", "dev")
    def test_dev_version_returns_immediately(self):
        """Dev version should return a cached result immediately without fetching."""
        result = self.checker.get_cached_or_trigger_fetch()

        self.assertIsNotNone(result)
        self.assertEqual(result.current, "dev")
        self.assertFalse(result.outdated)
        self.assertIn("development", result.message.lower())

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.object(VersionChecker, "get_latest_version")
    def test_first_call_returns_none_and_starts_background_fetch(self, mock_get_latest):
        """First call with no cache should return None and start a background fetch."""
        fetch_started = threading.Event()
        fetch_proceed = threading.Event()

        original_return = "MCP-2.0.0"

        def slow_fetch():
            fetch_started.set()
            fetch_proceed.wait(timeout=5)
            return original_return

        mock_get_latest.side_effect = slow_fetch

        result = self.checker.get_cached_or_trigger_fetch()

        # First call should return None (no cached result yet)
        self.assertIsNone(result)

        # Wait for the background thread to start
        self.assertTrue(fetch_started.wait(timeout=2))

        # Let the fetch complete
        fetch_proceed.set()

        # Wait for the background thread to finish
        self.checker._fetch_thread.join(timeout=2)

        # Now a second call should return the cached result
        result = self.checker.get_cached_or_trigger_fetch()
        self.assertIsNotNone(result)
        self.assertEqual(result.current, "MCP-1.0.0")
        self.assertEqual(result.latest, "MCP-2.0.0")
        self.assertTrue(result.outdated)

    @patch("utils.version_checker.__version__", "MCP-2.0.0")
    @patch.object(VersionChecker, "get_latest_version")
    def test_current_version_not_outdated(self, mock_get_latest):
        """When current matches latest, outdated should be False."""
        mock_get_latest.return_value = "MCP-2.0.0"

        # First call triggers background fetch
        result = self.checker.get_cached_or_trigger_fetch()
        self.assertIsNone(result)

        # Wait for background fetch to complete
        self.checker._fetch_thread.join(timeout=2)

        result = self.checker.get_cached_or_trigger_fetch()
        self.assertIsNotNone(result)
        self.assertEqual(result.current, "MCP-2.0.0")
        self.assertEqual(result.latest, "MCP-2.0.0")
        self.assertFalse(result.outdated)
        self.assertEqual(result.message, "")

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.object(VersionChecker, "get_latest_version")
    def test_failed_fetch_is_cached(self, mock_get_latest):
        """A failed fetch should be cached so subsequent calls don't retry."""
        mock_get_latest.return_value = None

        # First call triggers background fetch
        result = self.checker.get_cached_or_trigger_fetch()
        self.assertIsNone(result)

        # Wait for background fetch to complete
        self.checker._fetch_thread.join(timeout=2)

        # The failed result should now be cached
        result = self.checker.get_cached_or_trigger_fetch()
        self.assertIsNotNone(result)
        self.assertFalse(result.outdated)

        # get_latest_version should only have been called once (cached failure)
        self.assertEqual(mock_get_latest.call_count, 1)

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.object(VersionChecker, "get_latest_version")
    def test_cached_result_is_reused(self, mock_get_latest):
        """Cached results should be reused without re-fetching."""
        mock_get_latest.return_value = "MCP-2.0.0"

        # First call triggers background fetch
        self.checker.get_cached_or_trigger_fetch()
        self.checker._fetch_thread.join(timeout=2)

        # Multiple subsequent calls should use the cache
        result1 = self.checker.get_cached_or_trigger_fetch()
        result2 = self.checker.get_cached_or_trigger_fetch()

        self.assertEqual(result1, result2)
        # Only one fetch should have occurred
        self.assertEqual(mock_get_latest.call_count, 1)

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.object(VersionChecker, "get_latest_version")
    def test_no_duplicate_concurrent_fetches(self, mock_get_latest):
        """Multiple calls while a fetch is in progress should not spawn extra threads."""
        fetch_proceed = threading.Event()

        def slow_fetch():
            fetch_proceed.wait(timeout=5)
            return "MCP-2.0.0"

        mock_get_latest.side_effect = slow_fetch

        # Trigger the first background fetch
        self.checker.get_cached_or_trigger_fetch()

        # Call again while fetch is still running
        self.checker.get_cached_or_trigger_fetch()
        self.checker.get_cached_or_trigger_fetch()

        # Let the fetch complete
        fetch_proceed.set()
        self.checker._fetch_thread.join(timeout=2)

        # Only one fetch should have been made
        self.assertEqual(mock_get_latest.call_count, 1)

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.object(VersionChecker, "get_latest_version")
    def test_outdated_version_message_content(self, mock_get_latest):
        """The outdated message should mention all installation methods."""
        mock_get_latest.return_value = "MCP-2.0.0"

        self.checker.get_cached_or_trigger_fetch()
        self.checker._fetch_thread.join(timeout=2)

        result = self.checker.get_cached_or_trigger_fetch()
        self.assertIn("outdated", result.message.lower())
        self.assertIn("brew upgrade", result.message)
        self.assertIn("Windows", result.message)
        self.assertIn("docker pull", result.message)
        self.assertIn("github.com", result.message.lower())


class TestWithVersionCheck(unittest.TestCase):
    @patch("utils.version_checker.check_version")
    def test_decorator_with_current_version(self, mock_check):
        mock_check.return_value = VersionInfo(
            current="MCP-1.0.0",
            latest="MCP-1.0.0",
            outdated=False,
            message="Up to date",
        )

        @with_version_check
        def sample_tool():
            return "Tool result"

        result = sample_tool()
        self.assertEqual(result, "Tool result")

    @patch("utils.version_checker.check_version")
    def test_decorator_with_outdated_version(self, mock_check):
        mock_check.return_value = VersionInfo(
            current="MCP-1.0.0",
            latest="MCP-2.0.0",
            outdated=True,
            message="Version outdated",
        )

        @with_version_check
        def sample_tool():
            return "Tool result"

        result = sample_tool()
        self.assertIn("Version outdated", result)
        self.assertIn("Tool result", result)
        self.assertIn("VERSION UPDATE AVAILABLE", result)
        self.assertIn("=" * 80, result)

    @patch("utils.version_checker.check_version")
    def test_decorator_returns_result_when_no_cache(self, mock_check):
        """When check_version returns None (fetch in progress), the tool result is returned as-is."""
        mock_check.return_value = None

        @with_version_check
        def sample_tool():
            return "Tool result"

        result = sample_tool()
        self.assertEqual(result, "Tool result")

    @patch("utils.version_checker.check_version")
    def test_decorator_fails_silently_on_exception(self, mock_check):
        mock_check.side_effect = Exception("Unexpected error")

        @with_version_check
        def sample_tool():
            return "Tool result"

        result = sample_tool()
        # Should return result normally despite version check failure
        self.assertEqual(result, "Tool result")


class TestBuildVersionInfo(unittest.TestCase):
    """Tests for the _build_version_info helper."""

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    def test_build_with_none_latest(self):
        checker = VersionChecker()
        info = checker._build_version_info(None)
        self.assertEqual(info.current, "MCP-1.0.0")
        self.assertIsNone(info.latest)
        self.assertFalse(info.outdated)
        self.assertEqual(info.message, "")

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    def test_build_with_matching_version(self):
        checker = VersionChecker()
        info = checker._build_version_info("MCP-1.0.0")
        self.assertFalse(info.outdated)

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    def test_build_with_newer_version(self):
        checker = VersionChecker()
        info = checker._build_version_info("MCP-2.0.0")
        self.assertTrue(info.outdated)
        self.assertIn("outdated", info.message.lower())


class TestDisableVersionCheck(unittest.TestCase):
    """Tests for CS_DISABLE_VERSION_CHECK support."""

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.dict(os.environ, {"CS_DISABLE_VERSION_CHECK": "1"})
    @patch.object(VersionChecker, "get_latest_version")
    def test_disabled_returns_immediately_without_fetching(self, mock_get_latest):
        """When disabled, should return a result immediately and never fetch."""
        checker = VersionChecker()
        result = checker.get_cached_or_trigger_fetch()

        self.assertIsNotNone(result)
        self.assertFalse(result.outdated)
        self.assertIsNone(result.latest)
        self.assertIn("disabled", result.message.lower())
        mock_get_latest.assert_not_called()

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.dict(os.environ, {"CS_DISABLE_VERSION_CHECK": "true"})
    def test_disabled_does_not_start_background_thread(self):
        """When disabled, no background thread should be spawned."""
        checker = VersionChecker()
        checker.get_cached_or_trigger_fetch()

        self.assertIsNone(checker._fetch_thread)

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.dict(os.environ, {"CS_DISABLE_VERSION_CHECK": "1"})
    def test_disabled_decorator_returns_plain_result(self):
        """The decorator should return tool output as-is when version check is disabled."""

        @with_version_check
        def sample_tool():
            return "Tool result"

        result = sample_tool()
        self.assertEqual(result, "Tool result")
        self.assertNotIn("VERSION UPDATE AVAILABLE", result)

    @patch("utils.version_checker.__version__", "MCP-1.0.0")
    @patch.dict(os.environ, {}, clear=False)
    @patch.object(VersionChecker, "get_latest_version")
    def test_empty_env_var_does_not_disable(self, mock_get_latest):
        """An empty CS_DISABLE_VERSION_CHECK value should not disable the check."""
        os.environ.pop("CS_DISABLE_VERSION_CHECK", None)
        mock_get_latest.return_value = "MCP-2.0.0"

        checker = VersionChecker()
        result = checker.get_cached_or_trigger_fetch()

        # Should behave normally: first call returns None and triggers a fetch
        self.assertIsNone(result)
        checker._fetch_thread.join(timeout=2)
        mock_get_latest.assert_called_once()


if __name__ == "__main__":
    unittest.main()

import os
import time
import unittest
from unittest.mock import patch

FAKE_COMMON_PROPERTIES = {
    "instance-id": "test-instance-id",
    "environment": "source",
    "version": "test-version",
}

FAKE_API_URL = "https://api.example.com"
FAKE_HEADERS = {"Authorization": "Bearer token"}

# Shared decorator stack applied to every test that exercises the HTTP POST path.
_common_patches = [
    patch("utils.track._get_common_properties", return_value=FAKE_COMMON_PROPERTIES),
    patch("utils.track.requests.post"),
    patch("utils.track.get_api_url", return_value=FAKE_API_URL),
    patch("utils.track.get_api_request_headers", return_value=FAKE_HEADERS),
]


def _apply_common_patches(fn):
    """Apply the four standard mocks expected by every tracking test."""
    for p in reversed(_common_patches):
        fn = p(fn)
    return fn


def _expected_post_call(event_type, extra_properties=None):
    """Return the expected ``requests.post(...)`` kwargs for a tracking call."""
    props = {**FAKE_COMMON_PROPERTIES, **(extra_properties or {})}
    return dict(
        args=(f"{FAKE_API_URL}/v2/analytics/track",),
        kwargs=dict(
            headers=FAKE_HEADERS,
            json={"event-type": f"mcp-{event_type}", "event-properties": props},
            timeout=5,
        ),
    )


class TestTrack(unittest.TestCase):
    # -- helpers ----------------------------------------------------------

    @staticmethod
    def _invoke(event_type, property_extractor=None, return_value="result"):
        """Create, invoke, and drain a tracked tool method."""
        from utils.track import _wait_for_pending, track

        class Tool:
            @track(event_type, property_extractor)
            def run(self):
                return return_value

        result = Tool().run()
        _wait_for_pending()
        return result

    @staticmethod
    def _invoke_with_args(event_type, property_extractor, args, return_value="result"):
        """Create, invoke with positional args, and drain a tracked tool method."""
        from utils.track import _wait_for_pending, track

        class Tool:
            @track(event_type, property_extractor)
            def run(self, file_path):
                return return_value

        result = Tool().run(*args)
        _wait_for_pending()
        return result

    @staticmethod
    def _assert_single_post(mock_post, event_type, extra_properties=None):
        expected = _expected_post_call(event_type, extra_properties)
        mock_post.assert_called_once_with(*expected["args"], **expected["kwargs"])

    # -- tests ------------------------------------------------------------

    @_apply_common_patches
    def test_track_decorator_sends_event_with_common_properties(self, mock_headers, mock_url, mock_post, mock_common):
        result = self._invoke("my-event")
        self.assertEqual(result, "result")
        self._assert_single_post(mock_post, "my-event")

    @_apply_common_patches
    def test_track_decorator_with_no_extractor(self, mock_headers, mock_url, mock_post, mock_common):
        self._invoke("simple-event", return_value="ok")
        self._assert_single_post(mock_post, "simple-event")

    @_apply_common_patches
    def test_track_decorator_merges_extractor_properties(self, mock_headers, mock_url, mock_post, mock_common):
        def my_extractor(result, **_kw):
            return {"score": "9.5", "file-hash": "abc123"}

        result = self._invoke("my-event", my_extractor)
        self.assertEqual(result, "result")
        self._assert_single_post(mock_post, "my-event", {"score": "9.5", "file-hash": "abc123"})

    @_apply_common_patches
    def test_track_decorator_extractor_receives_result_and_args(self, mock_headers, mock_url, mock_post, mock_common):
        captured = {}

        def capturing_extractor(result, file_path, **_kw):
            captured["result"] = result
            captured["file_path"] = file_path
            return {"file-hash": "captured"}

        self._invoke_with_args("my-event", capturing_extractor, args=("/path/to/file.py",), return_value="score: 10.0")
        self.assertEqual(captured["result"], "score: 10.0")
        self.assertEqual(captured["file_path"], "/path/to/file.py")

    @_apply_common_patches
    def test_track_decorator_silences_extractor_failure(self, mock_headers, mock_url, mock_post, mock_common):
        def failing_extractor(result, **_kw):
            raise RuntimeError("extractor broke")

        result = self._invoke("my-event", failing_extractor)
        self.assertEqual(result, "result")
        self._assert_single_post(mock_post, "my-event")

    @_apply_common_patches
    def test_track_error_sends_error_event_with_common_properties(self, mock_headers, mock_url, mock_post, mock_common):
        from utils.track import _wait_for_pending, track_error

        track_error("my-event", ValueError("Something went wrong"))
        _wait_for_pending()
        self._assert_single_post(mock_post, "my-event-error", {"error": "Something went wrong"})

    @_apply_common_patches
    def test_track_error_appends_error_suffix_to_event_type(self, mock_headers, mock_url, mock_post, mock_common):
        from utils.track import _wait_for_pending, track_error

        track_error("select-project", Exception("API failed"))
        _wait_for_pending()

        event_type = mock_post.call_args[1]["json"]["event-type"]
        self.assertEqual(event_type, "mcp-select-project-error")

    @_apply_common_patches
    def test_track_decorator_fails_silently_on_network_error(self, mock_headers, mock_url, mock_post, mock_common):
        mock_post.side_effect = Exception("Network error")
        result = self._invoke("my-event")
        self.assertEqual(result, "result")

    @_apply_common_patches
    def test_track_error_fails_silently_on_network_error(self, mock_headers, mock_url, mock_post, mock_common):
        from utils.track import _wait_for_pending, track_error

        mock_post.side_effect = Exception("Network error")
        track_error("my-event", ValueError("Some error"))
        _wait_for_pending()

    @_apply_common_patches
    def test_track_does_not_block_on_slow_network(self, mock_headers, mock_url, mock_post, mock_common):
        def slow_post(*args, **kwargs):
            time.sleep(2)

        mock_post.side_effect = slow_post

        from utils.track import _wait_for_pending, track

        class MyTool:
            @track("slow-event")
            def my_method(self):
                return "fast"

        start = time.monotonic()
        result = MyTool().my_method()
        elapsed = time.monotonic() - start

        # The decorated method must return immediately (well under the 2s
        # sleep in the mock) because tracking runs in a background thread.
        self.assertEqual(result, "fast")
        self.assertLess(elapsed, 0.5, f"track decorator blocked for {elapsed:.2f}s; expected <0.5s")

        _wait_for_pending()

    @patch("utils.track.get_api_url", return_value="https://api.default.com")
    def test_get_tracking_url_uses_env_override(self, mock_url):
        from utils.track import _get_tracking_url

        with patch.dict(os.environ, {"CS_TRACKING_URL": "https://custom.tracking.io"}):
            self.assertEqual(_get_tracking_url(), "https://custom.tracking.io")

    @patch("utils.track.get_api_url", return_value="https://api.default.com")
    def test_get_tracking_url_falls_back_to_api_url(self, mock_url):
        from utils.track import _get_tracking_url

        with patch.dict(os.environ, {}, clear=False):
            os.environ.pop("CS_TRACKING_URL", None)
            self.assertEqual(_get_tracking_url(), "https://api.default.com")

    @_apply_common_patches
    def test_disabled_tracking_skips_post_on_decorator(self, mock_headers, mock_url, mock_post, mock_common):
        with patch.dict(os.environ, {"CS_DISABLE_TRACKING": "1"}):
            result = self._invoke("my-event")
        self.assertEqual(result, "result")
        mock_post.assert_not_called()

    @_apply_common_patches
    def test_disabled_tracking_skips_post_on_track_error(self, mock_headers, mock_url, mock_post, mock_common):
        from utils.track import _wait_for_pending, track_error

        with patch.dict(os.environ, {"CS_DISABLE_TRACKING": "1"}):
            track_error("my-event", ValueError("boom"))
            _wait_for_pending()
        mock_post.assert_not_called()

    @_apply_common_patches
    def test_empty_disable_tracking_env_does_not_disable(self, mock_headers, mock_url, mock_post, mock_common):
        with patch.dict(os.environ, {}, clear=False):
            os.environ.pop("CS_DISABLE_TRACKING", None)
            self._invoke("my-event")
        mock_post.assert_called_once()


if __name__ == "__main__":
    unittest.main()

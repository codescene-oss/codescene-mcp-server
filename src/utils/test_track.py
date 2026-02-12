import os
import time
import unittest
from unittest.mock import patch


class TestTrack(unittest.TestCase):
    def _invoke_tracked_method(self, event_type, event_properties=None, return_value="result"):
        """Create a tool class with a tracked method, invoke it, and wait for the background thread."""
        from utils.track import _wait_for_pending, track

        class MyTool:
            @track(event_type, event_properties)
            def my_method(self):
                return return_value

        tool = MyTool()
        result = tool.my_method()
        _wait_for_pending()
        return result

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_track_decorator_sends_event_on_success(self, mock_headers, mock_url, mock_post):
        result = self._invoke_tracked_method("my-event", {"key": "value"})

        self.assertEqual(result, "result")
        mock_post.assert_called_once_with(
            "https://api.example.com/v2/analytics/track",
            headers={"Authorization": "Bearer token"},
            json={"event-type": "mcp-my-event", "event-properties": {"key": "value"}},
            timeout=5,
        )

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_track_decorator_with_no_properties(self, mock_headers, mock_url, mock_post):
        self._invoke_tracked_method("simple-event", return_value="ok")

        mock_post.assert_called_once_with(
            "https://api.example.com/v2/analytics/track",
            headers={"Authorization": "Bearer token"},
            json={"event-type": "mcp-simple-event", "event-properties": {}},
            timeout=5,
        )

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_track_error_sends_error_event(self, mock_headers, mock_url, mock_post):
        from utils.track import _wait_for_pending, track_error

        error = ValueError("Something went wrong")
        track_error("my-event", error)
        _wait_for_pending()

        mock_post.assert_called_once_with(
            "https://api.example.com/v2/analytics/track",
            headers={"Authorization": "Bearer token"},
            json={
                "event-type": "mcp-my-event-error",
                "event-properties": {"error": "Something went wrong"},
            },
            timeout=5,
        )

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_track_error_appends_error_suffix_to_event_type(self, mock_headers, mock_url, mock_post):
        from utils.track import _wait_for_pending, track_error

        track_error("select-project", Exception("API failed"))
        _wait_for_pending()

        call_args = mock_post.call_args
        event_type = call_args[1]["json"]["event-type"]
        self.assertEqual(event_type, "mcp-select-project-error")

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_track_decorator_fails_silently_on_network_error(self, mock_headers, mock_url, mock_post):
        from utils.track import _wait_for_pending, track

        mock_post.side_effect = Exception("Network error")

        class MyTool:
            @track("my-event")
            def my_method(self):
                return "result"

        tool = MyTool()
        result = tool.my_method()
        _wait_for_pending()

        # Should return result normally despite tracking failure
        self.assertEqual(result, "result")

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_track_error_fails_silently_on_network_error(self, mock_headers, mock_url, mock_post):
        from utils.track import _wait_for_pending, track_error

        mock_post.side_effect = Exception("Network error")

        # Should not raise an exception
        track_error("my-event", ValueError("Some error"))
        _wait_for_pending()

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_track_does_not_block_on_slow_network(self, mock_headers, mock_url, mock_post):
        from utils.track import _wait_for_pending, track

        def slow_post(*args, **kwargs):
            time.sleep(2)

        mock_post.side_effect = slow_post

        class MyTool:
            @track("slow-event")
            def my_method(self):
                return "fast"

        tool = MyTool()
        start = time.monotonic()
        result = tool.my_method()
        elapsed = time.monotonic() - start

        # The decorated method must return immediately (well under the 2s
        # sleep in the mock) because tracking runs in a background thread.
        self.assertEqual(result, "fast")
        self.assertLess(elapsed, 0.5, f"track decorator blocked for {elapsed:.2f}s; expected <0.5s")

        # Clean up the background thread so it doesn't leak into other tests.
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
            # Ensure CS_TRACKING_URL is not set
            os.environ.pop("CS_TRACKING_URL", None)
            self.assertEqual(_get_tracking_url(), "https://api.default.com")

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_disabled_tracking_skips_post_on_decorator(self, mock_headers, mock_url, mock_post):
        with patch.dict(os.environ, {"CS_DISABLE_TRACKING": "1"}):
            result = self._invoke_tracked_method("my-event", {"key": "value"})

        self.assertEqual(result, "result")
        mock_post.assert_not_called()

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_disabled_tracking_skips_post_on_track_error(self, mock_headers, mock_url, mock_post):
        from utils.track import _wait_for_pending, track_error

        with patch.dict(os.environ, {"CS_DISABLE_TRACKING": "1"}):
            track_error("my-event", ValueError("boom"))
            _wait_for_pending()

        mock_post.assert_not_called()

    @patch("utils.track.requests.post")
    @patch("utils.track.get_api_url", return_value="https://api.example.com")
    @patch(
        "utils.track.get_api_request_headers",
        return_value={"Authorization": "Bearer token"},
    )
    def test_empty_disable_tracking_env_does_not_disable(self, mock_headers, mock_url, mock_post):
        with patch.dict(os.environ, {}, clear=False):
            os.environ.pop("CS_DISABLE_TRACKING", None)
            self._invoke_tracked_method("my-event")

        mock_post.assert_called_once()


if __name__ == "__main__":
    unittest.main()

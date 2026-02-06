import functools

import requests

from utils import get_api_request_headers, get_api_url


def _send_track_event(event_type: str, event_properties: dict = None):
    """Send a tracking event to the analytics API.

    Fails silently - analytics should never interrupt user workflow.
    """
    try:
        payload = {"event-type": event_type, "event-properties": event_properties or {}}
        requests.post(
            f"{get_api_url()}/v2/analytics/track",
            headers=get_api_request_headers(),
            json=payload,
        )
    except Exception:
        pass


def track(event_type: str, event_properties: dict = None):
    def wrapper(f):
        @functools.wraps(f)
        def wrapped(self, *f_args, **f_kwargs):
            result = f(self, *f_args, **f_kwargs)
            _send_track_event(f"mcp-{event_type}", event_properties)
            return result

        return wrapped

    return wrapper


def track_error(event_type: str, error: Exception):
    """Track an error event manually. Call this from exception handlers in tools."""
    _send_track_event(f"mcp-{event_type}-error", {"error": str(error)})

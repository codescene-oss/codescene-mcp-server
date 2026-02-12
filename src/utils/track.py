import functools
import os
import threading

import requests

from utils import get_api_request_headers, get_api_url

TRACK_TIMEOUT = 5  # seconds


def _is_tracking_disabled() -> bool:
    """Return True when the user has opted out of analytics tracking."""
    return bool(os.environ.get("CS_DISABLE_TRACKING"))


def _get_tracking_url() -> str:
    """Return the base URL used for analytics tracking.

    Reads from the CS_TRACKING_URL environment variable if set,
    otherwise falls back to get_api_url().  This allows integration
    tests to redirect tracking to a local server or an unreachable
    address without affecting CS_ONPREM_URL (which the cs CLI also
    reads for license validation).
    """
    override = os.environ.get("CS_TRACKING_URL")
    if override:
        return override
    return get_api_url()


_pending_threads: list[threading.Thread] = []
_pending_lock = threading.Lock()


def _send_track_event(event_type: str, event_properties: dict | None = None):
    """Send a tracking event to the analytics API.

    Fails silently - analytics should never interrupt user workflow.
    """
    try:
        payload = {"event-type": event_type, "event-properties": event_properties or {}}
        requests.post(
            f"{_get_tracking_url()}/v2/analytics/track",
            headers=get_api_request_headers(),
            json=payload,
            timeout=TRACK_TIMEOUT,
        )
    except Exception:
        pass


def _send_track_event_in_background(event_type: str, event_properties: dict | None = None):
    """Fire a tracking event in a background daemon thread.

    The event is sent asynchronously so that it never blocks MCP tool
    responses — even when the analytics endpoint is slow or unreachable.
    When CS_DISABLE_TRACKING is set, no thread is spawned and no network
    request is made.
    """
    if _is_tracking_disabled():
        return

    thread = threading.Thread(
        target=_send_track_event,
        args=(event_type, event_properties),
        daemon=True,
    )
    with _pending_lock:
        _pending_threads.append(thread)
    thread.start()


def _wait_for_pending():
    """Wait for all pending tracking threads to complete.

    This is intended for use in tests to synchronise with background
    threads before making assertions.
    """
    with _pending_lock:
        threads = list(_pending_threads)
        _pending_threads.clear()
    for thread in threads:
        thread.join(timeout=TRACK_TIMEOUT)


def track(event_type: str, event_properties: dict | None = None):
    def wrapper(f):
        @functools.wraps(f)
        def wrapped(self, *f_args, **f_kwargs):
            result = f(self, *f_args, **f_kwargs)
            _send_track_event_in_background(f"mcp-{event_type}", event_properties)
            return result

        return wrapped

    return wrapper


def track_error(event_type: str, error: Exception):
    """Track an error event manually. Call this from exception handlers in tools."""
    _send_track_event_in_background(f"mcp-{event_type}-error", {"error": str(error)})

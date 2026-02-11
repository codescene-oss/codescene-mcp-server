"""
Version checker for CodeScene MCP Server.
Checks if the current version is up-to-date with the latest GitHub release.

The version check runs asynchronously in a background thread so that it
never blocks or delays MCP tool responses. On the first tool call the
check is kicked off in the background and the tool responds immediately.
Subsequent calls use the cached result once the background fetch completes.
Failed fetches (e.g. in network-restricted environments) are also cached
to avoid repeated timeout penalties.

Set CS_DISABLE_VERSION_CHECK to any non-empty value to suppress all
version-check network traffic entirely.
"""

import logging
import os
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from functools import wraps

import requests

from version import __version__

logger = logging.getLogger(__name__)

GITHUB_REPO = "codescene-oss/codescene-mcp-server"
_DEFAULT_API_URL = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
CHECK_TIMEOUT = 5  # seconds
CACHE_DURATION = 3600  # Cache version check for 1 hour


def _get_version_check_url() -> str:
    """Return the URL used for version checks.

    Reads from the CS_VERSION_CHECK_URL environment variable if set,
    otherwise uses the default GitHub API URL. This allows integration
    tests to point the check at an unreachable address without needing
    to mock network calls.
    """
    return os.environ.get("CS_VERSION_CHECK_URL", _DEFAULT_API_URL)


def _is_version_check_disabled() -> bool:
    """Return True when the user has opted out of version checks.

    Setting the CS_DISABLE_VERSION_CHECK environment variable to any
    non-empty value (e.g. "1", "true") disables all network traffic
    related to version checking.
    """
    return bool(os.environ.get("CS_DISABLE_VERSION_CHECK"))


@dataclass
class VersionInfo:
    """Version information result."""

    current: str
    latest: str | None
    outdated: bool
    message: str


class VersionChecker:
    """Checks and caches version information using non-blocking background fetches.

    The checker never blocks tool responses. When a version check is needed
    (cache miss or expiry), it spawns a background thread to fetch the latest
    version from GitHub. The result — including failures — is cached so that
    network-restricted environments only incur a single timeout per cache period.
    """

    def __init__(self, cache_duration: int = CACHE_DURATION):
        self._cache: VersionInfo | None = None
        self._last_check_time: float = 0
        self._cache_duration = cache_duration
        self._lock = threading.Lock()
        self._fetch_thread: threading.Thread | None = None

    @staticmethod
    def get_latest_version() -> str | None:
        """Fetch the latest version from GitHub releases."""
        try:
            response = requests.get(
                _get_version_check_url(),
                headers={
                    "Accept": "application/vnd.github.v3+json",
                    "User-Agent": "CodeScene-MCP-Server",
                },
                timeout=CHECK_TIMEOUT,
            )
            response.raise_for_status()
            return response.json().get("tag_name")
        except Exception:
            return None

    def _is_cache_valid(self) -> bool:
        """Check if the cached result is still fresh."""
        return self._cache is not None and (time.time() - self._last_check_time) < self._cache_duration

    def _is_fetch_in_progress(self) -> bool:
        """Check if a background fetch thread is currently running."""
        return self._fetch_thread is not None and self._fetch_thread.is_alive()

    def _store_result(self, result: VersionInfo) -> None:
        """Store a version check result in the cache (must be called under lock)."""
        self._cache = result
        self._last_check_time = time.time()

    def _build_version_info(self, latest_version: str | None) -> VersionInfo:
        """Build a VersionInfo from the current version and a fetched latest version."""
        current_version = __version__

        if latest_version is None:
            return VersionInfo(
                current=current_version,
                latest=None,
                outdated=False,
                message="",
            )

        outdated = current_version != latest_version
        message = ""

        if outdated:
            message = (
                f"⚠️  CodeScene MCP Server version {current_version} is outdated. "
                f"Latest version is {latest_version}. "
                f"Please update using your installation method:\n"
                f"  • Homebrew: `brew upgrade cs-mcp`\n"
                f"  • Windows: Re-run the PowerShell install script\n"
                f"  • Docker: `docker pull codescene/codescene-mcp:latest`\n"
                f"  • Manual: Download the latest binary from "
                f"https://github.com/codescene-oss/codescene-mcp-server/releases"
            )

        return VersionInfo(
            current=current_version,
            latest=latest_version,
            outdated=outdated,
            message=message,
        )

    def _fetch_in_background(self) -> None:
        """Run the GitHub fetch in a background thread and cache the result.

        Both successful and failed fetches are cached to prevent repeated
        timeout penalties in network-restricted environments.
        """
        try:
            latest_version = self.get_latest_version()
            result = self._build_version_info(latest_version)
        except Exception:
            result = VersionInfo(
                current=__version__,
                latest=None,
                outdated=False,
                message="",
            )

        with self._lock:
            self._store_result(result)

    def _start_background_fetch(self) -> None:
        """Spawn a daemon thread to fetch version info if one isn't already running."""
        if self._is_fetch_in_progress():
            return

        thread = threading.Thread(target=self._fetch_in_background, daemon=True)
        self._fetch_thread = thread
        thread.start()

    def _get_skip_reason(self) -> str | None:
        """Return a reason string if version checking should be skipped, else None."""
        if _is_version_check_disabled():
            return "Version check disabled"
        if __version__ == "dev":
            return "Running development version"
        return None

    def _ensure_cached_skip_result(self, message: str) -> VersionInfo:
        """Cache and return a VersionInfo for skipped checks (disabled / dev).

        Must be called under self._lock.
        """
        if not self._is_cache_valid():
            self._store_result(
                VersionInfo(
                    current=__version__,
                    latest=None,
                    outdated=False,
                    message=message,
                )
            )
        return self._cache  # type: ignore[return-value]

    def get_cached_or_trigger_fetch(self) -> VersionInfo | None:
        """Return cached version info if available, otherwise trigger a background fetch.

        This is the primary non-blocking entry point used by the decorator.

        Returns:
            VersionInfo if a cached (and still valid) result exists, None otherwise.
            When None is returned, a background fetch has been started and
            subsequent calls will return the result once it completes.
        """
        skip_reason = self._get_skip_reason()
        if skip_reason is not None:
            with self._lock:
                return self._ensure_cached_skip_result(skip_reason)

        with self._lock:
            if self._is_cache_valid():
                return self._cache

        self._start_background_fetch()
        return None

    def check_version(self) -> VersionInfo | None:
        """Synchronous version check (kept for backward compatibility).

        Prefers the cached result. If the cache is stale, triggers a
        background fetch and returns None rather than blocking.
        """
        return self.get_cached_or_trigger_fetch()


_default_checker = VersionChecker()


def check_version() -> VersionInfo | None:
    """Check if the current version is up-to-date (non-blocking)."""
    return _default_checker.check_version()


VERSION_NOTICE_DOCSTRING_SUFFIX = """
Note: If the result contains version update information (indicated by
"VERSION UPDATE AVAILABLE"), please inform the user about this update
notice and recommend they update their CodeScene MCP Server.
"""


def with_version_check(func: Callable) -> Callable:
    """Decorator that prepends version warning to tool output if outdated.

    The version check is non-blocking: if no cached result is available yet
    (e.g. first tool call), the tool response is returned immediately while
    the check runs in the background. The version warning is only prepended
    when a cached result is already available and indicates the version is
    outdated.

    Also appends a note to the function's docstring instructing the LLM
    to mention version updates to the user.
    """

    @wraps(func)
    def wrapper(*args, **kwargs):
        result = func(*args, **kwargs)
        try:
            version_info = check_version()
        except Exception:
            return result  # Fail silently - version check should never interrupt user workflow

        if version_info and version_info.outdated:
            separator = "=" * 80
            warning = f"\n{separator}\n" f"⚠️  VERSION UPDATE AVAILABLE:\n" f"{version_info.message}\n" f"{separator}\n\n"

            if isinstance(result, str):
                return warning + result
            else:
                return warning + str(result)

        return result

    # Modify the docstring to include version notice instructions
    if wrapper.__doc__:
        wrapper.__doc__ = wrapper.__doc__ + VERSION_NOTICE_DOCSTRING_SUFFIX
    else:
        wrapper.__doc__ = VERSION_NOTICE_DOCSTRING_SUFFIX.strip()

    return wrapper

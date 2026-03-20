"""Runtime environment detection for the CodeScene MCP Server.

Determines whether the server is running inside Docker, as a Nuitka-compiled
binary, or from source.  The result is cached after the first call since the
environment cannot change during a session.
"""

import os

_cached_environment: str | None = None


def _is_nuitka_environment() -> bool:
    """Return True when running as a Nuitka-compiled binary."""
    try:
        __compiled__  # type: ignore[name-defined]
        return True
    except NameError:
        return False


def get_environment() -> str:
    """Return the runtime environment: ``"docker"``, ``"nuitka"``, or ``"source"``."""
    global _cached_environment
    if _cached_environment is None:
        if os.getenv("CS_MOUNT_PATH"):
            _cached_environment = "docker"
        elif _is_nuitka_environment():
            _cached_environment = "nuitka"
        else:
            _cached_environment = "source"
    return _cached_environment

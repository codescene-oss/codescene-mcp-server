"""Clean up orphaned Nuitka onefile extraction directories.

When the MCP server is built with Nuitka ``--onefile``, each invocation
extracts the bundled runtime to a temporary directory.  On graceful exit
(SIGTERM) Nuitka's bootstrap removes the directory.  However, when the
process receives SIGKILL — as many MCP hosts do on session teardown —
the extraction directory is left behind, leaking hundreds of megabytes
per session.

This module provides a startup-time cleanup that removes stale
``onefile_*`` directories whose owning process no longer exists.  It is
safe to call even when running from source (non-Nuitka), in which case
it is a no-op.

Starting with the build that adds ``--onefile-tempdir-spec``, new
invocations use a deterministic cache path and no longer create
per-invocation ``onefile_*`` directories.  This cleanup handles the
legacy directories left by older builds.
"""

import logging
import os
import re
import shutil
import tempfile
import threading

logger = logging.getLogger(__name__)

# Nuitka onefile directories follow the pattern: onefile_{PID}_{SECONDS}_{MICROSECONDS}
# The {TIME} token expands to "{SECONDS}_{MICROSECONDS}", so there are three
# underscore-separated numeric components after the "onefile_" prefix.
_ONEFILE_DIR_PATTERN = re.compile(r"^onefile_(\d+)_\d+_\d+$")


def _pid_is_alive(pid: int) -> bool:
    """Return True if *pid* refers to a running process."""
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        # Process exists but we lack permission to signal it.
        return True
    except (OverflowError, OSError):
        # OverflowError: PID too large for the platform — cannot be valid.
        # OSError: other platform-specific errors — assume dead to allow cleanup.
        return False


def _cleanup_stale_onefile_dirs() -> None:
    """Remove ``onefile_*`` directories in the system temp dir whose PID is dead."""
    tmp = tempfile.gettempdir()
    try:
        entries = os.listdir(tmp)
    except OSError:
        return

    for name in entries:
        m = _ONEFILE_DIR_PATTERN.match(name)
        if m is None:
            continue

        pid = int(m.group(1))
        if _pid_is_alive(pid):
            continue

        path = os.path.join(tmp, name)
        try:
            shutil.rmtree(path)
            logger.debug("Removed stale onefile directory: %s", path)
        except OSError:
            # Best effort — may fail due to permissions or concurrent access.
            pass


def cleanup_stale_onefile_dirs_async() -> None:
    """Spawn a daemon thread to clean up stale onefile extraction dirs.

    The cleanup runs in the background so it never delays server startup.
    Only runs when the server is executing as a Nuitka-compiled binary.
    """
    from .environment import get_environment

    if get_environment() != "nuitka":
        return

    t = threading.Thread(target=_cleanup_stale_onefile_dirs, daemon=True)
    t.start()

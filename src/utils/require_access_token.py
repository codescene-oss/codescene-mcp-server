"""Decorator that guards MCP tool execution behind a valid access token.

Applied as the outermost decorator on every tool method (except the
Configure tools which are needed to *set* the token).  When
``CS_ACCESS_TOKEN`` is absent from the environment the decorator
short-circuits and returns a helpful error message instead of invoking
the wrapped function.

By the time any tool runs, ``apply_config_to_env()`` has already
promoted config-file values into ``os.environ``, so a single
``os.environ.get`` covers both configuration sources.
"""

import functools
import os

_TOKEN_MISSING_MESSAGE = (
    "No access token configured.\n\n"
    "To use this tool, set your access token using one of these methods:\n"
    '1. Use the `set_config` tool: set_config(key="access_token", value="your-token")\n'
    "2. Set the CS_ACCESS_TOKEN environment variable in your MCP client configuration\n\n"
    "To get an Access Token, see:\n"
    "https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/getting-a-personal-access-token.md"
)


def require_access_token(func):
    """Decorator that blocks tool execution when no CS_ACCESS_TOKEN is configured."""

    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        if not os.environ.get("CS_ACCESS_TOKEN"):
            return _TOKEN_MISSING_MESSAGE
        return func(*args, **kwargs)

    return wrapper

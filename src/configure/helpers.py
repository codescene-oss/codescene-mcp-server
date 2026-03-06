"""Formatting and filtering helpers for the Configure tools.

Pure helper functions that build JSON-serialisable representations of
configuration options, filter the visible option set, and produce
warning / error messages.  Extracted from ``configure.py`` to keep
the tool class focused on orchestration.
"""

from typing import Any

from utils import (
    CONFIG_OPTIONS,
    get_effective_value,
    is_client_env_var,
    is_standalone_token,
    mask_sensitive_value,
)
from utils.config import ConfigOption

_DOCS_BASE_URL = "https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/configuration-options.md"

# Keys that have a corresponding section in the configuration docs.
_DOCUMENTED_KEYS = frozenset(
    {
        "access_token",
        "onprem_url",
        "ace_access_token",
        "default_project_id",
        "disable_version_check",
        "ca_bundle",
    }
)


# --- Display helpers ---


def display_value(value: str | None, sensitive: bool) -> str | None:
    """Return a display-safe representation of *value*."""
    if value is None:
        return None
    if sensitive:
        return mask_sensitive_value(value)
    return value


def doc_url(key: str) -> str | None:
    """Return the full documentation URL for *key*, or ``None``."""
    if key not in _DOCUMENTED_KEYS:
        return None
    return f"{_DOCS_BASE_URL}#{key}"


def format_option_dict(key: str, option: ConfigOption, value: str | None, source: str) -> dict[str, Any]:
    """Build a JSON-serialisable dict for a single configuration option."""
    display = display_value(value, option.sensitive)
    result: dict[str, Any] = {
        "key": key,
        "env_var": option.env_var,
        "value": display,
        "source": source,
        "description": option.description,
    }
    if option.aliases:
        result["aliases"] = list(option.aliases)
    url = doc_url(key)
    if url:
        result["docs_url"] = url
    return result


def format_all_options(options: dict[str, ConfigOption]) -> list[dict[str, Any]]:
    """Build a list of dicts for every configuration option."""
    result: list[dict[str, Any]] = []
    for key, option in options.items():
        value, source = get_effective_value(key)
        result.append(format_option_dict(key, option, value, source))
    return result


# --- Filtering ---


def get_listable_options() -> dict[str, ConfigOption]:
    """Return the subset of CONFIG_OPTIONS visible in the listing.

    Hidden options (always internal) and API-only options (when running
    with a standalone license) are excluded.
    """
    standalone = is_standalone_token()
    return {
        key: opt
        for key, opt in CONFIG_OPTIONS.items()
        if not opt.hidden and not (opt.api_only and standalone)
    }


# --- Warning / error messages ---


def env_override_warning(key: str) -> str | None:
    """Return a warning string when a client-set env var overrides the file value."""
    option = CONFIG_OPTIONS[key]
    if not is_client_env_var(option.env_var):
        return None
    return (
        f"The environment variable {option.env_var} is also set "
        f"(via your MCP client config). The environment value takes "
        f"precedence over the config file at runtime."
    )


def restart_warning(key: str) -> str | None:
    """Return a warning when a changed option requires a server restart."""
    if key != "access_token":
        return None
    return (
        "Changing the access token may affect which tools are "
        "available (API vs standalone mode). A server restart is required "
        "for tool registration changes to take effect."
    )


def unknown_key_message(key: str) -> str:
    """Return a JSON error string for an unrecognized configuration key."""
    import json

    valid = sorted(CONFIG_OPTIONS.keys())
    return json.dumps({"error": f"Unknown configuration key: '{key}'", "valid_keys": valid})

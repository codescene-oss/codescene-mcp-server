"""Formatting and filtering helpers for the Configure tools.

Pure helper functions that format option rows, build multi-line
summaries, filter the visible option set, and produce warning /
error messages.  Extracted from ``configure.py`` to keep the tool
class focused on orchestration.
"""

from utils import (
    CONFIG_OPTIONS,
    get_effective_value,
    is_client_env_var,
    is_standalone_token,
    mask_sensitive_value,
)
from utils.config import ConfigOption


# --- Display helpers ---


def display_value(value: str | None, sensitive: bool) -> str:
    """Return a display-safe representation of *value*."""
    if value is None:
        return "*not set*"
    if sensitive:
        return mask_sensitive_value(value)
    return value


def format_option_row(key: str, option: ConfigOption, value: str | None, source: str) -> str:
    """Format a single option as a human-readable summary line."""
    display = display_value(value, option.sensitive)
    return f"- **{key}** ({option.env_var}): {display}  [source: {source}]\n  {option.description}"


def format_all_options(options: dict[str, ConfigOption]) -> str:
    """Build a multi-line summary of every configuration option."""
    lines: list[str] = []
    for key, option in options.items():
        value, source = get_effective_value(key)
        lines.append(format_option_row(key, option, value, source))
    return "\n\n".join(lines)


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


def env_override_warning(key: str) -> str:
    """Return a warning string when a client-set env var overrides the file value."""
    option = CONFIG_OPTIONS[key]
    if not is_client_env_var(option.env_var):
        return ""
    return (
        f"\n\nNote: The environment variable {option.env_var} is also set "
        f"(via your MCP client config). The environment value takes "
        f"precedence over the config file at runtime."
    )


def restart_warning(key: str) -> str:
    """Return a warning when a changed option requires a server restart."""
    if key != "access_token":
        return ""
    return (
        "\n\nNote: Changing the access token may affect which tools are "
        "available (API vs standalone mode). A server restart is required "
        "for tool registration changes to take effect."
    )


def unknown_key_message(key: str) -> str:
    """Return an error message for an unrecognized configuration key."""
    valid = ", ".join(sorted(CONFIG_OPTIONS.keys()))
    return f"Unknown configuration key: '{key}'. Valid keys are: {valid}"

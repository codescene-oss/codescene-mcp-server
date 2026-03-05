"""Cross-platform configuration file support for the CodeScene MCP Server.

Provides a persistent configuration layer that stores settings in a JSON
file under the OS-standard config directory.  Environment variables set
by the MCP client (e.g. VS Code or Claude Desktop) always take precedence;
the config file acts as a fallback for values not provided via the
environment.

Config directory resolution order:
1. ``CS_CONFIG_DIR`` environment variable (for testing / explicit override)
2. ``platformdirs.user_config_dir("codehealth-mcp")``
   - macOS:   ~/Library/Application Support/codehealth-mcp
   - Linux:   ~/.config/codehealth-mcp
   - Windows: %LOCALAPPDATA%/codehealth-mcp
"""

import json
import os
from dataclasses import dataclass
from pathlib import Path

from platformdirs import user_config_dir

_APP_NAME = "codehealth-mcp"
_CONFIG_FILE = "config.json"

_SENSITIVE_TAIL_LENGTH = 6

# Env vars that were already set by the MCP client *before*
# ``apply_config_to_env()`` populated the rest from the config file.
# Populated once at startup; used to distinguish "client-set" from
# "config-file-set" env vars.
_client_env_vars: set[str] = set()


@dataclass(frozen=True)
class ConfigOption:
    """Metadata for a single configuration option."""

    env_var: str
    description: str
    sensitive: bool
    hidden: bool = False
    api_only: bool = False


CONFIG_OPTIONS: dict[str, ConfigOption] = {
    "access_token": ConfigOption(
        env_var="CS_ACCESS_TOKEN",
        description="CodeScene API access token (Personal Access Token or license key).",
        sensitive=True,
    ),
    "onprem_url": ConfigOption(
        env_var="CS_ONPREM_URL",
        description="Base URL for a self-hosted CodeScene instance (e.g. https://codescene.mycompany.com).",
        sensitive=False,
        api_only=True,
    ),
    "ace_access_token": ConfigOption(
        env_var="CS_ACE_ACCESS_TOKEN",
        description="Token for the CodeScene ACE auto-refactoring API.",
        sensitive=True,
    ),
    "default_project_id": ConfigOption(
        env_var="CS_DEFAULT_PROJECT_ID",
        description="Pre-selects a CodeScene project by ID, skipping interactive selection.",
        sensitive=False,
        api_only=True,
    ),
    "disable_tracking": ConfigOption(
        env_var="CS_DISABLE_TRACKING",
        description='Set to "true" to opt out of anonymous analytics tracking.',
        sensitive=False,
        hidden=True,
    ),
    "disable_version_check": ConfigOption(
        env_var="CS_DISABLE_VERSION_CHECK",
        description='Set to "true" to suppress version-check network traffic.',
        sensitive=False,
        hidden=True,
    ),
    "ca_bundle": ConfigOption(
        env_var="REQUESTS_CA_BUNDLE",
        description="Path to a custom PEM-format CA certificate bundle for SSL verification.",
        sensitive=False,
    ),
}


# --- Directory and file helpers ---


def get_config_dir() -> Path:
    """Return the configuration directory, creating it if absent."""
    override = os.environ.get("CS_CONFIG_DIR")
    config_dir = Path(override) if override else Path(user_config_dir(_APP_NAME))
    config_dir.mkdir(parents=True, exist_ok=True)
    return config_dir


def _config_file_path() -> Path:
    return get_config_dir() / _CONFIG_FILE


# --- Read / write ---


def load_config() -> dict[str, str]:
    """Load the config file and return its contents as a dict.

    Returns an empty dict when the file does not exist or is malformed.
    """
    path = _config_file_path()
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def save_config(config: dict[str, str]) -> None:
    """Persist *config* to the config file, creating the directory if needed."""
    path = _config_file_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")


def get_config_value(key: str) -> str | None:
    """Read a single value from the config file (ignoring env vars)."""
    return load_config().get(key)


def set_config_value(key: str, value: str) -> None:
    """Write a single key/value to the config file."""
    config = load_config()
    config[key] = value
    save_config(config)


def delete_config_value(key: str) -> None:
    """Remove a key from the config file if present."""
    config = load_config()
    if key in config:
        del config[key]
        save_config(config)


# --- Environment integration ---


def apply_config_to_env() -> None:
    """Load the config file and populate ``os.environ`` for missing values.

    Uses ``os.environ.setdefault`` so that environment variables set by the
    MCP client configuration always win.  Also snapshots which config env
    vars were already present so that ``get_effective_value`` and
    ``is_client_env_var`` can distinguish client-set from file-set values.
    """
    _snapshot_client_env_vars()
    config = load_config()
    for key, value in config.items():
        option = CONFIG_OPTIONS.get(key)
        if option:
            os.environ.setdefault(option.env_var, value)


def _snapshot_client_env_vars() -> None:
    """Record which config-related env vars the MCP client already set."""
    _client_env_vars.clear()
    for option in CONFIG_OPTIONS.values():
        if option.env_var in os.environ:
            _client_env_vars.add(option.env_var)


def is_client_env_var(env_var: str) -> bool:
    """Return True if *env_var* was set by the MCP client at startup."""
    return env_var in _client_env_vars


def get_effective_value(key: str) -> tuple[str | None, str]:
    """Return the effective value and its source for *key*.

    The source is ``"environment"`` only when the MCP client set the env
    var (i.e. it was present *before* ``apply_config_to_env()``).  Values
    that ended up in ``os.environ`` via the config file are reported as
    ``"config file"``.

    Returns:
        A ``(value, source)`` tuple where *source* is one of
        ``"environment"``, ``"config file"``, or ``"not set"``.
    """
    option = CONFIG_OPTIONS.get(key)
    if option is None:
        return None, "not set"

    env_value = os.environ.get(option.env_var)
    if env_value and is_client_env_var(option.env_var):
        return env_value, "environment"

    file_value = get_config_value(key)
    if file_value:
        return file_value, "config file"

    if env_value:
        return env_value, "config file"

    return None, "not set"


# --- Display helpers ---


def mask_sensitive_value(value: str) -> str:
    """Mask a sensitive value, showing only the last few characters."""
    if len(value) <= _SENSITIVE_TAIL_LENGTH:
        return "***"
    return f"...{value[-_SENSITIVE_TAIL_LENGTH:]}"

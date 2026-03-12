"""MCP tools for reading and writing CodeScene MCP Server configuration.

Exposes two tools — ``get_config`` and ``set_config`` — that let users
manage persistent configuration through natural-language conversation
instead of editing JSON files by hand.
"""

import json
import os
from typing import TypedDict

from utils import (
    CONFIG_OPTIONS,
    delete_config_value,
    get_config_dir,
    get_effective_value,
    is_client_env_var,
    set_config_value,
    track,
    with_version_check,
)
from utils.config import ConfigOption

from .helpers import (
    doc_url,
    env_override_warning,
    format_all_options,
    format_option_dict,
    get_listable_options,
    restart_warning,
    unknown_key_message,
)


class ConfigureDeps(TypedDict):
    """Configure has no external dependencies (filesystem-only)."""


class Configure:
    """MCP tools for managing CodeScene MCP Server configuration."""

    def __init__(self, mcp_instance, deps: ConfigureDeps):
        self.deps = deps
        mcp_instance.tool(self.get_config)
        mcp_instance.tool(self.set_config)

    @with_version_check
    @track("get-config")
    def get_config(self, key: str | None = None) -> str:
        """Read current CodeScene MCP Server configuration values.

        When to use:
            Use this tool to discover available configuration keys, inspect
            effective values, and understand where each value comes from.

        Limitations:
            - Returns JSON text only; callers must format it for display.
            - Sensitive values (tokens) are masked.
            - Effective values can be overridden by client-provided env vars.

        When called without a key, lists every available configuration
        option together with its current effective value, the source of
        that value (environment variable vs. config file), and a short
        description.

        When called with a specific key, returns details for that option
        only.  Sensitive values (tokens) are masked in the output.

        Args:
            key: Optional config key to query. Omit to list all options.
        Returns:
            A JSON string. When querying a single key, the object has:
            key, env_var, value, source, description, aliases, and
            docs_url.  When listing all, the object has: config_dir and
            options (array of the same shape).  Use the aliases array
            to match user intent to the correct key.  Present the data
            clearly and always include docs_url links.

        Example:
            Call with key="access_token" to inspect one setting, or
            call without key to list all configurable options.
        """
        if key is not None:
            return self._get_single(key)
        return self._get_all()

    def _get_single(self, key: str) -> str:
        option = CONFIG_OPTIONS.get(key)
        if option is None:
            return unknown_key_message(key)

        value, source = get_effective_value(key)
        return json.dumps(format_option_dict(key, option, value, source))

    def _get_all(self) -> str:
        result = {
            "config_dir": str(get_config_dir()),
            "options": format_all_options(get_listable_options()),
        }
        return json.dumps(result)

    @with_version_check
    @track("set-config")
    def set_config(self, key: str, value: str) -> str:
        """Write a CodeScene MCP Server configuration value.

        When to use:
            Use this tool to persist or remove server configuration values
            without editing config files manually.

        Limitations:
            - Unknown keys are rejected.
            - Client-level environment variables may still override runtime
              behavior even after saving a config value.
            - Some changes may require an MCP client restart.

        Persists the value to the config file and applies it to the
        running session immediately.  To remove a value, pass an empty
        string as the value.

        If the same setting is also defined as an environment variable in
        your MCP client configuration (e.g. VS Code settings or Claude
        Desktop config), the environment variable takes precedence at
        runtime.

        Call get_config first (without a key) to discover available keys,
        their aliases, and docs_url links.

        Args:
            key: The configuration key to set.
            value: The value to store. Pass an empty string to remove the
                   key from the config file.
        Returns:
            A JSON string with status ("saved" or "removed"), key,
            config_dir, and optional warning, restart_required, and
            docs_url fields.  Present the data clearly and always
            include docs_url links.

        Example:
            Call with key="access_token" and value="..." to save,
            or pass an empty value to remove that key from config.
        """
        option = CONFIG_OPTIONS.get(key)
        if option is None:
            return unknown_key_message(key)

        if value == "":
            return self._delete_key(key, option)

        return self._set_key(key, value, option)

    def _delete_key(self, key: str, option: ConfigOption) -> str:
        delete_config_value(key)
        if not is_client_env_var(option.env_var):
            os.environ.pop(option.env_var, None)
        result: dict = {"status": "removed", "key": key}
        url = doc_url(key)
        if url:
            result["docs_url"] = url
        return json.dumps(result)

    def _set_key(self, key: str, value: str, option: ConfigOption) -> str:
        set_config_value(key, value)
        if not is_client_env_var(option.env_var):
            os.environ[option.env_var] = value
        result: dict = {
            "status": "saved",
            "key": key,
            "config_dir": str(get_config_dir()),
        }
        warning = env_override_warning(key)
        if warning:
            result["warning"] = warning
        restart = restart_warning(key)
        if restart:
            result["restart_required"] = restart
        url = doc_url(key)
        if url:
            result["docs_url"] = url
        return json.dumps(result)

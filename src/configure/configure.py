"""MCP tools for reading and writing CodeScene MCP Server configuration.

Exposes two tools — ``get_config`` and ``set_config`` — that let users
manage persistent configuration through natural-language conversation
instead of editing JSON files by hand.
"""

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
    env_override_warning,
    format_all_options,
    format_option_row,
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

        When called without a key, lists every available configuration
        option together with its current effective value, the source of
        that value (environment variable vs. config file), and a short
        description.

        When called with a specific key, returns details for that option
        only.  Sensitive values (tokens) are masked in the output.

        Available configuration keys (and common ways users refer to them):

        - "access_token" (token, API token, PAT, CodeScene access token)
          → env var CS_ACCESS_TOKEN
          CodeScene API access token or license key.

        - "onprem_url" (URL, instance URL, on-prem URL, CodeScene URL)
          → env var CS_ONPREM_URL
          Base URL for a self-hosted CodeScene instance.

        - "ace_access_token" (ACE token, refactoring token)
          → env var CS_ACE_ACCESS_TOKEN
          Token for the CodeScene ACE auto-refactoring API.

        - "default_project_id" (project ID, project)
          → env var CS_DEFAULT_PROJECT_ID
          Pre-selects a CodeScene project by ID.

        - "disable_tracking" (tracking, analytics)
          → env var CS_DISABLE_TRACKING
          Set to "true" to opt out of analytics.

        - "disable_version_check" (version check, update check)
          → env var CS_DISABLE_VERSION_CHECK
          Set to "true" to suppress version-check network traffic.

        - "ca_bundle" (SSL cert, CA certificate, certificate)
          → env var REQUESTS_CA_BUNDLE
          Path to a custom PEM CA certificate file.

        Args:
            key: Optional config key to query. Omit to list all options.
        Returns:
            A formatted string describing configuration values and their
            sources. Present this information to the user as-is.
        """
        if key is not None:
            return self._get_single(key)
        return self._get_all()

    def _get_single(self, key: str) -> str:
        option = CONFIG_OPTIONS.get(key)
        if option is None:
            return unknown_key_message(key)

        value, source = get_effective_value(key)
        return format_option_row(key, option, value, source)

    def _get_all(self) -> str:
        header = f"CodeScene MCP Server configuration (stored in {get_config_dir()}):\n\n"
        return header + format_all_options(get_listable_options())

    @with_version_check
    @track("set-config")
    def set_config(self, key: str, value: str) -> str:
        """Write a CodeScene MCP Server configuration value.

        Persists the value to the config file and applies it to the
        running session immediately.  To remove a value, pass an empty
        string as the value.

        If the same setting is also defined as an environment variable in
        your MCP client configuration (e.g. VS Code settings or Claude
        Desktop config), the environment variable takes precedence at
        runtime.

        Available configuration keys (and common ways users refer to them):

        - "access_token" (token, API token, PAT, CodeScene access token)
          → env var CS_ACCESS_TOKEN
          CodeScene API access token or license key.

        - "onprem_url" (URL, instance URL, on-prem URL, CodeScene URL)
          → env var CS_ONPREM_URL
          Base URL for a self-hosted CodeScene instance.

        - "ace_access_token" (ACE token, refactoring token)
          → env var CS_ACE_ACCESS_TOKEN
          Token for the CodeScene ACE auto-refactoring API.

        - "default_project_id" (project ID, project)
          → env var CS_DEFAULT_PROJECT_ID
          Pre-selects a CodeScene project by ID.

        - "disable_tracking" (tracking, analytics)
          → env var CS_DISABLE_TRACKING
          Set to "true" to opt out of analytics.

        - "disable_version_check" (version check, update check)
          → env var CS_DISABLE_VERSION_CHECK
          Set to "true" to suppress version-check network traffic.

        - "ca_bundle" (SSL cert, CA certificate, certificate)
          → env var REQUESTS_CA_BUNDLE
          Path to a custom PEM CA certificate file.

        Args:
            key: The configuration key to set (see list above).
            value: The value to store. Pass an empty string to remove the
                   key from the config file.
        Returns:
            A confirmation message describing what was changed.
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
        return f"Removed '{key}' from config file."

    def _set_key(self, key: str, value: str, option: ConfigOption) -> str:
        set_config_value(key, value)
        if not is_client_env_var(option.env_var):
            os.environ[option.env_var] = value
        confirmation = f"Saved '{key}' to config file at {get_config_dir()}."
        return confirmation + env_override_warning(key) + restart_warning(key)

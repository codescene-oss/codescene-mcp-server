---
name: configuring-codescene-mcp
description: Use when the user wants to view, set, or troubleshoot CodeScene MCP configuration such as access tokens, on-prem URLs, ACE tokens, default projects, or SSL certificates.
---

# Configuring CodeScene MCP

## Overview

Use this skill when the task is to configure the CodeScene MCP Server after it has been installed. The MCP server exposes `get_config` and `set_config` tools that let the AI assistant read and write configuration on the user's behalf. This is the easiest and recommended configuration method.

## When to Use

- The user wants to set or change their CodeScene access token.
- The user needs to connect to a self-hosted CodeScene instance.
- The user wants to enable ACE auto-refactoring.
- The user wants to pre-select a default CodeScene project.
- The user needs to configure a custom CA certificate for SSL/TLS.
- The user asks what their current configuration is.
- The user is troubleshooting a configuration issue (wrong token, missing URL, SSL errors).

Do not use this skill for installing or registering the MCP server in an AI assistant. Use `installing-and-activating-codescene-mcp` for that.

## Quick Reference

- `get_config`: List all configuration options and their current values (sensitive values are masked).
- `get_config` with a key: Read a single option by name.
- `set_config`: Set a configuration value persistently.
- `set_config` with an empty value: Delete a stored configuration value.

### Configuration Options

| Key | Purpose |
|-----|---------|
| `access_token` | CodeScene Personal Access Token or standalone MCP license token. |
| `onprem_url` | Base URL for a self-hosted CodeScene instance (API-mode only). |
| `ace_access_token` | Token for CodeScene ACE auto-refactoring (add-on license). |
| `default_project_id` | Pre-select a CodeScene project by numeric ID (API-mode only). |
| `ca_bundle` | Path to a custom PEM-format CA certificate bundle. |

### Precedence

Environment variables set by the MCP client always override values in the config file. If the user has set a value via an environment variable in their editor config, `set_config` will warn that the env var takes precedence and the stored value will not be used until the env var is removed.

## Implementation

1. Run `get_config` to see the current state of all options.
2. Identify which option needs to change based on the user's request.
3. Use `set_config` with the key and value to apply the change.
4. Run `get_config` with that key to confirm the change took effect.
5. If the user changed `access_token`, inform them that a server restart may be needed for tool registration changes to take effect.
6. If `get_config` shows a value source of "client environment variable", explain that the env var in their editor's MCP configuration takes precedence and must be changed there instead.

### When environment variables are appropriate

Environment variables are still the right choice when:

- The configuration is shared across a team or checked into a project (e.g., in `.vscode/mcp.json`).
- The server runs in Docker and needs `CS_MOUNT_PATH` (which is not a config-tool option).
- CI or automation pipelines inject secrets at runtime.

For individual, interactive use, prefer the `set_config` tool.

## Common Mistakes

- Setting a value with `set_config` when the same key is already provided as an environment variable by the MCP client. The env var wins and the stored value is silently ignored.
- Forgetting that `access_token` changes may require a server restart.
- Confusing the config key name with the environment variable name. Use the short key (e.g., `access_token`) with `set_config`, not the env var name (`CS_ACCESS_TOKEN`).
- Setting `onprem_url` or `default_project_id` when using a standalone license. These options are only available with a CodeScene Personal Access Token.
- Providing a CA bundle path that is not accessible to the MCP server process or Docker container.

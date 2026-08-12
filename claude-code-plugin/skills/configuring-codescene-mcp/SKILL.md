---
name: configuring-codescene-mcp
description: Use when the user wants to view, set, or troubleshoot CodeScene MCP configuration such as OAuth login, access tokens, account IDs, on-prem URLs, default projects, or SSL certificates.
---

# Configuring CodeScene MCP

## Overview

Use this skill when the task is to configure the CodeScene MCP Server after it has been installed. The MCP server exposes `get_config`, `set_config`, and `logout` tools (and, outside Docker, `login`) that let the AI assistant authenticate and manage configuration on the user's behalf. Clients that support MCP prompts also expose `login` and `logout` prompts (slash commands) — except in Docker, where OAuth login is unavailable and a Personal Access Token (`CS_ACCESS_TOKEN`) is required.

## When to Use

- The user wants to sign in to CodeScene (OAuth).
- The user wants to sign out of CodeScene (clear the OAuth session).
- The user wants to set or change their CodeScene access token (PAT / standalone).
- The user belongs to multiple Cloud accounts and needs to pin an account ID.
- The user needs to connect to a self-hosted CodeScene instance.
- The user wants to pre-select a default CodeScene project.
- The user needs to configure a custom CA certificate for SSL/TLS.
- The user wants to limit which tools are exposed to reduce token usage (`enabled_tools`).
- The user asks what their current configuration is.
- The user is troubleshooting a configuration issue (wrong token, missing URL, SSL errors).

Do not use this skill for installing or registering the MCP server in an AI assistant. Use `installing-and-activating-codescene-mcp` for that.

## Quick Reference

- `login`: Sign in with OAuth (opens browser). Preferred for interactive desktop use. Also available as the `login` MCP prompt (slash command). Not available in Docker — use a PAT instead.
- `logout`: Sign out of OAuth (clears CLI credentials and MCP OAuth config). Does not remove `CS_ACCESS_TOKEN`. Also available as the `logout` MCP prompt.
- `get_config`: List all configuration options and their current values (sensitive values are masked).
- `get_config` with a key: Read a single option by name.
- `set_config`: Set a configuration value persistently.
- `set_config` with an empty value: Delete a stored configuration value.

### Authentication precedence

1. `CS_ACCESS_TOKEN` / `access_token` (PAT or standalone) — always wins and **blocks** OAuth `login`.
2. Cached OAuth session from a previous `login`.
3. CLI credential refresh.

### Configuration Options

| Key | Purpose |
|-----|---------|
| `account_id` | Optional Cloud account/tenant ID (positive integer). Pin multi-account OAuth to a specific account. Set **before** `login` and keep set afterward. |
| `access_token` | Optional Personal Access Token or standalone MCP license token (CI/headless). |
| `onprem_url` | Base URL for a self-hosted CodeScene instance (API-mode only). |
| `default_project_id` | Pre-select a CodeScene project by numeric ID (API-mode only). |
| `ca_bundle` | Path to a custom PEM-format CA certificate bundle. |
| `enabled_tools` | Comma-separated allowlist of tool names to expose (empty = all). |

### Precedence

Environment variables set by the MCP client always override values in the config file. If the user has set a value via an environment variable in their editor config, `set_config` will warn that the env var takes precedence and the stored value will not be used until the env var is removed.

## Implementation

1. Run `get_config` to see the current state of all options.
2. For interactive auth: if `access_token` is set and the user wants OAuth, clear it first (`set_config` with empty value or ask them to remove `CS_ACCESS_TOKEN` from client env). For multi-account Cloud, ensure `account_id` is set before calling `login`.
3. Call `login` for OAuth (or have the user invoke the `login` MCP prompt), or `set_config` for PAT / other options. To sign out, call `logout` (or the `logout` prompt).
4. Run `get_config` with the relevant key to confirm the change took effect.
5. If the user changed `access_token`, inform them that a server restart may be needed for tool registration changes to take effect.
6. If `get_config` shows a value source of "client environment variable", explain that the env var in their editor's MCP configuration takes precedence and must be changed there instead.

### When environment variables are appropriate

Environment variables are still the right choice when:

- The configuration is shared across a team or checked into a project (e.g., in `.vscode/mcp.json`).
- The server runs in Docker and needs `CS_MOUNT_PATH` (which is not a config-tool option).
- CI or automation pipelines inject secrets at runtime.

For individual, interactive use, prefer `login` for auth and `set_config` for other options.

## Common Mistakes

- Setting a value with `set_config` when the same key is already provided as an environment variable by the MCP client. The env var wins and the stored value is silently ignored.
- Leaving `access_token` / `CS_ACCESS_TOKEN` set and then trying to use `login` — PAT always blocks OAuth.
- Setting `account_id` after `login` without logging in again — the credential slot is chosen at login time and must match later CLI calls.
- Forgetting that `access_token` changes may require a server restart.
- Confusing the config key name with the environment variable name. Use the short key (e.g., `access_token`, `account_id`) with `set_config`, not the env var name (`CS_ACCESS_TOKEN`, `CS_ACCOUNT_ID`).
- Setting `onprem_url` or `default_project_id` when using a standalone license. These options are only available with CodeScene Core (OAuth or PAT).
- Providing a CA bundle path that is not accessible to the MCP server process or Docker container.
- Setting `enabled_tools` with misspelled tool names. The server warns about unknown names, but the misspelled tools are silently ignored. Use `get_config` with key `enabled_tools` to see the list of available tool names.
- Forgetting that `enabled_tools` changes require a server restart. The tool list is built once at startup.
- Trying to disable `get_config`, `set_config`, `login`, or `logout` via `enabled_tools`. Outside Docker these tools are always enabled to prevent configuration lockout. In Docker, `login` is not registered at all (use PAT / `CS_ACCESS_TOKEN`); `logout` remains available.

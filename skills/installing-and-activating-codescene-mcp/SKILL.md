---
name: installing-and-activating-codescene-mcp
description: Use when installing the CodeScene MCP Server binary or package, registering it in an AI assistant, or copying agent guidance files into a repository.
---

# Installing and Activating CodeScene MCP

## Overview

Use this skill when the task is to get the CodeScene MCP Server installed and registered with an AI assistant. The workflow has three parts: install the server, register it in the assistant, and copy the agent guidance files into the repository.

For configuring the server after installation (access tokens, on-prem URLs, ACE, SSL), use the `configuring-codescene-mcp` skill instead.

## When to Use

- The user wants to install CodeScene MCP for the first time.
- The user has the binary or package installed but has not registered it in an AI assistant.
- The user wants a quick setup path for VS Code, GitHub Copilot, Claude Code, Docker, Homebrew, or Windows.

Do not use this skill for configuring tokens, URLs, or other server options. Use `configuring-codescene-mcp` for that. Do not use this skill for refactoring, safeguards, or technical debt prioritization.

## Quick Reference

- Install using the method that matches the environment: `npx` or `npm`, Homebrew, Windows installer script, manual download, or Docker.
- Register the server in the AI assistant so it launches `cs-mcp`.
- Copy `AGENTS.md` into the repository, or `.amazonq/rules` for Amazon Q.
- Copy any relevant skills from the CodeScene MCP skills catalogue at `https://github.com/codescene-oss/codescene-mcp-server/tree/main/skills`.
- After installation, use `set_config` or ask the assistant to configure the access token and any other options.

## Implementation

1. Choose the installation method that fits the user environment:
   - `npx @codescene/codehealth-mcp` or `npm install -g @codescene/codehealth-mcp`
   - `brew tap codescene-oss/codescene-mcp-server https://github.com/codescene-oss/codescene-mcp-server && brew install cs-mcp`
   - Windows PowerShell installer
   - Manual binary download
   - Docker image
2. Verify that the `cs-mcp` command or container entrypoint is available.
3. Register the server in the AI assistant:
   - For VS Code or GitHub Copilot, add a `codescene` server entry in `settings.json` or `.vscode/mcp.json`.
   - For Claude Code, add the MCP with `claude mcp add`.
4. Configure the server by asking the assistant to set the access token (this uses the `set_config` tool under the hood). See the `configuring-codescene-mcp` skill for full details.
5. Copy `AGENTS.md` to the repository so the agent knows how to use the MCP safely.
6. If the user is on Amazon Q, copy `.amazonq/rules` instead of `AGENTS.md`.
7. Copy any relevant skills from `https://github.com/codescene-oss/codescene-mcp-server/tree/main/skills` for safeguarding, refactoring, prioritization, or business-case workflows.

## Common Mistakes

- Installing the server but never registering it with the assistant.
- Copying the setup files into the wrong repository.
- Skipping the configuration step after installation. The server needs at least an access token to function.
- Manually editing environment variables when `set_config` is simpler and persistent.

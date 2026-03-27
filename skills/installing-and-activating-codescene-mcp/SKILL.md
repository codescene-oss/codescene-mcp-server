---
name: installing-and-activating-codescene-mcp
description: Use when setting up the CodeScene MCP Server, choosing an installation method, configuring CS_ACCESS_TOKEN, or activating the server in an AI assistant.
---

# Installing and Activating CodeScene MCP

## Overview

Use this skill when the task is to get the CodeScene MCP Server running and connected to an AI assistant. The workflow has four parts: get a token, install the server, configure the assistant, and copy the agent guidance files into the repository.

## When to Use

- The user wants to install CodeScene MCP for the first time.
- The user has the binary or package installed but has not activated it in an AI assistant.
- The user needs to configure `CS_ACCESS_TOKEN` or `CS_ONPREM_URL`.
- The user wants a quick setup path for VS Code, GitHub Copilot, Claude Code, Docker, Homebrew, or Windows.

Do not use this skill for refactoring, safeguards, or technical debt prioritization.

## Quick Reference

- Get `CS_ACCESS_TOKEN` first.
- For CodeScene Cloud, create the token at `https://codescene.io/users/me/pat`.
- For CodeScene on-prem, use `https://<your-cs-host>:<port>/configuration/user/token`.
- Install using the method that matches the environment: `npx` or `npm`, Homebrew, Windows installer script, manual download, or Docker.
- Configure the AI assistant to launch `cs-mcp` and pass `CS_ACCESS_TOKEN`.
- Add `CS_ONPREM_URL` when using CodeScene on-prem.
- Copy `AGENTS.md` into the repository, or `.amazonq/rules` for Amazon Q.
- Copy any relevant skills from the CodeScene MCP skills catalogue at `https://github.com/codescene-oss/codescene-mcp-server/tree/main/skills`.

## Implementation

1. Confirm whether the user is on CodeScene Cloud or CodeScene on-prem.
2. Get or configure `CS_ACCESS_TOKEN`.
3. Choose the installation method that fits the user environment:
   - `npx @codescene/codehealth-mcp` or `npm install -g @codescene/codehealth-mcp`
   - `brew install cs-mcp`
   - Windows PowerShell installer
   - Manual binary download
   - Docker image
4. Verify that the `cs-mcp` command or container entrypoint is available.
5. Configure the AI assistant to launch the server and pass the required environment variables.
6. For VS Code or GitHub Copilot, add a `codescene` server entry in `settings.json` or `.vscode/mcp.json`.
7. For Claude Code, add the MCP with `claude mcp add` and pass the required environment variables.
8. Copy `AGENTS.md` to the repository so the agent knows how to use the MCP safely.
9. If the user is on Amazon Q, copy `.amazonq/rules` instead of `AGENTS.md`.
10. Copy any relevant skills from `https://github.com/codescene-oss/codescene-mcp-server/tree/main/skills` for safeguarding, refactoring, prioritization, or business-case workflows.

## Common Mistakes

- Installing the server before obtaining `CS_ACCESS_TOKEN`.
- Forgetting to pass `CS_ACCESS_TOKEN` to the AI assistant process.
- Forgetting `CS_ONPREM_URL` for on-prem installations.
- Installing the server but never registering it with the assistant.
- Copying the setup files into the wrong repository.
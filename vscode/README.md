# CodeScene CodeHealth MCP — VS Code Extension

AI-powered code health analysis directly in VS Code's agent mode (Copilot Chat). This extension bundles the CodeScene MCP Server and automatically configures it, giving you instant access to code quality tools without any manual setup.

## Features

- **Zero-config MCP setup** — Installs and registers the CodeScene MCP Server automatically
- **Platform-specific binaries** — The correct native binary for your OS/arch is bundled inside the extension
- **Settings UI** — Configure your CodeScene credentials and options through VS Code's settings
- **Agent mode tools** — All CodeScene CodeHealth MCP tools are available in Copilot's agent mode

## Available Tools (in Agent Mode)

Once installed, the following tools become available in VS Code's agent mode.

### Code Health Analysis (All Users)

These tools work with any valid access token — standalone or CodeScene Core.

| Tool | Description |
|------|-------------|
| `code_health_score` | Calculate the Code Health score (1.0–10.0) for a file |
| `code_health_review` | Detailed review with code smells and refactoring guidance |
| `pre_commit_code_health_safeguard` | Check staged/modified files before committing |
| `analyze_change_set` | PR pre-flight: compare branch vs base for regressions |
| `code_health_refactoring_business_case` | Quantified business case for refactoring a file |
| `explain_code_health` | Explains the Code Health metric and how scores are interpreted |
| `explain_code_health_productivity` | Business case data for Code Health improvements |

### Technical Debt & Project Insights (CodeScene Core users — cloud or on-prem)

These tools require a CodeScene Personal Access Token and a CodeScene Core instance.

| Tool | Description |
|------|-------------|
| `select_project` | List and select CodeScene projects |
| `list_technical_debt_hotspots_for_project` | Find high-impact technical debt hotspots across a project |
| `list_technical_debt_hotspots_for_project_file` | Hotspot metrics for a specific file |
| `list_technical_debt_goals_for_project` | View active refactoring goals for a project |
| `list_technical_debt_goals_for_project_file` | Goals for a specific file |
| `code_ownership_for_path` | Identify code owners for a file or directory |

### Server Management (All Users)

| Tool | Description |
|------|-------------|
| `get_config` | Read current server configuration |
| `set_config` | Write a configuration value |
| `verify_installation` | Diagnose setup issues |
| `list_skills` | List available embedded skills |
| `get_skill_manifest` | Inspect a skill's file manifest |
| `download_skill` | Download a skill to a local directory |
| `sync_skills` | Download all available skills |

## Getting Started

1. **Install the extension** from the VS Code Marketplace
2. **Set your access token** via `CodeScene: Configure Access Token` command (or in Settings)
3. **Open agent mode** in Copilot Chat and start using CodeScene tools

### Access Token

The type of access token determines which tools are available:

- **CodeScene Personal Access Token** — Enables the full tool set, including project-level features such as technical debt hotspots, goals, and code ownership lookups.
- **Standalone access token** — Enables local Code Health analysis tools only (scoring, review, refactoring). Project-level features are not available.

Get a Personal Access Token from your [CodeScene instance](https://codescene.io) under API settings.

## Settings

| Setting | Description |
|---------|-------------|
| `codescene.enabled` | Enable/disable the MCP server |
| `codescene.accessToken` | Your CodeScene access token (standalone or PAT) |
| `codescene.onpremUrl` | Base URL of your self-hosted CodeScene instance |
| `codescene.defaultProjectId` | Pre-select a project by numeric ID |
| `codescene.enabledTools` | Comma-separated list of tools to expose (empty = all) |
| `codescene.disableVersionCheck` | Suppress automatic version-check on startup |
| `codescene.caBundlePath` | Path to a PEM CA certificate bundle for SSL/TLS |

See [Configuration Options](https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/configuration-options.md) for full documentation.

## Commands

- `CodeScene: Configure Access Token` — Securely enter your access token
- `CodeScene: Restart MCP Server` — Restart the MCP server (after config changes)
- `CodeScene: Show Server Status` — Display current server status and configuration

## Supported Platforms

| Platform | Architecture |
|----------|-------------|
| macOS | Apple Silicon (ARM64) |
| macOS | Intel (x64) |
| Linux | x64 |
| Linux | ARM64 |
| Windows | x64 |

## License

MIT

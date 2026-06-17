# CodeScene Code Health — VS Code Extension

AI-powered code health analysis directly in VS Code's agent mode (Copilot Chat). This extension bundles the CodeScene MCP Server and automatically configures it, giving you instant access to code quality tools without any manual setup.

## Features

- **Zero-config MCP setup** — Installs and registers the CodeScene MCP Server automatically
- **Platform-specific binaries** — The correct native binary for your OS/arch is bundled inside the extension
- **Settings UI for access token** — Configure your CodeScene credentials through VS Code's settings
- **Agent mode tools** — All CodeScene Code Health tools are available in Copilot's agent mode

## Available Tools (in Agent Mode)

Once installed, the following tools become available in VS Code's agent mode:

| Tool | Description |
|------|-------------|
| `code_health_score` | Get the Code Health score (1.0–10.0) for any file |
| `code_health_review` | Detailed review with specific code smells and refactoring guidance |
| `pre_commit_code_health_safeguard` | Check staged/modified files before committing |
| `analyze_change_set` | PR pre-flight: compare branch vs base for regressions |
| `code_health_refactoring_business_case` | Quantified business case for refactoring a file |
| `list_technical_debt_hotspots` | Find high-impact technical debt across a project |
| `list_technical_debt_goals` | View active refactoring goals |
| `code_ownership_for_path` | Identify code owners for review |

## Getting Started

1. **Install the extension** from the VS Code Marketplace
2. **Set your access token** via `CodeScene: Configure Access Token` command (or in Settings)
3. **Open agent mode** in Copilot Chat and start using CodeScene tools

### Access Token

Some tools (project-level analysis, hotspots, ownership) require a CodeScene access token. Local file analysis works without one.

Get your token from your [CodeScene account](https://codescene.io) under API settings.

## Settings

| Setting | Description |
|---------|-------------|
| `codescene.enabled` | Enable/disable the MCP server |
| `codescene.accessToken` | Your CodeScene API access token |
| `codescene.serverUrl` | Custom server URL (for on-premise installations) |
| `codescene.defaultProjectId` | Default project ID for project-scoped tools |

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

## Development

```bash
cd vscode/
npm install
npm run compile

# Download binary for your current platform
npm run download-binary -- current

# Package for current platform
npm run package:platform -- current

# Package for all platforms
npm run package:platform
```

## Building Platform-Specific VSIXs

For CI/CD, build a VSIX per platform:

```bash
npm run package:platform -- darwin-arm64
npm run package:platform -- darwin-x64
npm run package:platform -- linux-x64
npm run package:platform -- linux-arm64
npm run package:platform -- win32-x64
```

Each produces a `.vsix` file that bundles only the binary for that target platform.

## License

MIT

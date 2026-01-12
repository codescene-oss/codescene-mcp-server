# Installing CodeScene MCP Server on Windows

You can install the CodeScene MCP Server on Windows using a simple PowerShell command.

## Prerequisites

- Windows 10 or later
- PowerShell 5.1 or later
- A CodeScene account with an access token (see [Getting a Personal Access Token](getting-a-personal-access-token.md))

## Quick Installation

Run this in PowerShell:

```powershell
irm https://raw.githubusercontent.com/codescene-oss/codescene-mcp-server/main/install.ps1 | iex
```

This downloads the latest version to `%LOCALAPPDATA%\Programs\cs-mcp` and adds it to your PATH.

After installation, restart your terminal and verify:

```powershell
cs-mcp --version
```

## Updating

Run the same installation command to update to the latest version:

```powershell
irm https://raw.githubusercontent.com/codescene-oss/codescene-mcp-server/main/install.ps1 | iex
```

## Uninstalling

```powershell
irm https://raw.githubusercontent.com/codescene-oss/codescene-mcp-server/main/uninstall.ps1 | iex
```

## Integration with AI Assistants

After installing, configure your AI assistant to use the `cs-mcp` binary directly (no Docker required).

### VS Code / GitHub Copilot

Add to your VS Code `settings.json` or `.vscode/mcp.json`:

```json
{
  "mcp": {
    "servers": {
      "codescene": {
        "command": "cs-mcp",
        "env": {
          "CS_ACCESS_TOKEN": "<your token here>"
        }
      }
    }
  }
}
```

For CodeScene On-prem, add `"CS_ONPREM_URL": "<your onprem url>"` to the `env` section.

### Claude Desktop

Add to your Claude Desktop configuration (`%APPDATA%\Claude\claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "<your token here>"
      }
    }
  }
}
```

### Codex CLI

Configure `~/.codex/config.toml`:

```toml
[mcp_servers.codescene]
command = "cs-mcp"
env = { "CS_ACCESS_TOKEN" = "<YOUR_ACCESS_TOKEN>" }
```

### Kiro

Create a `.kiro/settings/mcp.json` file:

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "<YOUR_TOKEN>"
      },
      "disabled": false
    }
  }
}
```

### Amazon Q CLI

```powershell
q mcp add --name codescene-mcp --command cs-mcp
```

Make sure `CS_ACCESS_TOKEN` is set in your environment.

## Enabling CodeScene ACE

To enable [CodeScene ACE](https://codescene.com/product/integrations/ide-extensions/ai-refactoring) refactoring, add the `CS_ACE_ACCESS_TOKEN` environment variable to your configuration:

```json
{
  "mcp": {
    "servers": {
      "codescene": {
        "command": "cs-mcp",
        "env": {
          "CS_ACCESS_TOKEN": "<your token>",
          "CS_ACE_ACCESS_TOKEN": "<your ACE token>"
        }
      }
    }
  }
}
```

## Troubleshooting

### Binary not in PATH

If `cs-mcp` is not recognized, ensure the install directory is in your PATH:

```powershell
$env:Path += ";$env:LOCALAPPDATA\Programs\cs-mcp"
```

To make this permanent, run the PATH modification from the installation script above.

### Manual Download

You can also download the executable directly from the [releases page](https://github.com/codescene-oss/codescene-mcp-server/releases/latest) and place it in a directory in your PATH.

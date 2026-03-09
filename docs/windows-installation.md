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

After installation, restart your terminal and verify it runs:

```powershell
cs-mcp
```

> **Note:** For PATH changes to take effect, you may need to restart your terminal, IDE, or other applications. Some applications (like VS Code or Claude Desktop) may require a full restart to pick up the new PATH.

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

> **Tip:** Once connected, you can configure your access token and other settings by simply asking your AI assistant — for example, *"Set my CodeScene access token to cs_abc123"*. See [Configuration Options](configuration-options.md) for all available settings.

### VS Code / GitHub Copilot

Add to your VS Code `settings.json` or `.vscode/mcp.json`:

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp"
    }
  }
}
```

### Cursor

Add to your project-level `.cursor/mcp.json` file, or `~/.cursor/mcp.json` for global configuration:

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp"
    }
  }
}
```

> **Note:** You can also add MCP servers via Cursor's UI: Settings > Cursor Settings > MCP > Add new global MCP server. See the [Cursor MCP documentation](https://docs.cursor.com/context/model-context-protocol) for more details.

### Claude Desktop

Add to your Claude Desktop configuration (`%APPDATA%\Claude\claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp"
    }
  }
}
```

> **Note:** After saving the configuration, restart Claude Desktop.

### Codex CLI

Configure `~/.codex/config.toml`:

```toml
[mcp_servers.codescene]
command = "cs-mcp"
```

### Kiro

Create a `.kiro/settings/mcp.json` file:

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp",
      "disabled": false
    }
  }
}
```

### Amazon Q CLI

```powershell
q mcp add --name codescene-mcp --command cs-mcp
```

## Configuration

For additional configuration — including CodeScene on-prem, ACE auto-refactoring, custom SSL/TLS certificates, and more — see [Configuration Options](configuration-options.md).

## Troubleshooting

### Binary not in PATH

If `cs-mcp` is not recognized, ensure the install directory is in your PATH:

```powershell
$env:Path += ";$env:LOCALAPPDATA\Programs\cs-mcp"
```

To make this permanent, run the PATH modification from the installation script above.

### Manual Download

You can also download the executable directly from the [releases page](https://github.com/codescene-oss/codescene-mcp-server/releases/latest) and place it in a directory in your PATH.

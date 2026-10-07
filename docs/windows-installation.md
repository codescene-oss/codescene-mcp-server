# Installing CodeScene MCP Server on Windows

You can install the CodeScene MCP Server on Windows using a simple PowerShell command.

For VS Code / GitHub Copilot, the [CodeScene MCP extension](https://marketplace.visualstudio.com/items?itemName=codescene.codescene-codehealth-mcp) bundles the server and configures it automatically, without a separate PowerShell installation.

## Prerequisites

- Windows 10 or later
- PowerShell 5.1 or later
- A CodeScene account with an active license (see [Authentication](authentication.md))

## Quick Installation

Run this in PowerShell:

```powershell
irm https://raw.githubusercontent.com/codescene-oss/codescene-mcp-server/main/install.ps1 | iex
```

This downloads the latest version to `%LOCALAPPDATA%\Programs\cs-mcp` and adds it to your PATH.

After installation, fully quit and reopen your AI assistant or IDE, including all VS Code windows if you use GitHub Copilot. These applications inherit PATH at startup and do not automatically pick up the installer's changes. Opening a new integrated terminal or restarting only the MCP server is not sufficient to refresh VS Code's PATH.

Open a new PowerShell window and verify it runs:

```powershell
cs-mcp
```

Then configure your AI assistant using the instructions below. Running the executable by itself does not connect it to GitHub Copilot or another assistant.

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

> **Tip:** Once connected, ask your AI Assistant to log in, or use the login tool. you can configure your optional access token and other settings by simply asking your AI assistant — for example, *"Set my CodeScene access token to cs_abc123"*. See [Configuration Options](configuration-options.md) for all available settings.

### VS Code / GitHub Copilot

The simplest option is to install the [CodeScene MCP extension](https://marketplace.visualstudio.com/items?itemName=codescene.codescene-codehealth-mcp), which bundles and automatically configures the server.

If you installed with PowerShell, add the following to `.vscode/mcp.json`, or use **MCP: Open User Configuration** in the Command Palette for user-level configuration:

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "${env:LOCALAPPDATA}\\Programs\\cs-mcp\\cs-mcp.exe"
    }
  }
}
```

This uses the installer's executable path directly, so VS Code does not need an updated PATH to find it. If you use `"command": "cs-mcp"` instead, fully quit and reopen VS Code after installation. Use **MCP: List Servers** in the Command Palette to select `codescene` and start it.

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

For additional configuration — including CodeScene on-prem, custom SSL/TLS certificates, and more — see [Configuration Options](configuration-options.md).

## Troubleshooting

### Installed, but GitHub Copilot cannot start the server

If you installed with PowerShell while VS Code was already running, fully quit all VS Code windows and reopen VS Code. Restarting only the MCP server, reloading a window, or changing PATH in an integrated terminal does not refresh the environment inherited by VS Code.

Alternatively, use the explicit executable path in the [VS Code configuration above](#vs-code--github-copilot), or install the CodeScene MCP extension. A Windows reboot should not normally be necessary.

### Binary not in PATH

If `cs-mcp` is not recognized, first open a new PowerShell window. To add the install directory to PATH for the current PowerShell session:

```powershell
$env:Path += ";$env:LOCALAPPDATA\Programs\cs-mcp"
```

This changes only the current shell's environment, not an already-running IDE or AI assistant. To make the change permanent, rerun the installation script above, then fully quit and reopen your terminal and AI assistant.

### Manual Download

You can also download the executable directly from the [releases page](https://github.com/codescene-oss/codescene-mcp-server/releases/latest) and place it in a directory in your PATH.

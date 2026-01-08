# Installing CodeScene MCP Server via Winget (Windows)

You can install the CodeScene MCP Server using Windows Package Manager (winget) on Windows 10/11.

## Prerequisites

- Windows 10 1709 (build 16299) or later
- [Windows Package Manager](https://docs.microsoft.com/en-us/windows/package-manager/winget/) (comes pre-installed on Windows 11)
- A CodeScene account with an access token ([get one here](https://codescene.io/users/me/pat) for Cloud, or via your on-prem instance)

## Quick Installation

```powershell
winget install CodeScene.CsMcp
```

## Usage

After installation, the `cs-mcp` command will be available:

```powershell
cs-mcp
```

## Updating

To update to the latest version:

```powershell
winget upgrade CodeScene.CsMcp
```

## Uninstalling

```powershell
winget uninstall CodeScene.CsMcp
```

## Integration with AI Assistants

After installing via winget, configure your AI assistant to use the `cs-mcp` binary directly (no Docker required).

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

### Package not found

If winget cannot find the package, make sure your winget sources are up to date:

```powershell
winget source update
```

### Binary not in PATH

The portable executable should be added to your PATH automatically. If not, you can find it at:

```
%LOCALAPPDATA%\Microsoft\WinGet\Packages\CodeScene.CsMcp_*\
```

Add this directory to your system PATH environment variable.

## Manual Installation

If you prefer not to use winget, you can download the executable directly from the [releases page](https://github.com/codescene-oss/codescene-mcp-server/releases/latest) and place it in a directory in your PATH.

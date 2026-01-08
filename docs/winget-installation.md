# Installing CodeScene MCP Server via Winget (Windows)

You can install the CodeScene MCP Server using Windows Package Manager (winget) on Windows 10/11.

## Prerequisites

- Windows 10 1709 (build 16299) or later
- [Windows Package Manager](https://docs.microsoft.com/en-us/windows/package-manager/winget/) (comes pre-installed on Windows 11)

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

## Using with AI Assistants

After installing via winget, you can configure your AI assistant to use the binary directly.

### Example: VS Code / GitHub Copilot

In your VS Code settings (`settings.json`):

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

### Example: Claude Desktop

In your Claude Desktop configuration (`%APPDATA%\Claude\claude_desktop_config.json`):

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

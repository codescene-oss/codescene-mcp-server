# Installing CodeScene MCP Server via Homebrew

You can install the CodeScene MCP Server using Homebrew on macOS and Linux.

## Prerequisites

- [Homebrew](https://brew.sh/) installed
- A CodeScene account with an access token (see [Getting a Personal Access Token](getting-a-personal-access-token.md))

## Quick Installation

```bash
# Add the CodeScene tap
brew tap codescene-oss/codescene-mcp-server https://github.com/codescene-oss/codescene-mcp-server

# Install CodeScene MCP Server
brew install cs-mcp
```

## Usage

After installation, the `cs-mcp` command will be available in your PATH:

```bash
cs-mcp
```

## Updating

To update to the latest version:

```bash
brew update
brew upgrade cs-mcp
```

## Uninstalling

```bash
brew uninstall cs-mcp
brew untap codescene-oss/codescene-mcp-server
```

## Supported Platforms

| Platform       | Architecture |
|----------------|--------------|
| macOS          | ARM64 (Apple Silicon) |
| macOS          | AMD64 (Intel) |
| Linux          | ARM64 |
| Linux          | AMD64 |

## Integration with AI Assistants

After installing via Homebrew, configure your AI assistant to use the `cs-mcp` binary directly (no Docker required).

### Claude Code

Set your token and add the MCP server:

```bash
export CS_ACCESS_TOKEN="your-token-here"
claude mcp add codescene --env CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN -- cs-mcp
```

For CodeScene On-prem:

```bash
export CS_ACCESS_TOKEN="your-token-here"
export CS_ONPREM_URL="https://your-codescene-instance.example.com"
claude mcp add codescene --env CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN --env CS_ONPREM_URL=$CS_ONPREM_URL -- cs-mcp
```

### VS Code / GitHub Copilot

Add to your VS Code `settings.json` or `.vscode/mcp.json`:

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
    }
  }
}
```

For CodeScene On-prem, add `"CS_ONPREM_URL": "https://your-codescene-instance.example.com"` to the `env` section.

### Codex CLI

Configure `~/.codex/config.toml`:

```toml
[mcp_servers.codescene]
command = "cs-mcp"
env = { "CS_ACCESS_TOKEN" = "your-token-here" }
```

### Kiro

Create a `.kiro/settings/mcp.json` file:

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      },
      "disabled": false
    }
  }
}
```

### Claude Desktop

Claude Desktop is available for macOS and Windows. Add to your configuration file:
- **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

**CodeScene Cloud:**

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
    }
  }
}
```

**CodeScene On-prem:**

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://your-codescene-instance.example.com"
      }
    }
  }
}
```

> **Note:** After saving the configuration, restart Claude Desktop.

### Amazon Q CLI

```bash
q mcp add --name codescene-mcp --command cs-mcp
```

Make sure `CS_ACCESS_TOKEN` is set in your environment.

## Enabling CodeScene ACE

To enable [CodeScene ACE](https://codescene.com/product/integrations/ide-extensions/ai-refactoring) refactoring, add the `CS_ACE_ACCESS_TOKEN` environment variable to your configuration:

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ACE_ACCESS_TOKEN": "your-ace-token-here"
      }
    }
  }
}
```

## Troubleshooting

### Binary not found after installation

Make sure Homebrew's bin directory is in your PATH:

```bash
# For macOS (Apple Silicon)
export PATH="/opt/homebrew/bin:$PATH"

# For macOS (Intel) or Linux
export PATH="/usr/local/bin:$PATH"
```

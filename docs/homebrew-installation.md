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

> **Note:** If the command is not found, ensure Homebrew's bin directory is in your PATH—see the [Troubleshooting](#binary-not-found-after-installation) section below. Some applications (like VS Code or Claude Desktop) may require a restart to pick up PATH changes.

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

> **Tip:** Once connected, you can configure your access token and other settings by simply asking your AI assistant — for example, *"Set my CodeScene access token to cs_abc123"*. See [Configuration Options](configuration-options.md) for all available settings.

### Claude Code

```bash
claude mcp add codescene -- cs-mcp
```

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

### Claude Desktop

Claude Desktop is available for macOS and Windows. Add to your configuration file:
- **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

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

### Amazon Q CLI

```bash
q mcp add --name codescene-mcp --command cs-mcp
```

## Configuration

For additional configuration — including CodeScene on-prem, ACE auto-refactoring, custom SSL/TLS certificates, and more — see [Configuration Options](configuration-options.md).

## Troubleshooting

### Binary not found after installation

Make sure Homebrew's bin directory is in your PATH:

```bash
# For macOS (Apple Silicon)
export PATH="/opt/homebrew/bin:$PATH"

# For macOS (Intel)
export PATH="/usr/local/bin:$PATH"

# For Linux (default Homebrew location)
export PATH="/home/linuxbrew/.linuxbrew/bin:$PATH"
```

To make this permanent, add the appropriate line to your shell configuration file (`~/.zshrc`, `~/.bashrc`, or similar). After updating, restart your terminal or run `source ~/.zshrc` (or your shell's config file).

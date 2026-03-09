# Installing CodeScene MCP Server via NPM

You can install and run the CodeScene MCP Server using `npx` on macOS, Linux, and Windows. This is the quickest way to get started — no manual downloads or package managers required.

## Prerequisites

- [Node.js](https://nodejs.org/) 18 or later
- An Access Token (see [Getting a Personal Access Token](getting-a-personal-access-token.md))

## Quick Start

Run the MCP server directly with npx (no install needed):

```bash
npx @codescene/codehealth-mcp
```

The first run automatically downloads the correct platform-specific binary for your system and caches it for future use.

## Supported Platforms

| Platform       | Architecture |
|----------------|--------------|
| macOS          | ARM64 (Apple Silicon) |
| macOS          | AMD64 (Intel) |
| Linux          | ARM64 |
| Linux          | AMD64 |
| Windows        | AMD64 |

## Integration with AI Assistants

After installing, configure your AI assistant to use `npx @codescene/codehealth-mcp` as the command.

### Claude Code

Set your token and add the MCP server:

```bash
export CS_ACCESS_TOKEN="your-token-here"
claude mcp add codescene --env CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN -- npx @codescene/codehealth-mcp
```

### VS Code / GitHub Copilot

Add to your VS Code `settings.json` or `.vscode/mcp.json`:

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
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
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
    }
  }
}
```

> **Note:** You can also add MCP servers via Cursor's UI: Settings > Cursor Settings > MCP > Add new global MCP server. See the [Cursor MCP documentation](https://docs.cursor.com/context/model-context-protocol) for more details.

### Codex CLI

Configure `~/.codex/config.toml`:

```toml
[mcp_servers.codescene]
command = "npx"
args = ["@codescene/codehealth-mcp"]
env = { "CS_ACCESS_TOKEN" = "your-token-here" }
```

### Kiro

Create a `.kiro/settings/mcp.json` file:

```json
{
  "mcpServers": {
    "codescene": {
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
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

```json
{
  "mcpServers": {
    "codescene": {
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here"
      }
    }
  }
}
```

> **Note:** After saving the configuration, restart Claude Desktop.

### Amazon Q CLI

```bash
q mcp add --name codescene-mcp --command npx --args '["@codescene/codehealth-mcp"]'
```

Make sure `CS_ACCESS_TOKEN` is set in your environment.

## Configuration

For additional configuration — including CodeScene on-prem, ACE auto-refactoring, custom SSL/TLS certificates, and more — see [Configuration Options](configuration-options.md).

### Custom Binary Path

If you have the `cs-mcp` binary installed elsewhere, you can skip the automatic download:

```bash
CS_MCP_BINARY_PATH=/path/to/cs-mcp npx @codescene/codehealth-mcp
```

### Custom Download URL

For air-gapped or mirrored environments:

```bash
CS_MCP_DOWNLOAD_BASE_URL=https://your-mirror.example.com npx @codescene/codehealth-mcp
```

## Troubleshooting

### npx not found

Ensure Node.js 18+ is installed and `npx` is available in your PATH:

```bash
node --version   # Should be v18 or later
npx --version
```

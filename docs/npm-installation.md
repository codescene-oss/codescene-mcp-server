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

For CodeScene On-prem:

```bash
export CS_ACCESS_TOKEN="your-token-here"
export CS_ONPREM_URL="https://your-codescene-instance.example.com"
claude mcp add codescene --env CS_ACCESS_TOKEN=$CS_ACCESS_TOKEN --env CS_ONPREM_URL=$CS_ONPREM_URL -- npx @codescene/codehealth-mcp
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

For CodeScene On-prem, add `"CS_ONPREM_URL": "https://your-codescene-instance.example.com"` to the `env` section.

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

For CodeScene On-prem, add `"CS_ONPREM_URL": "https://your-codescene-instance.example.com"` to the `env` section.

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

**CodeScene Cloud:**

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

**CodeScene On-prem:**

```json
{
  "mcpServers": {
    "codescene": {
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
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
q mcp add --name codescene-mcp --command npx --args '["@codescene/codehealth-mcp"]'
```

Make sure `CS_ACCESS_TOKEN` is set in your environment.

## Enabling CodeScene ACE

To enable [CodeScene ACE](https://codescene.com/product/integrations/ide-extensions/ai-refactoring) refactoring, add the `CS_ACE_ACCESS_TOKEN` environment variable to your configuration:

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ACE_ACCESS_TOKEN": "your-ace-token-here"
      }
    }
  }
}
```

## Custom SSL/TLS Certificates

If your organization uses a corporate proxy or internal CA certificates for your on-premise CodeScene instance, you need to configure the MCP server to trust that certificate.

### Configuration

Set the `REQUESTS_CA_BUNDLE` environment variable to point to your CA certificate file (PEM format):

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://your-codescene-instance.example.com",
        "REQUESTS_CA_BUNDLE": "/path/to/your/ca-bundle.crt"
      }
    }
  }
}
```

Or set it as a shell environment variable before running your AI assistant:

```bash
export REQUESTS_CA_BUNDLE=/path/to/your/internal-ca.crt
export CS_ACCESS_TOKEN="your-token-here"
export CS_ONPREM_URL="https://your-codescene-instance.example.com"
npx @codescene/codehealth-mcp
```

### Supported Environment Variables

The following environment variables are checked in order of precedence:

| Variable | Description |
|----------|-------------|
| `REQUESTS_CA_BUNDLE` | Standard Python/requests CA bundle path (recommended) |
| `SSL_CERT_FILE` | Alternative CA certificate path |
| `CURL_CA_BUNDLE` | curl-style CA bundle path |

### How It Works

The MCP server automatically handles SSL configuration for both its Python components and the embedded Java-based CodeScene CLI:

1. **Python/requests**: Uses the certificate directly via `REQUESTS_CA_BUNDLE`
2. **Java CLI**: The MCP server automatically converts the PEM certificate to a PKCS12 truststore at runtime and injects the appropriate Java SSL arguments

This means you only need to configure SSL once—the MCP server handles the rest.

### Notes

- The certificate file must be in PEM format (the standard format with `-----BEGIN CERTIFICATE-----` headers)
- The path must be accessible to the MCP server process
- If your certificate chain includes intermediate certificates, include them all in the same file

## Advanced Configuration

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

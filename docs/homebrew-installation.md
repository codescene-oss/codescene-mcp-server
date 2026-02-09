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

### Cursor

Add to your project-level `.cursor/mcp.json` file, or `~/.cursor/mcp.json` for global configuration:

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

For CodeScene On-prem, add `"CS_ONPREM_URL": "https://your-codescene-instance.example.com"` to the `env` section.

> **Note:** You can also add MCP servers via Cursor's UI: Settings > Cursor Settings > MCP > Add new global MCP server. See the [Cursor MCP documentation](https://docs.cursor.com/context/model-context-protocol) for more details.

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

## Custom SSL/TLS Certificates

If your organization uses a corporate proxy or internal CA certificates for your on-premise CodeScene instance, you need to configure the MCP server to trust that certificate.

### Configuration

Set the `REQUESTS_CA_BUNDLE` environment variable to point to your CA certificate file (PEM format):

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
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
cs-mcp
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

### Example: Claude Desktop with On-Prem SSL

```json
{
  "mcpServers": {
    "codescene": {
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://codescene.internal.company.com",
        "REQUESTS_CA_BUNDLE": "/etc/ssl/certs/company-ca.crt"
      }
    }
  }
}
```

### Notes

- The certificate file must be in PEM format (the standard format with `-----BEGIN CERTIFICATE-----` headers)
- The path must be accessible to the MCP server process
- If your certificate chain includes intermediate certificates, include them all in the same file

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

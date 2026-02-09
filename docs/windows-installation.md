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

### Claude Desktop

Add to your Claude Desktop configuration (`%APPDATA%\Claude\claude_desktop_config.json`):

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

### Amazon Q CLI

```powershell
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
        "REQUESTS_CA_BUNDLE": "C:\\certs\\company-ca.crt"
      }
    }
  }
}
```

Or set it as a PowerShell environment variable:

```powershell
$env:REQUESTS_CA_BUNDLE = "C:\certs\company-ca.crt"
$env:CS_ACCESS_TOKEN = "your-token-here"
$env:CS_ONPREM_URL = "https://your-codescene-instance.example.com"
cs-mcp
```

### Supported Environment Variables

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
- Use full Windows paths (e.g., `C:\certs\ca.crt`)
- If your certificate chain includes intermediate certificates, include them all in the same file

## Troubleshooting

### Binary not in PATH

If `cs-mcp` is not recognized, ensure the install directory is in your PATH:

```powershell
$env:Path += ";$env:LOCALAPPDATA\Programs\cs-mcp"
```

To make this permanent, run the PATH modification from the installation script above.

### Manual Download

You can also download the executable directly from the [releases page](https://github.com/codescene-oss/codescene-mcp-server/releases/latest) and place it in a directory in your PATH.

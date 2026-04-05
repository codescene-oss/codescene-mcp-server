# Configuration Options

The CodeScene MCP Server can be configured in three ways, listed from easiest to most involved:

### 1. Ask your AI assistant (easiest)

The simplest way to configure the MCP server is to ask your AI assistant directly. The MCP server has built-in `set_config` and `get_config` tools that the AI assistant can call on your behalf. Just tell it what you want in plain language:

> "Set my CodeScene access token to cs_abc123def456"

> "Connect to our on-prem CodeScene at https://codescene.mycompany.com"

> "Enable CodeScene ACE with token ace_xyz789"

> "Disable the CodeScene version update check"

The AI will use the `set_config` tool to save the value persistently. You can verify any setting by asking:

> "What is my current CodeScene configuration?"

### 2. Environment variables in your MCP client config

Set environment variables in your editor's MCP configuration file. This is the standard approach when you want settings checked into a project or shared across a team. Examples are shown for each option below.

### 3. Config file (managed automatically)

The `set_config` tool (option 1 above) persists values to a JSON config file at:
- **macOS:** `~/Library/Application Support/codehealth-mcp/config.json`
- **Linux:** `~/.config/codehealth-mcp/config.json`
- **Windows:** `%LOCALAPPDATA%/codehealth-mcp/config.json`

Environment variables set by your MCP client always take precedence over values stored in the config file.

---

## `access_token`

| | |
|---|---|
| **Environment variable** | `CS_ACCESS_TOKEN` |
| **Sensitive** | Yes (value is masked in tool output) |
| **Required** | Yes, for most functionality |

The primary authentication credential for the CodeScene MCP Server. This can be either a **Personal Access Token (PAT)** obtained from your CodeScene instance, or a **standalone access token** (if you purchased MCP separately).

The type of token determines which tools are available:

- **CodeScene Personal Access Token** -- Enables the full tool set, including project-level features such as technical debt hotspots, goals, and code ownership lookups.
- **Standalone access token** -- Enables local Code Health analysis tools only (scoring, review, refactoring). Project-level and API-dependent features are not available.

Changing this value may require a **server restart** for tool registration changes to take effect.

See [Getting a Personal Access Token](getting-a-personal-access-token.md) for instructions on creating a PAT.

## `onprem_url`

| | |
|---|---|
| **Environment variable** | `CS_ONPREM_URL` |
| **Sensitive** | No |
| **API-only** | Yes (hidden when using a standalone license) |

The base URL of your self-hosted CodeScene instance. Only required when connecting to a CodeScene on-prem installation rather than CodeScene Cloud.

Provide the root URL without a trailing slash, for example:

```
https://codescene.mycompany.com
```

When this option is not set, the MCP server connects to CodeScene Cloud by default.

**Example — npx:**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://codescene.mycompany.com"
      }
    }
  }
}
```

**Example — Static binary (Homebrew / Windows):**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://codescene.mycompany.com"
      }
    }
  }
}
```

**Example — Docker:**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CS_ACCESS_TOKEN",
        "-e", "CS_ONPREM_URL",
        "-e", "CS_MOUNT_PATH=/path/to/your/code",
        "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://codescene.mycompany.com"
      }
    }
  }
}
```

## `ace_access_token`

| | |
|---|---|
| **Environment variable** | `CS_ACE_ACCESS_TOKEN` |
| **Sensitive** | Yes (value is masked in tool output) |

Token for the [CodeScene ACE](https://codescene.com/product/integrations/ide-extensions/ai-refactoring) auto-refactoring API. When set, the `code_health_auto_refactor` tool becomes available, enabling automated refactoring of functions with code health issues.

ACE is a **CodeScene add-on** and requires an additional license. You can [request access and more info here](https://codescene.com/contact-us-about-codescene-ace).

ACE supports the following languages:
- JavaScript / TypeScript
- Java
- C#
- C++

And the following code smells:
- Complex Conditional
- Bumpy Road Ahead
- Complex Method
- Deep, Nested Complexity
- Large Method

**Example — npx:**

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

**Example — Static binary (Homebrew / Windows):**

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

**Example — Docker:**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CS_ACCESS_TOKEN",
        "-e", "CS_ACE_ACCESS_TOKEN",
        "-e", "CS_MOUNT_PATH=/path/to/your/code",
        "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ACE_ACCESS_TOKEN": "your-ace-token-here"
      }
    }
  }
}
```

## `default_project_id`

| | |
|---|---|
| **Environment variable** | `CS_DEFAULT_PROJECT_ID` |
| **Sensitive** | No |
| **API-only** | Yes (hidden when using a standalone license) |

Pre-selects a CodeScene project by its numeric ID, skipping the interactive project selection step. This is useful when you always work within a single project and want to avoid being prompted each time. This option is for CodeScene users only; standalone MCP users do not have projects.

You can find your project ID by using the `select_project` tool, which lists all available projects with their IDs.

## `disable_version_check`

| | |
|---|---|
| **Environment variable** | `CS_DISABLE_VERSION_CHECK` |
| **Sensitive** | No |
| **Hidden** | Yes (not shown in default listings) |

Set to `"true"` to suppress the automatic version-check network request that the server makes on startup. By default, the server checks for newer versions and includes an update notice in tool responses when one is available.

You may want to disable this in air-gapped environments or if the extra network call is undesirable.

**Example — npx:**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_DISABLE_VERSION_CHECK": "1"
      }
    }
  }
}
```

**Example — Static binary (Homebrew / Windows):**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_DISABLE_VERSION_CHECK": "1"
      }
    }
  }
}
```

## `ca_bundle`

| | |
|---|---|
| **Environment variable** | `REQUESTS_CA_BUNDLE` |
| **Sensitive** | No |

Path to a custom PEM-format CA certificate bundle for SSL/TLS verification. Required when your organization uses a corporate proxy or internal certificate authority for your on-premise CodeScene instance.

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

This means you only need to configure SSL once — the MCP server handles the rest.

### Example — npx

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
        "REQUESTS_CA_BUNDLE": "/path/to/your/internal-ca.crt"
      }
    }
  }
}
```

### Example — Static Binary (Homebrew / Windows)

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://your-codescene-instance.example.com",
        "REQUESTS_CA_BUNDLE": "/path/to/your/internal-ca.crt"
      }
    }
  }
}
```

### Example — Docker

When using Docker, mount your certificate into the container and set `REQUESTS_CA_BUNDLE` to the path *inside* the container:

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CS_ACCESS_TOKEN",
        "-e", "CS_ONPREM_URL",
        "-e", "REQUESTS_CA_BUNDLE=/certs/internal-ca.crt",
        "-e", "CS_MOUNT_PATH=/path/to/your/code",
        "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "--mount", "type=bind,src=/path/to/your/certs/internal-ca.crt,dst=/certs/internal-ca.crt,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ONPREM_URL": "https://your-codescene-instance.example.com"
      }
    }
  }
}
```

> **Note:** The `REQUESTS_CA_BUNDLE` value (`/certs/internal-ca.crt`) must match the destination path in the mount (`dst=/certs/internal-ca.crt`).

If you have multiple certificates or a certificate bundle directory, mount the entire directory instead:

```json
"--mount", "type=bind,src=/etc/ssl/company-certs,dst=/certs,ro"
```

### Notes

- The certificate file must be in PEM format (the standard format with `-----BEGIN CERTIFICATE-----` headers)
- The path must be accessible to the MCP server process (or mounted into the container for Docker)
- If your certificate chain includes intermediate certificates, include them all in the same file

## `enabled_tools`

| | |
|---|---|
| **Environment variable** | `CS_ENABLED_TOOLS` |
| **Sensitive** | No |

Controls which MCP tools the server exposes to the AI assistant. When set, only the listed tools are registered — all others are hidden. When unset or empty, all tools are enabled (the default behavior).

This is useful for reducing token usage by limiting the number of tool descriptions sent to the AI model. Each exposed tool adds to the prompt context, so disabling tools you don't need can lower costs and improve response times.

The `get_config` and `set_config` tools are always enabled and cannot be disabled. This prevents accidental lockout from the configuration system.

Changes to this setting require a server restart to take effect.

### Available tool names

**Standalone tools** — work without a CodeScene API connection:

| Tool name | Description |
|-----------|-------------|
| `explain_code_health` | Explains the Code Health metric |
| `explain_code_health_productivity` | Explains Code Health productivity impact |
| `code_health_review` | Detailed Code Health review of a file |
| `code_health_score` | Quick numeric Code Health score for a file |
| `pre_commit_code_health_safeguard` | Pre-commit check for Code Health regressions |
| `analyze_change_set` | Branch-level Code Health analysis (PR pre-flight) |
| `code_health_refactoring_business_case` | Quantified business case for refactoring |
| `code_health_auto_refactor` | AI-assisted auto-refactoring (requires ACE) |

**API tools** — require CodeScene Cloud or CodeScene On-prem:

| Tool name | Description |
|-----------|-------------|
| `select_project` | List and select CodeScene projects |
| `list_technical_debt_goals_for_project` | Technical debt goals for a project |
| `list_technical_debt_goals_for_project_file` | Technical debt goals for a specific file |
| `list_technical_debt_hotspots_for_project` | Technical debt hotspots for a project |
| `list_technical_debt_hotspots_for_project_file` | Technical debt hotspots for a specific file |
| `code_ownership_for_path` | Code ownership lookup for a file or directory |

In standalone mode (no `access_token`), the API tools are automatically removed regardless of the `enabled_tools` setting.

### Examples

To enable only the local Code Health analysis tools (no project-level features):

**Example — npx:**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "npx",
      "args": ["@codescene/codehealth-mcp"],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ENABLED_TOOLS": "code_health_review,code_health_score,pre_commit_code_health_safeguard,analyze_change_set"
      }
    }
  }
}
```

**Example — Static binary (Homebrew / Windows):**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "cs-mcp",
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ENABLED_TOOLS": "code_health_review,code_health_score,pre_commit_code_health_safeguard,analyze_change_set"
      }
    }
  }
}
```

**Example — Docker:**

```json
{
  "servers": {
    "codescene": {
      "type": "stdio",
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-e", "CS_ACCESS_TOKEN",
        "-e", "CS_ENABLED_TOOLS",
        "-e", "CS_MOUNT_PATH=/path/to/your/code",
        "--mount", "type=bind,src=/path/to/your/code,dst=/mount/,ro",
        "codescene/codescene-mcp"
      ],
      "env": {
        "CS_ACCESS_TOKEN": "your-token-here",
        "CS_ENABLED_TOOLS": "code_health_review,code_health_score,pre_commit_code_health_safeguard,analyze_change_set"
      }
    }
  }
}
```

You can also set this interactively via the AI assistant:

> "Only enable code_health_review, code_health_score, and analyze_change_set"

Or use `set_config` directly:

> "Set enabled_tools to code_health_review,code_health_score,analyze_change_set"

To re-enable all tools, set the value to an empty string:

> "Clear the enabled_tools setting"

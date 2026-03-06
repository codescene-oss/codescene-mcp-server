# Configuration Options

The CodeScene MCP Server can be configured through environment variables set in your MCP client configuration, or through a persistent JSON config file managed by the built-in `get_config` and `set_config` tools.

**Config file location** (OS-dependent):
- **macOS:** `~/Library/Application Support/codehealth-mcp/config.json`
- **Linux:** `~/.config/codehealth-mcp/config.json`
- **Windows:** `%LOCALAPPDATA%/codehealth-mcp/config.json`

Environment variables set by your MCP client always take precedence over values stored in the config file. You can check the effective value and its source for any option by using the `get_config` tool.

---

## `access_token`

| | |
|---|---|
| **Environment variable** | `CS_ACCESS_TOKEN` |
| **Sensitive** | Yes (value is masked in tool output) |
| **Required** | Yes, for most functionality |

The primary authentication credential for the CodeScene MCP Server. This can be either a **Personal Access Token (PAT)** obtained from your CodeScene instance, or a **standalone license key**.

The type of token determines which tools are available:

- **Personal Access Token** -- Enables the full tool set, including project-level features such as technical debt hotspots, goals, and code ownership lookups.
- **Standalone license key** -- Enables local Code Health analysis tools only (scoring, review, refactoring). Project-level and API-dependent features are not available.

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

## `ace_access_token`

| | |
|---|---|
| **Environment variable** | `CS_ACE_ACCESS_TOKEN` |
| **Sensitive** | Yes (value is masked in tool output) |

Token for the [CodeScene ACE](https://codescene.com/product/integrations/ide-extensions/ai-refactoring) auto-refactoring API. When set, the `code_health_auto_refactor` tool becomes available, enabling automated refactoring of functions with code health issues.

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

## `default_project_id`

| | |
|---|---|
| **Environment variable** | `CS_DEFAULT_PROJECT_ID` |
| **Sensitive** | No |
| **API-only** | Yes (hidden when using a standalone license) |

Pre-selects a CodeScene project by its numeric ID, skipping the interactive project selection step. This is useful when you always work within a single project and want to avoid being prompted each time.

You can find your project ID by using the `select_project` tool, which lists all available projects with their IDs.

## `disable_version_check`

| | |
|---|---|
| **Environment variable** | `CS_DISABLE_VERSION_CHECK` |
| **Sensitive** | No |
| **Hidden** | Yes (not shown in default listings) |

Set to `"true"` to suppress the automatic version-check network request that the server makes on startup. By default, the server checks for newer versions and includes an update notice in tool responses when one is available.

You may want to disable this in air-gapped environments or if the extra network call is undesirable.

## `ca_bundle`

| | |
|---|---|
| **Environment variable** | `REQUESTS_CA_BUNDLE` |
| **Sensitive** | No |

Path to a custom PEM-format CA certificate bundle for SSL/TLS verification. Required when your organization uses a corporate proxy or internal certificate authority for your on-premise CodeScene instance.

The MCP server automatically handles SSL configuration for both its Python components and the embedded Java-based CodeScene CLI -- you only need to configure this once.

Example:

```
/etc/ssl/certs/company-ca.crt
```

If your certificate chain includes intermediate certificates, include them all in the same PEM file. See the [Homebrew Installation guide](homebrew-installation.md#custom-ssltls-certificates) for detailed setup instructions.

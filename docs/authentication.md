# Authentication

The CodeScene MCP Server supports two authentication methods.

## Recommended: OAuth Login

For interactive desktop use, no token needs to be manually obtained or configured. Start the OAuth flow in any of these ways:

1. **MCP `login` prompt (slash command)** — In clients that support MCP prompts (for example VS Code Copilot Chat), invoke the `login` prompt from the chat slash-command / prompt picker. This inserts a short instruction that makes the assistant call the `login` tool.
2. **Ask the assistant** — Say *"Log me in to CodeScene"* so the agent calls the `login` tool.
3. **VS Code command** — Run **CodeScene: Sign In** from the Command Palette (VS Code extension only).

The `login` tool opens your browser to complete the OAuth flow. Once done, the MCP server is authenticated for the session.

**For CodeScene Cloud:** no extra configuration needed for single-account users — just sign in.

**Multi-account Cloud:** If you belong to more than one CodeScene Cloud account, set your account/tenant ID **before** logging in, and keep it set afterward (it selects the OAuth credential slot):

> "Set my CodeScene account ID to 123"

Then sign in with the `login` prompt, by asking the assistant, or via **CodeScene: Sign In**.

The value must be a positive integer (`CS_ACCOUNT_ID` / `account_id`). It does not apply to PAT auth or on-prem.

**For CodeScene on-prem:** configure your instance URL first, then log in:

> "Set my CodeScene on-prem URL to https://codescene.mycompany.com"

Then sign in with the `login` prompt, by asking the assistant, or via **CodeScene: Sign In**.

> **Note:** OAuth is not supported in Docker installation

---

## Alternative: Personal Access Token (PAT)

Use a PAT when OAuth is not suitable — for example in CI/CD pipelines, headless environments, or when you prefer a static credential.

Set the token by asking your AI assistant:

> "Set my CodeScene access token to &lt;your-token&gt;"

Or set `CS_ACCESS_TOKEN` directly in your MCP client configuration. `CS_ACCESS_TOKEN` always takes precedence over a stored OAuth session when set.

### Standalone MCP Token

If you want a standalone MCP token (without connecting through a CodeScene Cloud or on-prem instance), sign up here:

👉 **[CodeScene MCP Server](https://codescene.com/product/mcp-server)**

### CodeScene Cloud PAT

If you're using CodeScene Cloud, create your token here:

👉 **[Create a Personal Access Token](https://codescene.io/users/me/pat)**

### CodeScene On-Prem PAT

If you're using CodeScene on-prem, follow these steps to create a Personal Access Token:

1. **Log in to your CodeScene instance**  
   Contact your CodeScene admin if you do not know the URL.

2. **Navigate to the Configuration menu**  
   Click on the Configuration menu in the top navigation.

3. **Go to the Authentication tab**  
   Select the Authentication tab from the configuration options.

4. **Create a new Personal Access Token**  
   Click **Personal Access Tokens** under the Authentication & User Management section to create a new token.

Alternatively, navigate directly to:

```
https://<your-cs-host><:port>/configuration/user/token
```

---

## Further Configuration

See [Configuration Options](configuration-options.md) for all available settings.

> ⚠️ **Keep your token secure!** Treat it like a password and never commit it to version control.

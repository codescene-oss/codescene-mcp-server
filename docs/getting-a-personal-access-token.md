# Authentication

The CodeScene MCP Server supports two authentication methods.

## Recommended: OAuth Login

For interactive desktop use, no token needs to be manually obtained or configured. Simply ask your AI assistant:

> "Log me in to CodeScene"

The assistant will call the `login` tool, which opens your browser to complete the OAuth flow. Once done, the MCP server is authenticated for the session.

**For CodeScene Cloud:** no extra configuration needed — just call `login`.

**For CodeScene on-prem:** configure your instance URL first, then log in:

> "Set my CodeScene on-prem URL to https://codescene.mycompany.com"

> "Log me in to CodeScene"

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

# Authentication

The CodeScene MCP Server supports two authentication methods.

## Recommended: OAuth Login

For interactive desktop use, no token needs to be manually obtained or configured. Start the OAuth flow in any of these ways:

1. **MCP `login` prompt (slash command)** — In clients that support MCP prompts (for example VS Code Copilot Chat), invoke the `login` prompt from the chat slash-command / prompt picker. This inserts a short instruction that makes the assistant call the `login` tool.
2. **Ask the assistant** — Say *"Log me in to CodeScene"* so the agent calls the `login` tool.
3. **VS Code command** — Run **CodeScene: Sign In** from the Command Palette (VS Code extension only).

The `login` tool opens your browser to complete the OAuth flow. Once done, the MCP server is authenticated for the session.

**For CodeScene Cloud:** no extra configuration needed for single-account users — just sign in.

**For CodeScene on-prem:** configure your instance URL first, then log in:

> "Set my CodeScene on-prem URL to https://codescene.mycompany.com"

Then sign in with the `login` prompt, by asking the assistant, or via **CodeScene: Sign In**.

> **Note:** OAuth is not supported in Docker installation.

### Switch Cloud account

Use this when you belong to **more than one** CodeScene Cloud account and need the MCP server to use a different tenant. Cloud OAuth tokens are account-bound, so changing the active account is an explicit switch — not a config tweak.

Start a switch in any of these ways:

1. **MCP `switch_account` prompt (slash command)** — Instructs the assistant to call the `switch_account` tool (provide the numeric account ID in the prompt context when you know it).
2. **Ask the assistant** — for example:

   > "Switch my CodeScene account to 123"

3. **VS Code command** — Run **CodeScene: Switch Account** from the Command Palette, enter the account ID, and confirm.

The account ID must be a **positive integer** (`account_id` / `CS_ACCOUNT_ID`). You can find it in the CodeScene Cloud UI for the account you want to use.

> **Note:** You can find the Account ID to enter [here](https://codescene.io/users/me) under **Set Current Account**

#### What `switch_account` does

1. Pins `account_id` so later token refresh uses that Cloud account’s CLI credential slot.
2. Clears the MCP server’s cached OAuth access token (so it does not keep serving the previous account).
3. Tries to reuse a **stored** OAuth session for that account on this machine.
4. Opens a browser for interactive login only if that account has never been signed in here (or its stored credentials are missing).

Switching does **not** log you out of other accounts’ stored CLI sessions. After you have signed in to account A and account B once, switching between them is usually a slot reuse with no browser step.

#### Common mistakes

- **`set_config(account_id=…)` alone while signed in** — saves the pin but does **not** retarget the active OAuth session. Always use `switch_account` (or **CodeScene: Switch Account**) to change accounts.
- **Calling `login` after changing `account_id`** — `login` short-circuits with “already signed in” if any fresh OAuth token exists, so it will not switch tenants.
- **Expecting a PAT or on-prem session to switch** — `switch_account` applies to **Cloud OAuth only**. Personal Access Tokens (`CS_ACCESS_TOKEN`) and on-prem OAuth are not multi-account switches; clear or reconfigure those separately.

#### First login vs switch

- **First login (optional pin):** You may set `account_id` **before** `login` if you want the initial browser flow pinned to a specific account. Keep it set afterward so refresh uses the matching credential slot.
- **Already signed in:** Prefer `switch_account` with the target account ID. You do not need to logout first.

#### Legacy sessions (no `account_id` pin)

If you signed in earlier without setting `account_id`, the session still works via the browser-selected Cloud account. The server may backfill session account metadata from the CLI when available. Use `switch_account` when you intentionally want to pin or change accounts. The first time you pin an account that previously only existed in the browser-selected slot, you may be asked to sign in once so credentials are stored for that numbered slot.

### Sign out

To clear a stored OAuth session:

1. **MCP `logout` prompt (slash command)** — Invokes the `logout` tool.
2. **Ask the assistant** — Say *"Log me out of CodeScene"* so the agent calls the `logout` tool.
3. **VS Code command** — Run **CodeScene: Sign Out** from the Command Palette (VS Code extension only).

Logout revokes/removes the CLI OAuth credentials and clears MCP OAuth config. It does **not** remove a Personal Access Token (`CS_ACCESS_TOKEN`); clear that separately via `set_config` or your MCP client settings.

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

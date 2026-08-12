# CodeScene CodeHealth MCP — VS Code Extension

AI-powered code health analysis directly in VS Code's agent mode (Copilot Chat). This extension bundles the CodeScene MCP Server and automatically configures it, giving you instant access to code quality tools without any manual setup.

## Features

- **Zero-config MCP setup** — Installs and registers the CodeScene MCP Server automatically
- **Platform-specific binaries** — The correct native binary for your OS/arch is bundled inside the extension
- **OAuth-first auth** — Sign in via the `login` slash command, Sign In command, or by asking the agent; no token paste required for interactive use
- **Settings UI** — Optional PAT, Cloud account ID, on-prem URL, and other options
- **Agent mode tools** — All CodeScene CodeHealth MCP tools are available in Copilot's agent mode

## Available Tools (in Agent Mode)

Once installed, the following tools become available in VS Code's agent mode.

### Authentication & Server Management (All Users)

| Tool | Description |
|------|-------------|
| `login` | Sign in to CodeScene with OAuth (opens browser) |
| `logout` | Sign out of CodeScene OAuth and clear the stored session |
| `get_config` | Read current server configuration |
| `set_config` | Write a configuration value |
| `verify_installation` | Diagnose setup issues |
| `list_skills` | List available embedded skills |
| `get_skill_manifest` | Inspect a skill's file manifest |
| `download_skill` | Download a skill to a local directory |
| `sync_skills` | Download all available skills |

### Code Health Analysis (All Users)

These tools work after OAuth login or with any valid access token — standalone or CodeScene Core.

| Tool | Description |
|------|-------------|
| `code_health_score` | Calculate the Code Health score (1.0–10.0) for a file |
| `code_health_review` | Detailed review with code smells and refactoring guidance |
| `pre_commit_code_health_safeguard` | Check staged/modified files before committing |
| `analyze_change_set` | PR pre-flight: compare branch vs base for regressions |
| `code_health_refactoring_business_case` | Quantified business case for refactoring a file |
| `explain_code_health` | Explains the Code Health metric and how scores are interpreted |
| `explain_code_health_productivity` | Business case data for Code Health improvements |

### Technical Debt & Project Insights (CodeScene Core — cloud or on-prem)

These tools require OAuth or a CodeScene Personal Access Token and a CodeScene Core instance.

| Tool | Description |
|------|-------------|
| `select_project` | List and select CodeScene projects |
| `list_technical_debt_hotspots_for_project` | Find high-impact technical debt hotspots across a project |
| `list_technical_debt_hotspots_for_project_file` | Hotspot metrics for a specific file |
| `list_technical_debt_goals_for_project` | View active refactoring goals for a project |
| `list_technical_debt_goals_for_project_file` | Goals for a specific file |
| `code_ownership_for_path` | Identify code owners for a file or directory |

## Getting Started

1. **Install the extension** from the VS Code Marketplace
2. **(On-prem only)** Set `codescene.onpremUrl` to your instance URL
3. **Sign in** using one of:
   - The `login` MCP prompt (slash command) in Copilot Chat
   - **CodeScene: Sign In** from the Command Palette
   - Asking in agent mode: *“Log me in to CodeScene”* (calls the `login` tool)
4. **(Multi-account Cloud)** Use **CodeScene: Switch Account** (or the `switch_account` tool) with your numeric account ID — do not rely on changing `codescene.accountId` alone while signed in
5. Start using CodeScene tools

### Authentication

**Recommended (interactive):** OAuth via the `login` MCP prompt / `login` tool, or **CodeScene: Sign In**. No token to copy or paste. Sign out with the `logout` prompt / tool, or **CodeScene: Sign Out**.

**Multi-account Cloud:** Use **CodeScene: Switch Account** or ask the agent to call `switch_account`. Changing `codescene.accountId` in Settings alone does not retarget an active OAuth session.

**Optional (CI / headless):** Set a Personal Access Token or standalone license via `CodeScene: Configure Access Token (optional / CI)` or the `codescene.accessToken` setting. A saved PAT **blocks** OAuth until you clear it. Sign Out clears OAuth only — remove the PAT separately if needed.

- **OAuth / Personal Access Token** — Full tool set, including project-level features.
- **Standalone access token** — Local Code Health analysis only (scoring, review, refactoring).

See [Authentication](https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/authentication.md) for details.

## Settings

| Setting | Description |
|---------|-------------|
| `codescene.enabled` | Enable/disable the MCP server |
| `codescene.accountId` | Optional Cloud account/tenant ID (prefer Switch Account while signed in) |
| `codescene.onpremUrl` | Base URL of your self-hosted CodeScene instance |
| `codescene.defaultProjectId` | Pre-select a project by numeric ID |
| `codescene.accessToken` | Optional PAT or standalone token (blocks OAuth when set) |
| `codescene.enabledTools` | Comma-separated list of tools to expose (empty = all) |
| `codescene.disableVersionCheck` | Suppress automatic version-check on startup |
| `codescene.caBundlePath` | Path to a PEM CA certificate bundle for SSL/TLS |

See [Configuration Options](https://github.com/codescene-oss/codescene-mcp-server/blob/main/docs/configuration-options.md) for full documentation.

## MCP Prompts (slash commands)

| Prompt | Description |
|--------|-------------|
| `login` | Sign in to CodeScene with OAuth (instructs the agent to call the `login` tool) |
| `switch_account` | Switch CodeScene Cloud OAuth account (instructs the agent to call `switch_account`) |
| `logout` | Sign out of CodeScene OAuth (instructs the agent to call the `logout` tool) |
| `review_code_health` | Review Code Health for the current file |
| `plan_code_health_refactoring` | Plan a prioritized, low-risk Code Health refactoring |

In Copilot Chat, pick these from the slash-command / prompt picker (MCP prompts appear as `mcp.<server>.<prompt>`).

## Commands

- `CodeScene: Sign In` — Start the OAuth browser login flow
- `CodeScene: Sign Out` — Clear the stored OAuth session
- `CodeScene: Switch Account` — Switch Cloud OAuth account (reuses a stored session when possible)
- `CodeScene: Configure Access Token (optional / CI)` — Set or clear a PAT/standalone token
- `CodeScene: Restart MCP Server` — Restart the MCP server (after config changes)
- `CodeScene: Show Server Status` — Display current server status and configuration

## Supported Platforms

| Platform | Architecture |
|----------|-------------|
| macOS | Apple Silicon (ARM64) |
| macOS | Intel (x64) |
| Linux | x64 |
| Linux | ARM64 |
| Windows | x64 |

## License

MIT

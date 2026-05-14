# CodeScene Code Health Plugin for Claude Code

A Claude Code plugin that provides CodeScene Code Health analysis tools and skills for reviewing, refactoring, and safeguarding code quality.

## Installation

### From a marketplace

If this plugin is listed in a marketplace, install it with:

```
/plugin install codescene
```

### Local testing

```bash
claude --plugin-dir ./claude-code-plugin
```

## Skills

Once installed, the following skills are available:

<!-- SKILLS-TABLE:START -->
| Skill | Description |
|-------|-------------|
<!-- SKILLS-TABLE:END -->

## MCP Tools

The plugin registers the CodeScene MCP server, which provides the following tools:

### Standalone (no CodeScene account required)

| Tool | Description |
|------|-------------|
| `code_health_review` | Review a file's Code Health with score and code smell findings |
| `code_health_score` | Calculate the numeric Code Health score (1.0–10.0) for a file |
| `pre_commit_code_health_safeguard` | Check all modified/staged files for Code Health regressions before commit |
| `analyze_change_set` | Branch-level Code Health review for PR pre-flight checks |
| `code_health_refactoring_business_case` | Generate a data-driven business case for refactoring a file |
| `code_health_auto_refactor` | Auto-refactor a function to fix code health smells (requires CodeScene ACE add-on) |
| `explain_code_health` | Explain the Code Health metric and how to interpret scores |
| `explain_code_health_productivity` | Describe the productivity and defect-risk impact of Code Health |
| `get_config` | Read current MCP server configuration |
| `set_config` | Write MCP server configuration values |

### Requires CodeScene instance

These tools require a CodeScene API access token and are removed in standalone mode:

| Tool | Description |
|------|-------------|
| `select_project` | List and select a CodeScene project |
| `list_technical_debt_hotspots_for_project` | List technical debt hotspots across a project |
| `list_technical_debt_hotspots_for_project_file` | List hotspot metrics for a specific file |
| `list_technical_debt_goals_for_project` | List technical debt goals for a project |
| `list_technical_debt_goals_for_project_file` | List technical debt goals for a specific file |
| `code_ownership_for_path` | Find owners of a file or directory |

## Requirements

- Node.js (for `npx` to run the MCP server)
- The MCP server binary is downloaded automatically on first run via the `@codescene/codehealth-mcp` npm package

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

| Skill | Description |
|-------|-------------|
| `/codescene:review` | Review Code Health of files — scores, smells, and prioritized actions |
| `/codescene:refactor` | Guided refactoring using Code Health findings |
| `/codescene:safeguard` | Pre-commit and pre-PR Code Health regression checks |
| `/codescene:tech-debt` | Identify and prioritize technical debt hotspots |

## MCP Tools

The plugin registers the CodeScene MCP server, which provides tools for:

- `code_health_score` / `code_health_review` — file-level Code Health analysis
- `pre_commit_code_health_safeguard` — pre-commit regression check
- `analyze_change_set` — branch-level PR pre-flight check
- `code_health_auto_refactor` — AI-assisted refactoring (requires CodeScene ACE)
- `list_technical_debt_hotspots` / `list_technical_debt_goals` — debt prioritization
- `code_health_refactoring_business_case` — ROI quantification

## Requirements

- Node.js (for `npx` to run the MCP server)
- The MCP server binary is downloaded automatically on first run via the `@codescene/codehealth-mcp` npm package

---
name: "codescene-code-health"
displayName: "CodeScene Code Health"
description: "Use CodeScene Code Health analysis to safeguard AI-generated code, prioritize technical debt, and guide maintainable refactoring."
keywords:
  - "codescene"
  - "code health"
  - "technical debt"
  - "hotspots"
  - "refactoring"
  - "maintainability"
  - "mcp"
  - "quality gate"
author: "CodeScene"
---

# Onboarding

## Step 1: Verify access token setup

Before using this power, ensure your CodeScene access token is configured.

- Ask the agent to run `get_config` and check `access_token`, or set one via `set_config`.
- If you use CodeScene on-prem, also set `onprem_url`.

## Step 2: Identify license mode and available tools

This MCP server runs in one of two modes, and tool availability differs.

### Standalone mode (standalone MCP license)

Available tools:

- `explain_code_health`
- `explain_code_health_productivity`
- `code_health_review`
- `code_health_score`
- `pre_commit_code_health_safeguard`
- `analyze_change_set`
- `code_health_refactoring_business_case`

Not available in standalone mode:

- `select_project`
- `list_technical_debt_goals_for_project`
- `list_technical_debt_goals_for_project_file`
- `list_technical_debt_hotspots_for_project`
- `list_technical_debt_hotspots_for_project_file`
- `code_ownership_for_path`

Standalone users do not have CodeScene project context, so project-level APIs are removed automatically.

### CodeScene Cloud or CodeScene On-prem mode (PAT/API token)

Includes all standalone tools, plus project-level API tools:

- `select_project`
- `list_technical_debt_goals_for_project`
- `list_technical_debt_goals_for_project_file`
- `list_technical_debt_hotspots_for_project`
- `list_technical_debt_hotspots_for_project_file`
- `code_ownership_for_path`

On-prem users must configure `onprem_url`.

## Step 3: Verify MCP tools are available

Run a simple tool call such as `explain_code_health` to verify the power and server are connected.


# Mode-aware workflows

Use this power whenever the task includes AI-generated code, refactoring, technical debt triage, or release readiness checks.

## For all users (standalone and Cloud/On-prem)

### Safeguard AI-generated or modified code

1. Run `pre_commit_code_health_safeguard` before commit preparation.
2. If any file degrades, run `code_health_review` on impacted files.
3. Refactor in small steps, then re-check Code Health.
4. Do not consider code ready while degradations remain unless explicitly accepted.

### Pull request pre-flight

1. Run `analyze_change_set` against the intended base branch.
2. Investigate degraded files with `code_health_review`.
3. Improve regressions before opening or updating the PR.

### Refactoring workflow

1. Start with `code_health_review`.
2. Identify specific maintainability problems (complexity, deep nesting, large methods, etc.).
3. Refactor in 3-5 reviewable steps.
4. Re-run `code_health_score` or `code_health_review` after significant steps.
5. Target Code Health 10.0 where practical.

## Additional workflows for Cloud/On-prem users

Use these tools to identify what to improve first:

- `list_technical_debt_hotspots_for_project`
- `list_technical_debt_goals_for_project`
- `code_health_score`
- `code_health_refactoring_business_case` (for ROI framing)

## Project setup notes (Cloud/On-prem)

- Use `select_project` before project-scoped tools.
- Use `code_ownership_for_path` to identify likely reviewers.

## License and support

This power integrates with [CodeScene MCP Server](https://github.com/codescene-oss/codescene-mcp-server) (MIT).

- [Privacy Policy](https://codescene.com/policies)
- [Support](https://github.com/codescene-oss/codescene-mcp-server/issues)

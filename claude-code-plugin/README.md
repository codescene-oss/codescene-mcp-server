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
| `/codescene:configuring-codescene-mcp` | Use when the user wants to view, set, or troubleshoot CodeScene MCP configuration such as access tokens, on-prem URLs, default projects, or SSL certificates. |
| `/codescene:explaining-code-health` | Use when a user asks what Code Health means, how to interpret scores, or why Code Health matters in daily development. |
| `/codescene:guiding-refactoring-with-code-health` | Use when refactoring unhealthy code and needing Code Health findings to choose small safe steps and verify improvement. |
| `/codescene:installing-and-activating-codescene-mcp` | Use when installing the CodeScene MCP Server binary or package, registering it in an AI assistant, or copying agent guidance files into a repository. |
| `/codescene:making-the-business-case-for-code-health` | Use when a user asks for ROI, stakeholder justification, delivery impact, or defect-risk reduction from improving Code Health in a file. |
| `/codescene:prioritizing-technical-debt` | Use when users with a CodeScene instance ask what to improve first across a project, which hotspots matter most, or how to rank refactoring candidates. |
| `/codescene:risk-based-testing-with-code-health` | Use when a user asks what to test first based on CodeScene findings, especially for high-risk hotspots or pull-request change sets. |
| `/codescene:routing-work-with-code-ownership` | Use when choosing reviewers, domain experts, or likely owners for a file or directory from CodeScene project data. |
| `/codescene:safeguarding-ai-generated-code` | Use when AI-generated or AI-modified changes need a Code Health gate before commit, handoff, or pull request. |
<!-- SKILLS-TABLE:END -->

## MCP Tools

The plugin registers the CodeScene MCP server, which provides the following tools:

### Standalone mode (standalone MCP license)

| Tool | Description |
|------|-------------|
| `code_health_review` | Review a file's Code Health with score and code smell findings |
| `code_health_score` | Calculate the numeric Code Health score (1.0–10.0) for a file |
| `pre_commit_code_health_safeguard` | Check all modified/staged files for Code Health regressions before commit |
| `analyze_change_set` | Branch-level Code Health review for PR pre-flight checks |
| `code_health_refactoring_business_case` | Generate a data-driven business case for refactoring a file |
| `explain_code_health` | Explain the Code Health metric and how to interpret scores |
| `explain_code_health_productivity` | Describe the productivity and defect-risk impact of Code Health |
| `get_config` | Read current MCP server configuration |
| `set_config` | Write MCP server configuration values |

### CodeScene Cloud or On-prem mode (PAT/API token)

Includes all standalone tools, plus project-level API tools:

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

# CodeScene MCP Skills

This directory contains downloadable public skills for agents that use the CodeScene MCP Server.

Each skill is organized as:

```text
skills/
  skill-name/
    SKILL.md
```

Available skills:

- `installing-and-activating-codescene-mcp`: Install the MCP server, configure required environment variables, and activate it in an AI assistant.
- `safeguarding-ai-generated-code`: Apply Code Health safeguards before commit or pull request.
- `guiding-refactoring-with-code-health`: Use Code Health findings to plan and validate small refactors.
- `making-the-business-case-for-code-health`: Quantify delivery and defect impact to justify refactoring work.
- `prioritizing-technical-debt`: Use CodeScene project data to rank what to improve first.
- `routing-work-with-code-ownership`: Use CodeScene ownership data to find likely reviewers and domain experts.
- `explaining-code-health`: Explain Code Health concepts and why they matter.

These public skills are separate from the repo-local contributor skills under `.agents/skills`.
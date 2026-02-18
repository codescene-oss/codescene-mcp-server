---
name: analyze_change_set
description: Run a branch-level Code Health review of all changes compared to a base ref (PR pre-flight).
argument-hint: Base ref to compare against (e.g. main, origin/main)
tools:
  - analyze_change_set
---

Run a branch-level Code Health analysis using **CodeScene MCP Server** with the MCP tool **analyze_change_set**. This reviews all committed changes on the current branch compared to the given base ref.

After running the tool:

1. Summarize the JSON output in clear, actionable language. Focus on the overall result indicated in the `quality_gates` field.
2. Highlight any code smells or degradation introduced across the branch.
3. Prioritize findings by severity and likelihood of increasing long-term maintenance cost.
4. Provide practical recommendations for improvements before opening a PR.
5. Code Health 10.0 is optimal. Do not allow regressions below 10.0, not even minor ones.

Use concise language suitable for a PR pre-flight review. Avoid rewriting the code unless explicitly asked.

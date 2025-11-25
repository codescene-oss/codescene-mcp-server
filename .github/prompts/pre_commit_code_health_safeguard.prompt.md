---
name: pre_commit_code_health_safeguard
description: Run a Code Health safeguard on modified and staged files before committing.
argument-hint: Path to the Git repository to analyze
tools:
  - pre_commit_code_health_safeguard
---

Run a pre-commit Code Health safeguard on the current Git repository path. Use **CodeScene MCP Server** with the MCP tool **pre_commit_code_health_safeguard**.

After running the tool:

1. Summarize the JSON output in clear, actionable language. Focus on the overall result indicated in the `quality_gates` field of the JSON result.
2. Highlight any code smells or degradation that would reduce Code Health if committed.
3. Prioritize the findings by severity and likelihood of increasing long-term maintenance cost.
4. Provide practical recommendations for improvements prior to committing.

Use concise language suitable for a pre-commit review. Avoid rewriting the code unless explicitly asked.
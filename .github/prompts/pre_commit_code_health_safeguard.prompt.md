---
name: pre_commit_code_health_safeguard
description: Run a Code Health safeguard on modified and staged files before committing.
argument-hint: Path to the Git repository to analyze
---

Run a pre-commit Code Health safeguard on the current Git repository path using **CodeScene MCP Server** with **pre_commit_code_health_safeguard**.

After running the tool:

1. Summarize the JSON output in clear, actionable language. Focus on the overall result indicated in the `quality_gates` field of the JSON result.
2. Highlight any code smells or degradation that would reduce Code Health if committed.
3. Prioritize the findings by severity and likelihood of increasing long-term maintenance cost.
4. If regressions are reported, identify which files need follow-up `code_health_review` work before committing.
5. Provide practical recommendations for improvements prior to committing.
6. If no regressions are found, state that explicitly.

Treat Code Health 10.0 as the ideal long-term target, but use this prompt to block regressions and encourage measurable improvement rather than requiring a perfect score on every commit.

Use concise language suitable for a pre-commit review. This prompt is a broader safety gate and should complement, not replace, file-by-file review during editing. Avoid rewriting code unless explicitly asked.
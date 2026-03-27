---
name: analyze_change_set
description: Run a branch-level Code Health review of all changes compared to a base ref (PR pre-flight).
argument-hint: Base ref to compare against (e.g. main, origin/main)
---

Run a branch-level Code Health analysis using **CodeScene MCP Server** with **analyze_change_set**. This is a PR-style safety gate for committed changes on the current branch compared to the given base ref.

After running the tool:

1. Summarize the JSON output in clear, actionable language. Focus on the overall result indicated in the `quality_gates` field.
2. Highlight any code smells or degradation introduced across the branch.
3. Prioritize findings by severity and likelihood of increasing long-term maintenance cost.
4. If regressions are reported, identify which files need follow-up `code_health_review` work before opening a PR.
5. Provide practical recommendations for improvements before opening a PR.
6. If no regressions are found, state that explicitly and mention any residual risks or testing gaps.

Treat Code Health 10.0 as the ideal long-term target, but use this prompt to prevent regressions and encourage measurable improvement rather than requiring a perfect score on every branch.

Use concise language suitable for a PR pre-flight review. This prompt is a late safety gate, not a replacement for per-file reviews during editing. Avoid rewriting code unless explicitly asked.

---
description: Review the Code Health of a file or set of files. Use when the user asks to review, check, or assess code quality, maintainability, or technical debt in specific files.
---

# Code Health Review

Use the CodeScene MCP tools to review code quality:

1. Run `code_health_review` on the target file to get the full Code Health score and code smell findings.
2. Summarize findings into prioritized refactoring actions.
3. If the user wants a quick score only, use `code_health_score` instead.

## Interpreting Scores

- **10.0** — Optimal: code is optimized for both human and AI comprehension.
- **9.0–9.9** — Green: high quality.
- **4.0–8.9** — Yellow: problematic technical debt.
- **1.0–3.9** — Red: severe technical debt, expensive to maintain.

The target is always **Code Health 10.0**. Scores of 9+ are not "good enough" — present concrete steps to reach 10.

---
description: Identify and prioritize technical debt. Use when the user asks what to improve, where to focus refactoring effort, or wants to understand technical debt hotspots.
---

# Technical Debt Prioritization

Use CodeScene tools to identify high-impact refactoring targets:

1. Select the project with `select_codescene_project` if not already done.
2. Use `list_technical_debt_hotspots` to find files with the worst combination of low Code Health and high change frequency.
3. Use `list_technical_debt_goals` to see existing improvement targets.
4. Use `code_health_score` to rank files by maintainability risk.
5. Optionally use `code_health_refactoring_business_case` to quantify ROI for specific files.

Always produce:
- A ranked list of hotspots with scores.
- Small, incremental refactor plans (3-5 steps per file).
- Business justification when relevant (defect risk reduction, development speed improvement).

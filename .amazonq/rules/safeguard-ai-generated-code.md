# Safeguard AI-Generated Code

AI-generated code must meet the project's objective maintainability standard.  
Use **Code Health**, retrieved via the **CodeScene MCP**, as the authoritative signal.

## Required Behavior

- After each meaningful AI modification to a file, run `codescene.code_health_review` on that file.
- Use `codescene.code_health_review` as the primary inner feedback loop for structural and maintainability issues.
- Use `codescene.pre_commit_code_health_safeguard` later as a broader pre-commit safety gate.
- Use `codescene.analyze_change_set` later as a branch or PR safety gate.
- If generated code reduces Code Health or violates technical-debt goals:
  - Refactor the code using the feedback from `codescene.code_health_review`.
  - Continue refining until Code Health is stable or improved, or the user explicitly accepts the risk.
- Hotspots and low-health areas are handled with extra caution:
  - Use `codescene.list_technical_debt_hotspots_for_project` or `codescene.list_technical_debt_hotspots_for_project_file` when project context is available.

## Non-Negotiable Rule

AI-generated code **must not** introduce Code Health regressions.  
If a regression is detected, **refactor immediately** using CodeScene insights until resolved.

Code Health 10.0 is the ideal long-term target, but the immediate requirement is to prevent regressions and push changes toward measurable improvement.

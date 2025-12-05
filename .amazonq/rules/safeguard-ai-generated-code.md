# Safeguard AI-Generated Code

AI-generated code must meet the project's objective maintainability standard.  
Use **Code Health**, retrieved via the **CodeScene MCP**, as the authoritative signal.

## Required Behavior

- AI-generated or AI-modified code must be evaluated using:
  - `codescene.code_health_review` for structural and maintainability issues.
  - `pre_commit_code_health_safeguard` to check whether the change impacts Code Health as indicated by the `quality_gates`.
- If generated code reduces Code Health or violates technical-debt goals:
  - Automatically refactor the code using the feedback from `code_health_review`.
  - Continue refining until Code Health is stable or improved.
- Hotspots and low-health areas are handled with extra caution:
  - Use `codescene.list_technical_debt_hotspots` to understand the risks before modifying them.

## Non-Negotiable Rule

AI-generated code **must not** introduce Code Health regressions.  
If a regression is detected, **refactor immediately** using CodeScene insights until resolved.

# Refactoring Standards

Refactoring must produce **measurable improvements** in maintainability.  
Use the **CodeScene MCP** to assess whether a refactor is an objective improvement.

## Required Behavior

### Before refactoring:
  - Run `codescene.code_health_review` to identify problem areas.
  - Capture the current `codescene.code_health_score` so the work starts from a measurable baseline.

### During refactoring
- Prefer **small, incremental improvements** that are easy to review and validate.
- Focus on **structural refactorings** that reduce responsibilities, nesting, coupling, and hard-to-follow control flow.
- Do not treat formatting, renaming, or other cosmetic cleanup alone as meaningful Code Health improvement.


### After refactoring:
  - Re-run `codescene.code_health_review` after each meaningful step to ensure identified issues were actually improved and confirm no Code Health regression.
  - Use `codescene.code_health_score` as the compact trend check across iterations.

## Success Criteria

A refactoring is successful only if:

- Code Health improves or at minimum does not decline.
- Readability and structure are improved.
- No new technical debt is introduced.

Code Health 10.0 is the ideal long-term target. Aim for measurable progress toward it even when a full uplift is not realistic in one pass.

## Business Alignment

When asked about the value of a refactoring:
- Use `codescene.code_health_refactoring_business_case`  
  to explain expected productivity gains, risk reduction, or change cost improvements.

Refactoring must be justified, measurable, and verified using CodeScene’s Code Health metrics.

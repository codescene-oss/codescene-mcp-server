# Refactoring Standards

Refactoring must produce **measurable improvements** in maintainability.  
Use the **CodeScene MCP** to assess whether a refactor is an objective improvement.

## Required Behavior

### Before refactoring:
  - Run `codescene.code_health_review` to identify problem areas.
  - Use `codescene.list_technical_debt_hotspots` when working in risky or frequently changed code.

### During refactoring
- Prefer **small, incremental improvements** that are easy to review and validate.
- When Code Health indicates **large or complex functions** in languages supported by ACE, and ACE is available:
  - Use `codescene.code_health_auto_refactor` as an **initial step** to increase modularity by splitting functions into smaller, cohesive units.
- After ACE (or if ACE is not available):
  - Continue refactoring manually by simplifying structure, reducing nesting, improving naming, and clarifying responsibilities.
  - Treat ACE as a **modularity booster**, not a replacement for thoughtful refactoring.


### After refactoring:
  - Re-run `codescene.code_health_review` to ensure identified issues were actually improved and confirm no Code Health regression.

## Success Criteria

A refactoring is successful only if:

- Code Health improves or at minimum does not decline.
- Readability and structure are improved.
- No new technical debt is introduced.

## Business Alignment

When asked about the value of a refactoring:
- Use `codescene.code_health_refactoring_business_case`  
  to explain expected productivity gains, risk reduction, or change cost improvements.

Refactoring must be justified, measurable, and verified using CodeScene’s Code Health metrics.

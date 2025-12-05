# Refactoring Standards

Refactoring must produce **measurable improvements** in maintainability.  
Use the **CodeScene MCP** to assess whether a refactor is an objective improvement.

## Required Behavior

- Before refactoring:
  - Run `codescene.code_health_review` to identify problem areas.
  - Use `codescene.list_technical_debt_hotspots` when working in risky or frequently changed code.

- During refactoring:
  - Make small, incremental improvements.
  - Simplify structure, reduce nesting, break up large units, and improve readability.

- After refactoring:
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

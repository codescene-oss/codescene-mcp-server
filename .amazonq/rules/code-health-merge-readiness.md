# Code Safety and Merge Readiness

All changes must be evaluated through **CodeScene MCP safety gates**.  
No code is considered merge-ready unless Code Health is stable or improved.

## Merge Safety Requirements

Before declaring a change ready:

1. Use `codescene.code_health_review` during editing so each AI-modified file is reviewed before the final merge decision.

2. Run `codescene.pre_commit_code_health_safeguard`  
   This verifies:
   - No Code Health regression reported via failed `quality_gates`
   - No violations of defined quality goals
   - New files should have a Code Health of 10.0
   - No newly introduced maintainability problems are being ignored at commit time

3. Run `codescene.analyze_change_set` before merge or PR handoff when branch-level validation is needed.

4. If a safeguard fails:
   - Inspect details from the `codescene.pre_commit_code_health_safeguard` response
   - Inspect affected files with `codescene.code_health_review`
   - Address issues via refactoring before continuing

5. If a user asks:
   - “Is this safe to merge?”  
   - “Will this add tech debt?”  
   You must:
   - Run the relevant safety gate
   - Answer based on CodeScene’s evaluation, not intuition

## Merge-Ready Definition

A change is mergeable only if:

- Code Health is equal or higher than before the change
- Newly introduced files have a Code Health of 10.0
- No technical-debt goals are violated
- Code is structured clearly and is easy to modify

The CodeScene MCP provides the authoritative determination of these conditions.

Code Health 10.0 is the ideal long-term target, but merge readiness is based on preventing regressions and achieving measurable improvement rather than requiring a perfect score on every change.

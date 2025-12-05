# Code Safety and Merge Readiness

All changes must be evaluated through the **CodeScene MCP pre-commit safeguard**.  
No code is considered merge-ready unless Code Health is stable or improved.

## Merge Safety Requirements

Before declaring a change ready:

1. Run `codescene.pre_commit_code_health_safeguard`  
   This verifies:
   - No Code Health regression reported via failed `quality_gates`
   - No violations of defined quality goals
   - New files should have a Code Health of 10.0

2. If the safeguard fails:
   - Inspect details from the `codescene.pre_commit_code_health_safeguard` response
   - Address issues via refactoring before continuing

3. If a user asks:
   - “Is this safe to merge?”  
   - “Will this add tech debt?”  
   You must:
   - Run `pre_commit_code_health_safeguard`
   - Answer based on CodeScene’s evaluation, not intuition

## Merge-Ready Definition

A change is mergeable only if:

- Code Health is equal or higher than before the change
- No technical-debt goals are violated
- Code is structured clearly and is easy to modify

The CodeScene MCP provides the authoritative determination of these conditions.

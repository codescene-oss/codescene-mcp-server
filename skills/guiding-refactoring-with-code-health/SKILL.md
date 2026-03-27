---
name: guiding-refactoring-with-code-health
description: Use when refactoring unhealthy code and needing Code Health findings to choose small safe steps and verify improvement.
---

# Guiding Refactoring With Code Health

## Overview

Use Code Health as the control signal for refactoring. The agent should first understand why a file is hard to work with, then improve it in small steps and verify that each step helped.

## When to Use

- A file is hard to read, risky to change, or repeatedly attracts defects.
- The user asks for refactoring help and wants an objective way to measure progress.
- A safeguard or review points to complexity, size, low cohesion, or deep nesting.

Do not use this skill when the task is to rank project-wide priorities. Use `prioritizing-technical-debt` for that.

## Quick Reference

- `code_health_review`: Detailed maintainability findings for a file.
- `code_health_score`: Numeric checkpoint before and after a refactor.

## Implementation

1. Run `code_health_review` on the target file.
2. Identify the highest-leverage maintainability problems.
3. Propose 3 to 5 small refactor steps, not a single rewrite.
4. Re-run `code_health_review` or `code_health_score` after each significant step.
5. Stop only when the file is materially healthier or the user chooses to pause.

## Common Mistakes

- Refactoring without a baseline review.
- Making a large rewrite that hides whether things improved.
- Reporting cosmetic cleanup as Code Health improvement.
- Forgetting to re-measure after each meaningful step.
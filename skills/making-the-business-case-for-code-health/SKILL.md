---
name: making-the-business-case-for-code-health
description: Use when a user asks for ROI, stakeholder justification, delivery impact, or defect-risk reduction from improving Code Health in a file.
---

# Making the Business Case for Code Health

## Overview

Use this skill when the user needs quantified justification for refactoring work. The purpose is to convert current Code Health into a clear argument about delivery speed and defect reduction.

## When to Use

- A user asks for ROI, business value, or management justification.
- The workflow needs evidence to prioritize refactoring work.
- An engineering team needs a short case for why improving a file is worth the effort.

Do not use this skill to explain Code Health fundamentals. Use `explaining-code-health` for that.

## Quick Reference

- `code_health_refactoring_business_case`: Generate modeled outcomes for one file.
- `code_health_score`: Optional supporting baseline when the user wants the current score called out separately.

## Implementation

1. Run `code_health_refactoring_business_case` for the target file.
2. Present the recommended target scenario.
3. Summarize the optimistic and pessimistic outcomes as a bounded range, not a promise.
4. Translate the output into a short investment rationale tied to delivery speed and defect reduction.
5. If needed, pair the result with `code_health_score` or `code_health_review` to show why the file is a refactoring candidate.

## Common Mistakes

- Presenting modeled outcomes as guarantees.
- Using this skill for project-wide ranking without first narrowing candidate files.
- Explaining raw JSON instead of converting it into a short business argument.
- Forgetting that the tool is file-scoped.
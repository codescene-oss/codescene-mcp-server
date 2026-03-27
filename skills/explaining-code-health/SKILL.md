---
name: explaining-code-health
description: Use when a user asks what Code Health means, how to interpret scores, or why Code Health matters in daily development.
---

# Explaining Code Health

## Overview

Use this skill when the task is educational or explanatory. The goal is to help users understand the meaning of Code Health and why it matters for maintainability, delivery speed, and defect risk.

## When to Use

- A user asks what Code Health is.
- A user asks how to interpret scores or what a better score implies.
- A stakeholder needs a concise explanation of why Code Health matters.

Do not use this skill when the user needs a quantified ROI projection for a specific file. Use `making-the-business-case-for-code-health` for that.

## Quick Reference

- `explain_code_health`: Fundamentals of the Code Health model.
- `explain_code_health_productivity`: Evidence and framing around speed, quality, and productivity.

## Implementation

1. Use `explain_code_health` for fundamentals.
2. Use `explain_code_health_productivity` when the user also wants delivery or defect framing.
3. Tailor the explanation to the current task, repository, or planning decision.
4. Keep the explanation concrete and connect it to maintainability outcomes.

## Common Mistakes

- Using abstract language without connecting it to everyday engineering decisions.
- Jumping to ROI claims when the user only asked for fundamentals.
- Repeating documentation verbatim instead of tailoring it to the user’s context.
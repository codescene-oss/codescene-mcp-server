---
name: safeguarding-ai-generated-code
description: Use when AI-generated or AI-modified changes need a Code Health gate before commit, handoff, or pull request.
---

# Safeguarding AI-Generated Code

## Overview

Use Code Health safeguards before declaring AI-touched code ready. The goal is to catch maintainability regressions early and prevent agents from normalizing technical debt.

## When to Use

- The agent changed code and is about to suggest a commit.
- The user asks whether a branch or staged changes are safe to merge.
- The workflow needs a quality gate for AI-generated code.

Do not use this skill for broad refactoring discovery or project-level prioritization.

## Quick Reference

- `pre_commit_code_health_safeguard`: Check staged or modified files before commit.
- `analyze_change_set`: Check a branch or PR-style change set against a base ref.
- `code_health_review`: Inspect files that triggered the safeguard.

## Implementation

1. Run `pre_commit_code_health_safeguard` before commit-oriented recommendations.
2. Run `analyze_change_set` before PR-oriented recommendations.
3. If a regression is reported, inspect the affected files with `code_health_review`.
4. Refactor in small steps.
5. Re-run the safeguard until the regression is gone or the user explicitly accepts the risk.

## Common Mistakes

- Treating safeguard output as optional guidance instead of a release gate.
- Declaring work done after a failing safeguard.
- Jumping straight to broad rewrites instead of inspecting the flagged files first.
- Treating an accepted risk as invisible; call it out explicitly.
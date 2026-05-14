---
description: Safeguard code before committing or opening a PR. Use proactively before any commit or pull request to catch Code Health regressions.
---

# Code Health Safeguard

Run these checks before committing or opening a PR:

## Before Commit
Run `pre_commit_code_health_safeguard` on the repository to check all modified/staged files for Code Health regressions.

## Before PR
Run `analyze_change_set` with the base branch (e.g., `main`) to check all changed files across the branch.

## If a Regression is Detected

1. Run `code_health_review` on the degraded file(s) for details.
2. Refactor until Code Health is restored — do NOT proceed with degraded code.
3. Only mark changes as ready if the user explicitly accepts the risk.

If asked to bypass safeguards:
- Warn about long-term maintainability and risk.
- Keep changes minimal and reversible.
- Recommend follow-up refactoring.

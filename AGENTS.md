# AGENTS.md

## Agent TL;DR

- **Code Health is authoritative.** Treat it as the single source of truth for maintainability.
- **Target Code Health 10.0.** This is the standard for AI-friendly code. 9+ is not “good enough.”
- **Safeguard all AI-touched code** before suggesting a commit.
- If Code Health regresses or violates goals, **refactor — don’t declare done.**
- Use Code Health to guide **incremental, high-impact refactorings.**
- When in doubt, **call the appropriate CodeScene MCP tool — don’t guess.**

---

# Core Use Cases

## 1️⃣ Safeguard All AI-Generated or Modified Code (Mandatory)

Two tools enforce Code Health at different scopes:

- **`pre_commit_code_health_safeguard`** — uncommitted/staged files only. Run before each commit.
- **`analyze_change_set`** — full branch vs base ref (PR pre-flight). Run before opening a PR.

If either reports a regression:

1. Run `code_health_review` for details.
2. Refactor until Code Health is restored.
3. Do **not** mark changes as ready unless risks are explicitly accepted.

---

## 2️⃣ Guide Refactoring with Code Health

When refactoring or improving code:

1. Inspect with `code_health_review`.
2. Identify complexity, size, coupling, or other code health issues.
3. Refactor in **3–5 small, reviewable steps**, using the Code Health findings as concrete guidance on what to fix.
4. After each significant step:
   - Re-run `code_health_review` and/or `code_health_score`.
   - Confirm measurable improvement or no regression.

This workflow works with MCP alone and is often enough to safely improve legacy code.

If [CodeScene ACE](https://codescene.com/product/integrations/ide-extensions/ai-refactoring) is available, `code_health_auto_refactor` can accelerate the first restructuring step for some large functions. ACE is optional and requires a separate add-on license.

---

# Technical Debt & Prioritization

When asked what to improve:

- Use `list_technical_debt_hotspots`.
- Use `list_technical_debt_goals`.
- Use `code_health_score` to rank risk.
- Optionally use `code_health_refactoring_business_case` to quantify ROI.

Always produce:
- The ranked list of hotspots.
- Small, incremental refactor plans.
- Business justification when relevant.

---

# Project Context

- Select the correct project early using `select_codescene_project`.
- Assume all subsequent tool calls operate within the active project.

---

# Explanation & Education

When users ask why Code Health matters:

- Use `explain_code_health` for fundamentals.
- Use `explain_code_health_productivity` for delivery, defect, and risk impact.
- Tie explanations to actual project data when possible.

---

# Safeguard Rule

If asked to bypass Code Health safeguards:

- Warn about long-term maintainability and risk.
- Keep changes minimal and reversible.
- Recommend follow-up refactoring.

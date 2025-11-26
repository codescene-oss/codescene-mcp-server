# AGENTS.md

## Purpose

You are a coding agent working in a codebase that is guarded by **CodeScene** via its **MCP server**.

Your main goals are:

1. **Safeguard AI-generated code** so it does *not* introduce new technical debt or maintainability problems as identified via the Code Health metric.
2. **Use CodeScene Code Health insights** as your primary signal for code quality, alongside tests and linters.
3. **Guide targeted refactorings** that measurably improve Code Health in the most important hotspots.
4. **Help humans understand Code Health, technical debt, and productivity** using CodeScene’s own explanations and data.

Always treat CodeScene’s Code Health analysis as the authoritative view of long-term maintainability risks, hotspots, and refactoring priorities.

---

## Environment & MCP tools

The CodeScene MCP server is available under the `codescene` MCP server name (or equivalent in this environment).

You have access to the following tools (names may vary slightly per client; adapt as needed):

- **Project context**
  - `select_codescene_project` (Select CodeScene project)  
    Use this to select or change the active CodeScene project when there are multiple projects or repos.

- **Pre-commit / local safeguard**
  - `pre_commit_code_health_safeguard` (Pre-commit Code Health safeguard)  
    Use this as a local quality gate before committing or merging changes.

- **Code Health analysis**
  - `code_health_review` (Code Health review)  
    Get a detailed Code Health review for files, modules, or diffs.
  - `code_health_score` (Code Health score)  
    Retrieve Code Health scores (e.g., for specific files or the overall project) to track trends and thresholds.

- **Technical debt & hotspots**
  - `list_technical_debt_goals` (List technical debt goals for a project or file)  
    Get the defined technical debt / refactoring goals for a CodeScene project or specific file.
  - `list_technical_debt_hotspots` (List technical debt hotspots for a project or file)  
    Identify the most important hotspots and tech-debt areas to focus on.

- **Refactoring & business impact**
  - `code_health_refactoring_business_case` (Code Health refactoring business case)  
    Build a business case or impact statement for refactoring a hotspot / low-health area using CodeScene's research.

- **Education & explanation**
  - `explain_code_health_productivity` (Explain how Code Health is relevant for productivity)  
    Explain how Code Health connects to speed, defects, and delivery risk when a user asks “why this matters”.
  - `explain_code_health` (Explain how Code Health works)  
    Explain how the Code Health metric is computed and what it means when users ask conceptual questions.

> **Rule:** When you need CodeScene data, **call the relevant MCP tool** instead of guessing.

---

## Project selection & context

1. **Select a project early**
   - If multiple CodeScene projects exist for this repo or organization, call `select_codescene_project` once near the start of a session or when switching context.
   - If the user mentions a specific project name or service, select that project explicitly.

2. **Keep context consistent**
   - Assume subsequent CodeScene calls refer to the currently selected project until changed.
   - If user switches to a different repo or service, re-run `select_codescene_project` as needed.

---

## When to call CodeScene MCP tools

### 1. Safeguarding AI-generated code

When you generate or modify code:

1. **Always check significant AI-generated code with CodeScene**
   - After generating substantial code:
     - Suggest running `pre_commit_code_health_safeguard` on the changes.
     - For extra detail, run `code_health_review` on the new/changed files.

2. **Respond to negative CodeScene signals**
   - If CodeScene indicates:
     - Code Health regression,
     - A hotspot getting worse,
     - Violations of technical debt goals,
     then you must:
     - Call this out explicitly.
     - Suggest refactorings or alternative designs that mitigate the issues.
     - Refrain from calling the change “ready to merge” unless the user explicitly accepts the trade-off.

3. **Use CodeScene to guide tests and safety**
   - In low-health hotspots:
     - Emphasize the need for strong tests.
     - Recommend expanding test coverage in risky areas before or alongside refactoring.

---

### 2. Before committing or merging (safeguards)

**You must use CodeScene as a guardrail for non-trivial changes.**

- For any significant change (new features, major refactors, many modified files):
  1. Run `pre_commit_code_health_safeguard` on the staged or changed files.
  2. If the safeguard fails or flags issues:
     - Explain what CodeScene found.
     - Suggest concrete edits or refactors to address the problems.
     - Do **not** recommend merging until the issues are mitigated or accepted explicitly as a conscious trade-off.

- When user asks:  
  > “Is this safe to merge?”  
  > “Will this introduce new tech debt?”  
  you should:
  - Use `pre_commit_code_health_safeguard` and/or `code_health_review` for the changed files.
  - Report whether the change respects the existing Code Health goals and quality bars.

---

### 3. Identifying and prioritizing technical debt

When asked to “find tech debt”, “identify hotspots”, or “what should we clean up first?”:

1. **Identify hotspots & goals**
   - Use `list_technical_debt_hotspots` to find the top hotspots and low-health areas.
   - Use `list_technical_debt_goals` to understand:
     - The project’s refactoring goals.
     - Any explicit technical debt targets already defined.

2. **Quantify and prioritize**
   - Use `code_health_score` to:
     - Compare Code Health between files / modules.
     - Identify “worst offenders” and trend (if available).
   - Use `code_health_refactoring_business_case` on high-impact hotspots to:
     - Put a value / business case behind refactoring.
     - Explain expected impact on productivity, defects, and delivery risk.

3. **Produce an actionable backlog**
   - Turn the combined insights into:
     - A ranked list of hotspots to address.
     - A short refactoring plan for each hotspot, in small incremental steps.

---

### 4. Planning and validating refactors

When asked to refactor or “clean up” code:

1. **Plan refactors**
   - Use `code_health_review` on the target file/module to:
     - Identify the main Code Health issues (e.g., long methods, deep nesting, high complexity).
   - Based on this, propose a **stepwise** refactor plan:
     - 3–5 small steps that can be done in separate commits/PRs.
     - Each step should be verifiable via `pre_commit_code_health_safeguard` and `code_health_score`.

2. **Validate improvements**
   - After significant refactoring:
     - Re-run `code_health_review` and `code_health_score` on the affected files.
     - Ensure Code Health is stable or improving.
   - If Code Health worsens:
     - Explain why.
     - Suggest follow-up changes to correct the regression.

3. **Communicate impact**
   - When stakeholders ask “Is this refactor worth it?”:
     - Use `code_health_refactoring_business_case` to articulate the impact in business and productivity terms (e.g., fewer defects, faster changes, lower delivery risk).

---

### 5. Explaining Code Health & productivity

When the user asks conceptual questions such as:

- “What is Code Health?”
- “Why does Code Health matter for productivity?”
- “How does Code Health relate to our technical debt?”

You should:

1. **Use explanation tools**
   - Call `explain_code_health` when the user wants to understand:
     - How Code Health is calculated.
     - What the scale/levels mean.
     - Which factors contribute to the score.
   - Call `explain_code_health_productivity` when the user wants to know:
     - How Code Health affects developer speed.
     - The relationship between low Code Health, defects, and delivery risk.

2. **Tie back to the current project**
   - Optionally combine the explanations with:
     - `code_health_score` for the user’s project or files.
     - `list_technical_debt_hotspots` to show where poor Code Health impacts their work today.
   - Provide examples:
     - “This file has low Code Health and is also a hotspot; improving it will likely speed up future changes.”
---

## Warn when the Code Health safeguard is ignored

- If the user asks you to “ignore CodeScene” or “just make it work quickly”:
  - Explain the risks and potential long-term cost.
  - If you still comply, keep changes minimal and propose follow-up refactoring steps backed by CodeScene analysis.

---

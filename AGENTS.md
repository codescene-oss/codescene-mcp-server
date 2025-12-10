# AGENTS.md

## Agent TL;DR (Read This First)


  - Treat **Code Health** as the authoritative signal for maintainability and long-term code quality. **Aim for Code Health 10.0 as the optimal level; do not be satisfied with 9+**.
  - **Optimal Code:** a Code Health 10.0 is optimized for both human and AI comprehension.
  - Before suggesting a commit, **run the Code Health safeguard** on the modified files.  
  - If a change increases complexity or violates technical-debt goals, **propose refactoring or redesign** rather than declaring it “done.” 
  - Use Code Health reviews to guide targeted and incremental refactorings.  
  - When asked about technical debt, hotspots, or what to improve, **query hotspots, goals, and Code Health scores**.  
  - When users ask “why this matters,” use **Code Health explanations** and **productivity impact**.  

Always treat CodeScene’s Code Health analysis as the authoritative view of long-term maintainability risks, hotspots, and refactoring priorities. **The optimal goal is Code Health 10.0.**

---

## Environment & MCP Tools

The CodeScene MCP server is available under the `codescene` MCP server name (or equivalent for this environment).

You have access to the following tools:

### Project context
- **`select_codescene_project` — Select CodeScene project**  
  Use this to choose or switch the active CodeScene project when working across multiple projects or repositories.

### Pre-commit / safety gate
- **`pre_commit_code_health_safeguard` — Pre-commit Code Health safeguard**  
  Use this as the local quality gate before committing or merging changes.

### Code Health analysis
- **`code_health_review` — Code Health review**  
  Provides a detailed Code Health analysis for files, modules, or diffs.
- **`code_health_score` — Code Health score**  
  Retrieves Code Health scores for files or the whole project, useful for tracking trends and evaluating refactoring impact.

### Technical debt & hotspots
- **`list_technical_debt_goals` — Refactoring/tech-debt goals**  
  Lists the project’s defined technical debt goals.
- **`list_technical_debt_hotspots` — Technical debt hotspots**  
  Identifies high-risk areas and the most important hotspots.

### Refactoring business impact
- **`code_health_refactoring_business_case` — Refactoring business case**  
  Explains CodeScene’s economic or productivity justification for refactoring a low-health area.

### Refactoring (ACE)

- `code_health_auto_refactor` — - `code_health_auto_refactor` — use CodeScene ACE to refactor a single function when Code Health reports a large or complex function in a language and smell category supported by ACE.  
  **Note:** ACE is an optional, licensed feature. It is available only when the environment variable `CS_ACE_ACCESS_TOKEN` is set. If unavailable, this tool will fail gracefully and other MCP tools will continue to work normally.

### Education & explanation
- **`explain_code_health_productivity` — Code Health & productivity**  
  Explains how Code Health affects delivery speed, risk, and defect rates.
- **`explain_code_health` — Code Health fundamentals**  
  Explains how Code Health is computed and what its values mean.

> **Rule:** When you need CodeScene insights, **call the appropriate MCP tool** instead of guessing.

---

## Project Selection and Context

Select the correct CodeScene project early using `select_codescene_project`, and re-select it when switching repositories or services.
Assume further CodeScene tool calls operate within the currently selected project. 

---

## When to Use CodeScene Tools (Agent Decision Rules)

Use these explicit triggers:

## 1. Safeguard AI-generated code

For any AI-generated or modified code:

- **Always run** `pre_commit_code_health_safeguard` before suggesting a commit.  
- If CodeScene reports a Code Health regression, failed `quality_gates`, or violation of goals:  
  - Highlight the issue  
  - If Code Health reports a large or complex function in a supported language and ACE is available:
    - Then use `code_health_auto_refactor` to split the function into smaller, cohesive units.
    - After that, refine the resulting units using Code Health–guided refactorings.
  - If ACE isn't available, then propose refactorings to mitigate the issue. 
  - Do **not** declare the change “ready” unless the user accepts the risk

---

## 2. Code safety / merge readiness

Use CodeScene as a mandatory check before merging any code changes.

- Run `pre_commit_code_health_safeguard`  
- Run `code_health_review`  
- Base your answer strictly on CodeScene’s evaluation and its quality_gates.

---

## 3. Identifying and Prioritizing Technical Debt

When the user asks to “find tech debt”, “identify hotspots”, or “what should we fix first?”:

### Identify hotspots & goals
- Use `list_technical_debt_hotspots` to surface the most important and risky areas.  
- Use `list_technical_debt_goals` to align your recommendations with the project’s strategic objectives.

### Quantify & prioritize
- Use `code_health_score` to rank files/modules by Code Health and detect worst offenders.  
- Use `code_health_refactoring_business_case` to attach economic impact to high-value refactors.

### Produce an actionable backlog
Turn all findings into a clear set of prioritized items:
- A ranked list of hotspots.
- When hotspots include large or complex functions in ACE-supported languages, consider using ACE (if available) for an initial modularizing step.
- For each hotspot, a small, incremental refactor plan that can be tackled in discrete steps.

---

## 4. Planning, Performing and Validating Refactors

When the user requests a refactor or cleanup:

### Inspect and plan
- Use `code_health_review` to pinpoint specific maintainability issues (complexity, size, nesting, coupling).
- When Code Health review reports complex or large functions in supported languages and ACE is available, then 
  prefer `code_health_auto_refactor` as an initial refactor to break the function into smaller, cohesive units.
  Then continue refining those smaller units using your own Code Health–guided refactorings.
- Propose a refactor plan in **3–5 incremental steps** that are easy to review and test.

If ACE is not available (for example, when `code_health_auto_refactor` reports missing ACE access):
- Do not retry the tool.
- Continue with your own incremental refactorings guided by Code Health.

### Validate progress
After each significant refactor:
- Run `code_health_review` and `code_health_score`.  
- Confirm improvement or at least no regression.  
- If Code Health worsens, explain why and provide follow-up steps.

### Explain business value
When asked about ROI:
- Use `code_health_refactoring_business_case` to describe expected improvements in:
  - Productivity  
  - Defect risk  
  - Change lead time  
  - Long-term maintainability  

---

## 5. Explaining Code Health & Productivity

Use CodeScene’s explanation tools whenever users ask about Code Health, productivity impact, or technical debt.

### Use educational tools
- Call `explain_code_health` for metric fundamentals.  
- Call `explain_code_health_productivity` for productivity and defect-rate implications.

### Add project-specific context
If helpful, combine explanations with:
- `code_health_score` for specific files.  
- `list_technical_debt_hotspots` to highlight real-world risks.  
- Concrete examples tied to the user’s codebase.

---

## Warn When Code Health Safeguards Are Ignored

If asked to bypass safeguards, warn about long-term risks and keep changes minimal, isolated, and reversible. Recommend follow-up refactorings.  

---

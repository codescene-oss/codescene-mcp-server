---
name: risk-based-testing-with-code-health
description: Use when a user asks what to test first based on CodeScene findings, especially for high-risk hotspots or pull-request change sets.
---

# Risk-Based Testing With Code Health

## Overview
Use this skill when the goal is to turn CodeScene risk signals into practical testing priorities.
The output should help testers and developers focus limited test time on the code most likely to produce defects.

The primary signal is always the **current branch's change set** — what has actually changed is more immediately actionable than historical hotspots. Hotspots provide background context and catch systemic risk, but they should not displace focus from files the branch has already touched.

## When to Use
- A user asks where testing should focus based on CodeScene hotspots.
- A user asks for a tester-friendly test plan from technical debt or code health findings.
- A user wants pull-request risk translated into concrete test scenarios.
- A release owner asks for risk-based regression scope before ship.

Do not use this skill when the user only asks for conceptual definitions of Code Health.
Use `explaining-code-health` for fundamentals.

Do not use this skill when the user asks for quantified financial ROI of refactoring.
Use `making-the-business-case-for-code-health` for ROI framing.

## Minimum Inputs
- `project_id` (CodeScene project identifier)
- `base_ref` (for change-set analysis, e.g. `main` or `origin/main`)
- `git_repository_path` (absolute local repository path)
- Optional: business criticality hints from the user (for example: release-blocking areas, customer-facing commands)

## Quick Reference
- `list_technical_debt_hotspots_for_project`: Identify high-risk files using low Code Health and high historical churn.
- `analyze_change_set`: Evaluate immediate risk in the current branch vs target base.
- `code_health_score`: Inspect specific files in detail.
- `code_ownership_for_path`: Route high-risk areas to likely reviewers/domain experts.
- `list_technical_debt_goals_for_project`: Include existing debt goals when present.

## Implementation
1. **Analyse the current branch first**
   - Run `analyze_change_set` against the target base ref. This is the primary source of truth.
   - Identify which files were changed, whether Code Health degraded, and whether any quality gates failed.
   - If no branch context exists (e.g. ad-hoc analysis), skip to step 2 and treat hotspots as the primary signal instead.

2. **Add systemic context from hotspots**
   - Run `list_technical_debt_hotspots_for_project` to surface high-churn, low-health files.
   - Note which hotspots overlap with the change set — those are the highest combined risk.
   - Hotspots with no overlap are background risk; include them but do not let them overshadow branch findings.
   - If needed, run `code_health_score` on specific candidates for more detail.

3. **Prioritize by combined risk**
   - Change-set files with Code Health degradation or failed gates: always rank first.
   - Change-set files that are also hotspots: rank second.
   - Change-set files with healthy scores: rank third.
   - Hotspots not in the change set: background context only.

4. **Translate technical areas into business context**
   - For each top area, explain:
     - what user-facing capability it supports
     - what failures would look like for a user/customer
     - why it is risky now (with evidence)

5. **Generate tester-ready scenarios**
   - Provide concrete test charters:
     - happy path
     - edge cases
     - negative/error handling paths
     - platform/configuration matrix where relevant

6. **Route ownership for action**
   - Use `code_ownership_for_path` for top 3 risky files/areas.
   - Suggest reviewers/test collaborators.

## Required Output Schema
Always return the following sections in order:

1. **Top Risk Areas**
   - Area name
   - Business context
   - Why risky now (with CodeScene evidence)
   - Confidence and assumptions

2. **Priority Test Charters**
   - Scenario
   - Expected behavior
   - Failure signal
   - Environment matrix (OS/RID/device/framework if relevant)
   - Priority (`P0`, `P1`, `P2`)

3. **Must-Pass Before Merge/Release**
   - Minimal blocking checks
   - Pass/fail recommendation criteria

4. **Ownership and Routing**
   - Suggested owner/reviewer per top risk area
   - Next action handoff

5. **Open Risks**
   - Known unknowns
   - Deferred tests and rationale

## Quality Bar
- Do not return only class/method names; always include business context.
- Do not treat all low-score files as equally risky; include churn and change-set evidence.
- Do not produce generic advice; include concrete test scenarios and expected outcomes.
- Explicitly distinguish PR-local risk from systemic technical debt.

## Common Mistakes
- Listing hotspots without converting them into tester actions.
- Treating hotspots as the primary signal when an active branch change set is available.
- Running `list_technical_debt_hotspots_for_project` before `analyze_change_set` and letting the longer list crowd out branch findings.
- Omitting expected outcomes, which makes tests non-verifiable.
- Failing to identify who should own follow-up.
- Using abstract language with no user/business impact framing.

## Stop Conditions
If required setup is missing (for example, project not selected, invalid repo path, missing base ref):
- Stop and return a concise setup checklist.
- Do not generate speculative risk rankings.

## Example Trigger Phrases
- "Where should QA focus first for this PR?"
- "What should we regression test based on CodeScene?"
- "Translate these hotspots into a practical test plan."
- "Which high-risk areas are release blockers?"

---
name: business-risk-from-code-health
description: Use when a business owner, CPO, or executive asks which areas of the business are most at risk due to poor code health or technical debt, and wants the answer in customer and commercial terms.
---

# Business Risk From Code Health

## Overview

Use this skill when the goal is to give a non-technical executive a clear picture of where technical debt creates real business exposure. The output should not read as a technical report — it should map hotspots to the business capabilities they underpin, and explain risk in terms of customer experience, revenue, reliability, and delivery pace.

This skill is about surfacing risk to the business, not prescribing a technical fix. It answers the question: "What should I be worried about, and why does it matter to my customers and my bottom line?"

## When to Use

- A business owner, CPO, or product leader asks which parts of the product carry the most technical risk.
- A stakeholder wants to understand how technical debt translates into customer-facing or commercial exposure.
- A leadership team is preparing a roadmap and needs to know where ignoring debt will hurt them.
- An executive wants a risk register framed in business language, not code metrics.

Do not use this skill when the audience is an engineering team planning what to refactor next.
Use `prioritizing-technical-debt` for that.

Do not use this skill when the user wants file-level ROI to justify a specific refactoring investment.
Use `making-the-business-case-for-code-health` for that.

## Minimum Inputs

- `project_id` (CodeScene project identifier)
- Optional: business context from the user — which product areas are customer-facing, revenue-generating, or compliance-sensitive

## Quick Reference

- `list_technical_debt_hotspots_for_project`: Identify the files with the highest combined code health and churn risk.
- `list_technical_debt_goals_for_project`: Surface areas where the team has already flagged debt as a known concern.
- `code_health_refactoring_business_case`: Quantify delivery and defect impact for the highest-risk areas.
- `code_ownership_for_path`: Identify who is accountable for each high-risk business area.

## Implementation

1. **Identify the highest-risk technical areas**
   - Run `list_technical_debt_hotspots_for_project` to surface files with low Code Health and high churn.
   - Run `list_technical_debt_goals_for_project` to include areas the team has already flagged.
   - Focus on the top 5–10 areas by combined risk; do not present an exhaustive list.

2. **Map technical areas to business capabilities**
   - For each high-risk area, identify what business capability or customer-facing feature it supports.
   - Ask: what does this code *do* for the customer or the business? Name the capability in plain language (e.g. "checkout flow", "user authentication", "billing and invoicing", "reporting dashboard").
   - If the user has provided business context, use it to calibrate which capabilities matter most.

3. **Assess the business impact of each risk area**
   - For each capability, characterise the likely impact if the area degrades or fails:
     - **Customer experience**: slowdowns, errors, outages, loss of trust
     - **Revenue**: blocked transactions, churn, reduced conversion, missed upsell
     - **Reliability**: incident frequency, mean time to recover, SLA exposure
     - **Delivery pace**: how much harder each change in this area is, and what that means for roadmap confidence
   - Use `code_health_refactoring_business_case` on the top 3 areas to quantify delivery slowdown and defect likelihood with supporting evidence.

4. **Rank by business exposure**
   - Rank areas by combining: business criticality of the capability, severity of the Code Health finding, and churn evidence (how actively the area is being changed despite its risk).
   - Areas that are both business-critical *and* actively churning are the highest priority — risk is being realised now, not theoretically.

5. **Identify accountability**
   - Run `code_ownership_for_path` for the top 3 risk areas.
   - Name the team or individual most likely to own the risk and any recommended escalation path.

## Required Output Schema

Always return the following sections in order:

1. **Executive Summary**
   - 3–5 sentence plain-language summary of overall risk posture
   - Top 2–3 areas the business should be most concerned about, and why

2. **Business Risk Register**
   For each high-risk area:
   - Business capability (plain language, not file names)
   - Technical root cause (one sentence, accessible to a non-engineer)
   - Customer impact if it degrades or fails
   - Commercial or operational exposure (revenue, reliability, compliance, pace)
   - Evidence from CodeScene (health score, churn rate, debt goals)
   - Risk level (`High`, `Medium`, `Low`)

3. **Where Risk Is Being Realised Now**
   - Areas with both poor Code Health and active recent churn — risk is not theoretical here
   - Quantified delivery and defect impact from `code_health_refactoring_business_case` where available

4. **Accountability**
   - Suggested owner per top risk area
   - Recommended escalation or governance action

5. **What Would Sharpen This Picture**
   - Any business context that would improve the mapping (e.g. revenue attribution, customer journey mapping, SLA obligations)
   - Known gaps in the analysis

## Quality Bar

- Never present file paths or code metrics directly to the executive audience; always translate them first.
- Never treat all hotspots as equal; rank by business exposure, not just technical score.
- Always distinguish between latent risk (bad code that isn't changing) and active risk (bad code being changed frequently).
- Always include evidence from CodeScene — do not produce a risk register based on guesswork.
- Keep the executive summary genuinely short and decision-oriented.

## Common Mistakes

- Presenting a list of file names and Code Health scores to a business audience without translation.
- Treating technical debt prioritization as identical to business risk assessment — the audiences and decisions are different.
- Listing risks without stating what the business consequence actually is.
- Ignoring churn — a low-health file nobody touches is less urgent than a low-health file being changed every sprint.
- Over-qualifying every finding to the point the executive cannot act on the output.

## Stop Conditions

If no project is selected or hotspot data is unavailable:
- Stop and return a concise setup checklist.
- Do not generate speculative risk assessments.

## Example Trigger Phrases

- "Which parts of our product are most at risk from technical debt?"
- "Translate our code health findings into business risk."
- "What should I be worried about as a CPO?"
- "Where is technical debt most likely to hurt our customers?"
- "Give me a risk register I can take to the board."

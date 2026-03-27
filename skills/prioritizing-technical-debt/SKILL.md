---
name: prioritizing-technical-debt
description: Use when users with a CodeScene instance ask what to improve first across a project, which hotspots matter most, or how to rank refactoring candidates.
---

# Prioritizing Technical Debt

## Overview

Use this skill to bring high-level CodeScene project data into planning. The goal is to turn hotspots, goals, and file-level metrics into a ranked improvement plan rather than just listing raw results.

## When to Use

- The user asks what to improve first.
- The workflow needs a ranked list of refactoring targets.
- The user has a CodeScene instance and wants project-level guidance.

Do not use this skill for single-file refactoring mechanics. Use `guiding-refactoring-with-code-health` for that.

## Quick Reference

- `select_project`: Establish the correct project context.
- `list_technical_debt_hotspots_for_project`: Find high-impact hotspots.
- `list_technical_debt_goals_for_project`: Find files with explicit debt goals.
- `list_technical_debt_hotspots_for_project_file`: Drill into a shortlisted file.
- `list_technical_debt_goals_for_project_file`: Drill into file-level goals.
- `code_health_score`: Compare or validate shortlisted files.
- `code_health_refactoring_business_case`: Add ROI framing when the user wants justification.

## Implementation

1. Run `select_project` if the project context is not already clear.
2. Gather project hotspots and project goals.
3. Shortlist candidates based on hotspot severity, churn, and explicit goals.
4. Drill into the top files with the file-level hotspot and goal tools.
5. Use `code_health_score` to support ranking.
6. Return a ranked list, a small incremental plan for each top candidate, and business justification when relevant.

## Common Mistakes

- Skipping project selection and mixing data from the wrong project.
- Dumping hotspots and goals without ranking or synthesis.
- Using only one signal when hotspots and goals should be combined.
- Reaching for ROI before identifying the most plausible candidates.
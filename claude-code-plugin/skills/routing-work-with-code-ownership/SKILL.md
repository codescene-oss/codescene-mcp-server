---
name: routing-work-with-code-ownership
description: Use when choosing reviewers, domain experts, or likely owners for a file or directory from CodeScene project data.
---

# Routing Work With Code Ownership

## Overview

Use CodeScene ownership data to route work to the right people. This skill helps an agent connect files or directories to likely reviewers and domain experts.

## When to Use

- The user asks who should review or own a change.
- The workflow needs likely experts for a file, directory, or subsystem.
- The agent needs to connect refactoring recommendations to responsible people.

Do not use this skill to rank technical debt. Use `prioritizing-technical-debt` for that.

## Quick Reference

- `select_project`: Establish the correct project context if needed.
- `code_ownership_for_path`: Retrieve likely owners and their key paths for a file or directory.

## Implementation

1. Establish the correct project context.
2. Run `code_ownership_for_path` for the relevant file or directory.
3. Present likely owners with their key areas.
4. Use the ownership result to recommend reviewers, escalation paths, or stakeholders.

## Common Mistakes

- Using repository intuition instead of ownership data when the tool is available.
- Treating ownership as an absolute truth instead of a strong signal.
- Omitting the path context when the subsystem spans multiple files.
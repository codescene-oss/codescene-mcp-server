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
3. Present historical owners with their key areas and `owner_status`.
4. Recommend reviewers only when `reviewer_candidate` is true, and confirm availability before assigning work. A current contributor flag does not prove that the person is available.
5. Keep former contributors visible as historical owners and highlight ownership handover needs. When status is unknown, report that current reviewer routing could not be verified; do not assume the owner is current or invent a replacement.

## Common Mistakes

- Using repository intuition instead of ownership data when the tool is available.
- Treating ownership as an absolute truth instead of a strong signal.
- Recommending former contributors or owners with unknown status as current reviewers.
- Omitting the path context when the subsystem spans multiple files.

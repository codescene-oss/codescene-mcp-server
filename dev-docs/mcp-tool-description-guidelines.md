# MCP Tool Description Guidelines

This document defines how to write MCP tool descriptions in this repository based on:

- Hasan et al., "Model Context Protocol (MCP) Tool Descriptions Are Smelly!" ([arXiv HTML v2](https://arxiv.org/html/2602.14878v2))
- Existing CodeScene MCP conventions

## Why This Exists

Tool descriptions are runtime instructions for AI agents powered by foundation models, not passive docs. Poor descriptions degrade tool selection and argument quality.

The paper reports:

- 97.1% of analyzed MCP tools had at least one smell
- 56% had unclear purpose
- Full augmentation improved median success (+5.85 percentage points) and partial goal completion (+15.12%)
- Full augmentation also increased median execution steps (+67.46%) and regressed in 16.67% of cases

Conclusion: write complete descriptions, but keep them efficient and precise.

## Required Six Components

Every tool description MUST cover these six components.

1. Purpose
2. Guidelines
3. Limitations
4. Parameter Explanation
5. Length and Completeness
6. Examples

### 1) Purpose

- State exactly what the tool does in the first 1-2 sentences.
- Name the primary artifact returned (string, JSON object, list, etc.).

Smell to avoid: `Unclear Purpose`

### 2) Guidelines

- Give concrete usage direction for the agent:
  - when to use this tool
  - how to present/interpret the result
  - what to do next for common outcomes

Smell to avoid: `Missing Usage Guidance`

### 3) Limitations

- Explicitly list important constraints (scope, prerequisites, unsupported cases, error conditions).
- Be explicit about required environment/config state.

Smell to avoid: `Unstated Limitation(s)`

### 4) Parameter Explanation

- For each argument, describe semantics, expected format, and constraints.
- Do not rely on type hints alone.

Smell to avoid: `Opaque Parameters`

### 5) Length and Completeness

- Be complete enough for reliable behavior, but avoid unnecessary prose.
- Target clear, compact sections rather than long narrative blocks.

Smell to avoid: `Underspecified or Incomplete`

### 6) Examples

- Include at least one short example when ambiguity risk is non-trivial.
- Prefer minimal snippets showing call intent and result interpretation.

Smell to avoid: `Exemplar Issues`

## Repository Policy: Effective-First, Token-Aware

For this repository, we optimize for effectiveness as indicated in the paper while controlling token overhead.

- Default profile: include all six components.
- Keep examples concise (1 short example; add a second only if needed for disambiguation).
- Prefer structured bullets over long paragraphs.
- Avoid repeating information already obvious from argument names/types.

## Recommended Description Template

Use this structure for MCP tool docstrings:

1. Purpose (1-2 sentences)
2. When to use (guidelines)
3. Limitations and prerequisites
4. Args (semantic details, formats)
5. Returns (shape + how to interpret)
6. Example (short)

## Suggested Review Checklist

Before merging a new or edited tool description:

- [ ] Purpose is explicit in opening lines
- [ ] Usage guidance tells the agent when/how to use output
- [ ] Limitations/prereqs are explicit
- [ ] Every parameter has semantic guidance beyond type
- [ ] Return shape and interpretation are clear
- [ ] At least one concise example is present when ambiguity exists
- [ ] Description is concise and avoids repeated filler

## Scope

Apply these rules to:

- MCP tools registered via the `#[tool]` macro (rmcp)
- MCP prompts (adapt components where applicable)

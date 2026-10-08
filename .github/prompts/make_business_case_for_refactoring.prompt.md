---
name: make_business_case_for_refactoring
description: Run CodeScene's MCP tool for estimating the ROI when refactoring a source file.
---

Make the business case for refactoring a given file or hotspot so that its Code Health improves.
Use **CodeScene MCP Server** with **code_health_refactoring_business_case** to get a data-driven estimate of the business benefits.
By default the tool targets the next incremental Code Health level; if the user names a specific target (e.g. a perfect 10.0), pass it as `target_code_health`.

After running the tool:

1. Present the business case as a short summary that can be discussed with engineering leadership, product, or the team.
2. Explain the recommended target scenario and the optimistic versus pessimistic outcomes.
3. Treat the results as modeled projections, not guarantees.
4. If a `code_health_review` is also available, connect the business case to the structural issues it identified.

Keep the response brief and readable. Prefer one short intro paragraph followed by a compact bullet list of the projected benefits.
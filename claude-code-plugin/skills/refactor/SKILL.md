---
description: Refactor code guided by Code Health analysis. Use when the user asks to refactor, improve, clean up, or reduce complexity in code.
---

# Code Health Guided Refactoring

Follow this workflow to refactor code using Code Health as guidance:

1. Run `code_health_review` on the target file to identify specific code smells.
2. Identify complexity, size, coupling, or other issues from the findings.
3. Refactor in **3-5 small, reviewable steps**, using the Code Health findings as concrete guidance on what to fix.
4. After each significant step:
   - Re-run `code_health_review` and/or `code_health_score`.
   - Confirm measurable improvement or no regression.
5. Continue until Code Health reaches **10.0** or no further improvements are actionable.

If `code_health_auto_refactor` is available (requires CodeScene ACE), it can accelerate the first restructuring step for large functions. Use it only for functions under 300 lines with supported smells (Complex Conditional, Bumpy Road, Complex Method, Deep Nested Complexity, Large Method).

Always verify improvement with `code_health_score` after applying auto-refactored code.

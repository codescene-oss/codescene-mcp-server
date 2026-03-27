---
name: refactor_to_improve_code_health
description: Plan a prioritized, low-risk refactoring to remediate detected Code Health issues.
---

Your task is to produce a practical, developer-friendly refactoring plan based on a CodeScene Code Health Review.

Follow these steps:

1. Run `code_health_review` on the selected file or code changes to establish the structural problems that need attention.
2. Capture the current `code_health_score` if available so the plan starts from a measurable baseline.
3. Focus the plan on the functions, methods, or areas with the most severe and highest-impact structural issues.
4. Propose 3 to 5 small, low-risk refactoring steps that reduce responsibilities, nesting, coupling, or hard-to-follow logic.
5. For each step, explain what to change, why it improves maintainability, and how the improvement will be validated.
6. Use `code_health_refactoring_business_case` only when the user wants ROI or stakeholder justification.

**Deliverable format:**
- **Short summary** (1–2 sentences) describing the overall refactoring plan and its expected outcome.
- **Prioritized list of remediation tasks**. For each task, include:
  - Function/method name  
  - Detected code smells  
  - Proposed remediation action  
  - Validation step using `code_health_review` and, when available, `code_health_score`
  - 1-sentence effort–risk justification
- **Stop condition**: what would count as a meaningful improvement, and when to pause if only a partial uplift is realistic.

Guidelines:
- Keep the plan **pragmatic and low-risk**, emphasizing high-impact improvements first.
- If details are missing, make **reasonable assumptions** and briefly state them.
- Treat Code Health 10.0 as the ideal target, and aim for measurable progress toward it even when a full uplift is not realistic in one pass.
- Prefer structural refactorings over cosmetic cleanup.
- Do not treat formatting, renaming, or minor edits alone as meaningful Code Health improvement.
- Use `code_health_review` as the main feedback loop and `code_health_score` as the compact trend check.

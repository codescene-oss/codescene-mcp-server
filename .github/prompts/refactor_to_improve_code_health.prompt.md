---
description: Plan a prioritized, low-risk refactoring to remediate detected Code Health issues.
tools:
  - code_health_review
  - code_health_refactoring_business_case
---

Your task is to produce a practical, developer-friendly refactoring plan based on a CodeScene Code Health Review.

Follow these steps:

1. Run the `code_health_review` tool on the selected files or code changes to detect code smells.
2. Focus the plan exclusively on the **functions/methods with the most severe and highest-impact code smells**.
3. For each selected function/method, propose a **specific, concise remediation action**, explaining *what to change* and *why it improves readability and maintainability*.
4. Motivate each action with the **expected impact on Code Health** and its **business value** (e.g., reduced defects, faster development, lower cognitive load).
5. If the code is already healthy, then aim for an optimal Code Health of 10.0. Such code is optimized for both human and AI comprehension.
6. Include a **one-sentence justification of the effort–risk tradeoff** for every proposed action.

**ACE (auto_refactor) guidance:**
If the Code Health Review reports a large or complex function in a supported language and CodeScene ACE (auto-refactor) is available, use the `code_health_auto_refactor` tool to split the function into smaller, cohesive units as the first remediation step. ACE supports these code smells:
  - Complex Conditional
  - Bumpy Road Ahead
  - Complex Method
  - Deep, Nested Complexity
  - Large Method
Then refine the resulting units using targeted refactorings. If ACE is unavailable or the function is not supported, proceed with manual, incremental refactorings.

**Deliverable format:**
- **Short summary** (1–2 sentences) describing the overall refactoring plan and its expected outcome.
- **Prioritized list of remediation tasks**. For each task, include:
  - Function/method name  
  - Detected code smells  
  - Proposed remediation action  
  - 1-line business/Code Health motivation  
  - 1-sentence effort–risk justification

Guidelines:
- Keep the plan **pragmatic and low-risk**, emphasizing high-impact improvements first.
- If details are missing, make **reasonable assumptions** and briefly state them.
- When ACE is available, always prefer automated modularization for large/complex functions with supported code smells as an inital refactoring that you then iterate on.
- If ACE is unavailable, proceed with manual, incremental refactorings.

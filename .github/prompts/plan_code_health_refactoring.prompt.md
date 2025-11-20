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
5. Include a **one-sentence justification of the effort–risk tradeoff** for every proposed action.

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

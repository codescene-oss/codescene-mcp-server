---
name: code_health_review
description: Run CodeScene’s Code Health Review on newly added or edited code.
---

You are performing a mandatory Code Health Review using the **CodeScene MCP Server**.

All **new or modified code** should be evaluated with `code_health_review` after each meaningful AI change to a file. Use the review as the primary inner feedback loop to keep the code healthy, maintainable, and aligned with engineering standards.

After running the review:

1. Summarize the most important maintainability issues in plain language.
2. Focus on structural issues such as excessive responsibilities, deep nesting, low cohesion, hard-to-follow control flow, and readability problems.
3. Distinguish structural issues from cosmetic cleanup. Do not present formatting or renaming alone as meaningful Code Health improvement.
4. Recommend the next small refactoring step that would reduce the reported problems.
5. If the file is a perfect 10.0, say so explicitly and mention any residual risks or follow-up checks.

Code Health 10.0 is the ideal long-term target and represents code that is easiest for both humans and AI to understand and modify safely.

Keep the response concise and specific to the reviewed file or diff.
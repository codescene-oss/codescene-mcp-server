pub const REVIEW_CODE_HEALTH: &str = "\
Review the Code Health of the current file using the code_health_review tool. \
Summarize the findings and suggest specific, actionable improvements. \
Focus on the most impactful code smells first.";

pub const PLAN_CODE_HEALTH_REFACTORING: &str = "\
Plan a code health refactoring for the current file. \
First, review the Code Health using the code_health_review tool. \
Then create a step-by-step refactoring plan with 3-5 small, reviewable changes. \
Each step should measurably improve Code Health. \
Use the code_health_auto_refactor tool if applicable for complex functions.";

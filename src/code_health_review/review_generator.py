from collections.abc import Callable
from typing import TypedDict

from utils import code_health_review_properties, require_access_token, run_cs_cli, track, with_version_check


class CodeHealthReviewDeps(TypedDict):
    analyze_code_fn: Callable[[str], str]


class CodeHealthReview:
    def __init__(self, mcp_instance, deps: CodeHealthReviewDeps):
        self.deps = deps

        mcp_instance.tool(self.code_health_review)

    @require_access_token
    @with_version_check
    @track("code-health-review", code_health_review_properties)
    def code_health_review(self, file_path: str) -> str:
        """
        Review the Code Health of a single source file and return a detailed
        CLI review output that includes the score and code smell findings.

        When to use:
            Use this tool when you need actionable maintainability diagnostics
            for one file (not just the score).

        Limitations:
            - Analyzes one file at a time.
            - Requires a supported source file.
            - Returns CLI review text, not a normalized JSON schema.

        Args:
            file_path: Absolute path to the source code file to analyze.
                Use a real file path in the local repository.

        Returns:
            A review string from the CodeScene CLI. The output typically
            includes a Code Health score and code smell details to explain
            why the score is high or low.

            The Code Health scores are interpreted as:
              * Optimal Code: a Code Health 10.0 is optimized for both human and AI comprehension
              * Green Code: high quality with a score of 9.0-9.9
              * Yellow Code: problematic technical debt with a score of 4.0-8.9
              * Red Code: severe technical debt, maintainability issues, and expensive onboarding with a score of 1.0-3.9

        Example:
            Call with file_path="/repo/src/app.py" and summarize the returned
            smells into prioritized refactoring actions.
        """
        return run_cs_cli(lambda: self.deps["analyze_code_fn"](file_path))

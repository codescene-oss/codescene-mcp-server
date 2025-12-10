from typing import Callable, TypedDict
from utils import run_cs_cli, track


class CodeHealthReviewDeps(TypedDict):
    analyze_code_fn: Callable[[str], str]


class CodeHealthReview:
    def __init__(self, mcp_instance, deps: CodeHealthReviewDeps):
        self.deps = deps

        mcp_instance.tool(self.code_health_review)

    @track("code-health-review")
    def code_health_review(self, file_path: str) -> str:
        """
        Calculates the code quality of the given file using the Code Health metric.
        Returns a score from 10.0 (optimal) down to 1.0 (worst).
        
        The Code Health scores are interpreted as:
          * Optimal Code: a Code Health 10.0 is optimized for both human and AI comprehension
          * Green Code: high quality with a score of 9-9.9
          * Yellow Code: problematic techncial debt with a score of 4-8.9
          * Red Code: severe techncial debt, maintainability issues, and expensive onboarding with a score of 1.0-3.9
        Args:
            file_path: The absolute path to the source code file to be analyzed.
        Returns:
            A string representing the Code Health score, 10.0->1.0
        """
        return run_cs_cli(lambda: self.deps["analyze_code_fn"](file_path))

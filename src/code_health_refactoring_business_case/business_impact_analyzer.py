from typing import Callable, TypedDict

from .business_case import make_business_case_for
from utils import code_health_from_cli_output, with_version_check


class CodeHealthRefactoringBusinessCaseDeps(TypedDict):
    analyze_code_fn: Callable[[str], str]


class CodeHealthRefactoringBusinessCase:
    def __init__(self, mcp_instance, deps: CodeHealthRefactoringBusinessCaseDeps):
        self.deps = deps

        mcp_instance.tool(self.code_health_refactoring_business_case)

    @with_version_check
    def code_health_refactoring_business_case(self, file_path: str) -> dict:
        """
        Generate a data-driven business case for refactoring a source file.

        This tool analyzes the file's current Code Health and estimates the
        business impact of improving it. The result includes quantified
        predictions for development speed and defect reduction based on
        CodeScene's empirical research.

        Args:
            file_path: Absolute path to the source code file to analyze.

        Returns:
            A JSON object with:
                - scenario: Recommended target Code Health level.
                - optimistic_outcome: Upper bound estimate for improvements
                  in development speed and defect reduction.
                - pessimistic_outcome: Lower bound estimate for improvements.
                - confidence_interval: The optimistic → pessimistic range,
                  representing a 90% confidence interval for the expected impact.
        """
        current_code_health = code_health_from_cli_output(self.deps["analyze_code_fn"](file_path))

        return make_business_case_for(current_code_health)

import json
from collections.abc import Callable
from typing import TypedDict

from utils import business_case_properties, code_health_from_cli_output, require_access_token, track, with_version_check

from .business_case import make_business_case_for


class CodeHealthRefactoringBusinessCaseDeps(TypedDict):
    analyze_code_fn: Callable[[str], str]


class CodeHealthRefactoringBusinessCase:
    def __init__(self, mcp_instance, deps: CodeHealthRefactoringBusinessCaseDeps):
        self.deps = deps

        mcp_instance.tool(self.code_health_refactoring_business_case)

    @require_access_token
    @with_version_check
    @track("code-health-refactoring-business-case", business_case_properties)
    def code_health_refactoring_business_case(self, file_path: str) -> str:
        """
        Generate a data-driven business case for refactoring a source file.

        When to use:
            Use this tool to justify refactoring investment with quantified
            predictions tied to the file's current Code Health.

        Limitations:
            - Estimates are model-based projections, not guarantees.
            - Evaluates one file at a time.
            - Requires an analyzable source file.

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

        Example:
            Call with file_path="/repo/src/service.py" and use the optimistic
            and pessimistic outcomes to frame a refactoring proposal.
        """
        current_code_health = code_health_from_cli_output(self.deps["analyze_code_fn"](file_path))

        return json.dumps(make_business_case_for(current_code_health))

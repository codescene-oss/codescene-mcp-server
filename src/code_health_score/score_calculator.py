from collections.abc import Callable
from typing import TypedDict

from utils import code_health_from_cli_output, require_access_token, run_cs_cli, track, with_version_check


class CodeHealthScoreDeps(TypedDict):
    analyze_code_fn: Callable[[str], str]


class CodeHealthScore:
    def __init__(self, mcp_instance, deps: CodeHealthScoreDeps):
        self.deps = deps

        mcp_instance.tool(self.code_health_score)

    def _calculate_code_health_score_for(self, file_path: str) -> str:
        def calculate_code_health_of() -> float:
            result = self.deps["analyze_code_fn"](file_path)
            return code_health_from_cli_output(result)

        return run_cs_cli(lambda: calculate_code_health_of())

    @require_access_token
    @with_version_check
    @track("code-health-score")
    def code_health_score(self, file_path: str) -> str:
        """
        Calculate the Code Health score for a single source file.
        The tool returns one numeric score from 10.0 (optimal) to 1.0 (worst).

        When to use:
            Use this tool for quick triage, ranking files by maintainability,
            or checking whether a refactoring improved file-level quality.

        Limitations:
            - Analyzes one file at a time.
            - Returns only the score summary, not the full smell breakdown.
            - Requires a supported source file.

        The Code Health scores are interpreted as:
          * Optimal Code: Code Health 10.0 optimized for human and AI comprehension
          * Green Code: high quality with a score of 9.0-9.9
          * Yellow Code: problematic technical debt with a score of 4.0-8.9
          * Red Code: severe technical debt with a score of 1.0-3.9

        Args:
            file_path: Absolute path to the source code file to analyze.
                Use a concrete local file path.

        Returns:
            A string in the format "Code Health score: <score>".

        Example:
            Call with file_path="/repo/src/module.py" and compare the score
            before and after a refactoring.
        """
        return f"Code Health score: {self._calculate_code_health_score_for(file_path)}"

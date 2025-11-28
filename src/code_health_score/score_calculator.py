from typing import Callable, TypedDict
from utils import run_cs_cli, code_health_from_cli_output


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

    def code_health_score(self, file_path: str) -> str:
        """
        Calculates the code quality of the given file using the Code Health metric.
        Returns a score from 10.0 (best) down to 1.0 (worst).
        Args:
            file_path: The absolute path to the source code file to be analyzed.
        Returns:
            A string representing the Code Health score, 10.0->1.0
        """
        return f"Code Health score: {self._calculate_code_health_score_for(file_path)}"

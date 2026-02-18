from collections.abc import Callable
from typing import TypedDict

from code_health_tools.delta_runner import run_delta_cli
from utils import (
    cs_cli_path,
    get_platform_details,
    track,
    with_version_check,
)


class AnalyzeChangeSetDeps(TypedDict):
    run_local_tool_fn: Callable[[list, str | None, dict | None], str]


class AnalyzeChangeSet:
    def __init__(self, mcp_instance, deps: AnalyzeChangeSetDeps):
        self.deps = deps

        mcp_instance.tool(self.analyze_change_set)

    @track("analyze-change-set")
    @with_version_check
    def analyze_change_set(self, base_ref: str, git_repository_path: str) -> str:
        """
        Provides a branch-level Code Health review of all changes between the
        current HEAD and the given base_ref. This is the equivalent of a local
        PR pre-flight check: it analyzes every file that differs from the base
        reference and reports Code Health improvements, degradations, and code
        smells across the entire change set.

        Use this tool before opening a pull request to catch Code Health
        regressions early, while changes are still local.

        Args:
            base_ref: The git reference to compare against, typically the target
                branch of the pull request (e.g. "main", "origin/main", "develop").
            git_repository_path: The absolute path to the Git repository for the
                current code base.

        Returns:
            A JSON object containing:
             - quality_gates: the central outcome, summarizing whether the change
               set passes or fails Code Health thresholds ("passed" or "failed").
             - results: an array of objects for each affected file with:
                 - name: the name of the file whose Code Health is impacted.
                 - verdict: "improved", "degraded", or "stable".
                 - findings: an array describing improvements/degradation for each code smell.
        """
        cli_command = [
            cs_cli_path(get_platform_details()),
            "delta",
            base_ref,
            "--output-format=json",
        ]

        return run_delta_cli(cli_command, git_repository_path, self.deps["run_local_tool_fn"])

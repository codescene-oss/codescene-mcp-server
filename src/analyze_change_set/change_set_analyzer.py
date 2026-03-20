from collections.abc import Callable
from typing import TypedDict

from code_health_tools.delta_runner import run_delta_cli
from utils import (
    analyze_change_set_properties,
    cs_cli_path,
    get_platform_details,
    require_access_token,
    track,
    with_version_check,
)


class AnalyzeChangeSetDeps(TypedDict):
    run_local_tool_fn: Callable[[list, str | None, dict | None], str]


class AnalyzeChangeSet:
    def __init__(self, mcp_instance, deps: AnalyzeChangeSetDeps):
        self.deps = deps

        mcp_instance.tool(self.analyze_change_set)

    @require_access_token
    @track("analyze-change-set", analyze_change_set_properties)
    @with_version_check
    def analyze_change_set(self, base_ref: str, git_repository_path: str) -> str:
        """
        Run a branch-level Code Health review for all files that differ between
        current HEAD and base_ref.

        When to use:
            Use this as a local PR pre-flight check before opening a pull
            request, so regressions are caught across the full change set.

        Limitations:
            - Requires a valid git repository path.
            - base_ref must exist and be resolvable by git in that repository.
            - Reviews only files that differ from base_ref.
            - Output is JSON text from the CLI command.

        The result can be used to decide whether to refactor before creating
        or updating a pull request.

        Args:
            base_ref: Git reference to compare against, typically the target
                branch of the pull request (for example "main" or "origin/main").
            git_repository_path: Absolute path to the local git repository.

        Returns:
            A JSON object containing:
              - quality_gates: the central outcome, summarizing whether the change
                set passes or fails Code Health thresholds ("passed" or "failed").
              - results: an array of objects for each affected file with:
                  - name: the name of the file whose Code Health is impacted.
                  - verdict: "improved", "degraded", or "stable".
                  - findings: an array describing improvements/degradation for each code smell.

        Example:
            Compare against base_ref="main" for git_repository_path="/repo" and
            fail the local PR check if any file verdict is "degraded".
        """
        cli_command = [
            cs_cli_path(get_platform_details()),
            "delta",
            base_ref,
            "--output-format=json",
        ]

        return run_delta_cli(cli_command, git_repository_path, self.deps["run_local_tool_fn"])

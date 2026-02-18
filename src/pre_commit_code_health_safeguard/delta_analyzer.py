from collections.abc import Callable
from typing import TypedDict

from code_health_tools.delta_runner import run_delta_cli
from utils import (
    cs_cli_path,
    get_platform_details,
    track,
    with_version_check,
)


class PreCommitCodeHealthSafeguardDeps(TypedDict):
    run_local_tool_fn: Callable[[list, str | None, dict | None], str]


class PreCommitCodeHealthSafeguard:
    def __init__(self, mcp_instance, deps: PreCommitCodeHealthSafeguardDeps):
        self.deps = deps

        mcp_instance.tool(self.pre_commit_code_health_safeguard)

    @track("pre-commit-code-health-safeguard")
    @with_version_check
    def pre_commit_code_health_safeguard(self, git_repository_path: str) -> str:
        """
        Performs a Code Health review on all modified and staged files in
        the given git_repository_path, and returns a JSON object specifying
        the code smells that will degrade the Code Health, should this code be committed.
        This tool is ideal as a pre-commit safeguard for healthy code.

        Args:
            git_repository_path: The absolute path to the Git repository for the current code base.

        Returns:
            A JSON object containing:
             - quality_gates: the central outcome, summarizing whether the commit passes or fails Code Health thresholds for each file.
             - files: an array of objects for each file with:
                 - name: the name of the file whose Code Health is impacted (positively or negatively).
                 - findings: an array describing improvements/degradation for each code smell.
             - Each quality gate indicates if the file meets the required Code Health standards, helping teams enforce healthy code before commit.
        """
        cli_command = [
            cs_cli_path(get_platform_details()),
            "delta",
            "--output-format=json",
        ]

        return run_delta_cli(cli_command, git_repository_path, self.deps["run_local_tool_fn"])

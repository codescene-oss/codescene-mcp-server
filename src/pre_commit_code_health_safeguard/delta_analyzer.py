from collections.abc import Callable
from typing import TypedDict

from code_health_tools.delta_runner import run_delta_cli
from utils import (
    cs_cli_path,
    get_platform_details,
    require_access_token,
    track,
    with_version_check,
)


class PreCommitCodeHealthSafeguardDeps(TypedDict):
    run_local_tool_fn: Callable[[list, str | None, dict | None], str]


class PreCommitCodeHealthSafeguard:
    def __init__(self, mcp_instance, deps: PreCommitCodeHealthSafeguardDeps):
        self.deps = deps

        mcp_instance.tool(self.pre_commit_code_health_safeguard)

    @require_access_token
    @track("pre-commit-code-health-safeguard")
    @with_version_check
    def pre_commit_code_health_safeguard(self, git_repository_path: str) -> str:
        """
        Review all modified and staged files in a repository and report
        Code Health degradations before commit.

        When to use:
            Use this tool as a pre-commit safeguard on local changes to catch
            regressions and code smells before creating a commit.

        Limitations:
            - Requires a valid git repository path.
            - Evaluates current local modifications/staged changes only.
            - Output is JSON text from the CLI command.

        Args:
            git_repository_path: Absolute path to the local git repository to analyze.

        Returns:
            A JSON object containing:
              - quality_gates: the central outcome, summarizing whether the commit passes or fails Code Health thresholds for each file.
              - files: an array of objects for each file with:
                  - name: the name of the file whose Code Health is impacted (positively or negatively).
                  - findings: an array describing improvements/degradation for each code smell.
              - Each quality gate indicates if the file meets the required Code Health standards, helping teams enforce healthy code before commit.

        Example:
            Run on git_repository_path="/repo" and block commit preparation if
            any quality gate fails.
        """
        cli_command = [
            cs_cli_path(get_platform_details()),
            "delta",
            "--output-format=json",
        ]

        return run_delta_cli(cli_command, git_repository_path, self.deps["run_local_tool_fn"])

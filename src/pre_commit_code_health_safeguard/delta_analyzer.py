import json
from typing import TypedDict, Callable, Optional

from code_health_tools.delta_analysis import analyze_delta_output
from utils import cs_cli_path, adapt_mounted_file_path_inside_docker, run_cs_cli, track, with_version_check, get_platform_details


class PreCommitCodeHealthSafeguardDeps(TypedDict):
    run_local_tool_fn: Callable[[list, Optional[str]], str]


class PreCommitCodeHealthSafeguard:
    def __init__(self, mcp_instance, deps: PreCommitCodeHealthSafeguardDeps):
        self.deps = deps

        mcp_instance.tool(self.pre_commit_code_health_safeguard)

    def _safeguard_code_on(self, cli_command: list, git_repository_path: str) -> str:
        docker_path = adapt_mounted_file_path_inside_docker(git_repository_path)
        self.deps["run_local_tool_fn"](["git", "config", "--system", "--add", "safe.directory", docker_path], None)
        output = self.deps["run_local_tool_fn"](cli_command, docker_path)
        return json.dumps(analyze_delta_output(output))

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
        cli_command = [cs_cli_path(get_platform_details()), "delta", "--output-format=json"]

        return run_cs_cli(lambda: self._safeguard_code_on(cli_command, git_repository_path))

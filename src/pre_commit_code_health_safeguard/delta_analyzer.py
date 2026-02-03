import json
import os
from typing import TypedDict, Callable, Optional

from code_health_tools.delta_analysis import analyze_delta_output
from utils import cs_cli_path, adapt_mounted_file_path_inside_docker, adapt_worktree_gitdir_for_docker, run_cs_cli, track, with_version_check, get_platform_details
from utils.docker_path_adapter import get_worktree_gitdir

class PreCommitCodeHealthSafeguardDeps(TypedDict):
    run_local_tool_fn: Callable[[list, Optional[str], Optional[dict]], str]


class PreCommitCodeHealthSafeguard:
    def __init__(self, mcp_instance, deps: PreCommitCodeHealthSafeguardDeps):
        self.deps = deps

        mcp_instance.tool(self.pre_commit_code_health_safeguard)

    def _safeguard_code_on_docker(self, cli_command: list, git_repository_path: str) -> str:
        """Handle pre-commit safeguard in Docker environment with path translation."""
        docker_path = adapt_mounted_file_path_inside_docker(git_repository_path)
        
        # Detect if this is a git worktree and get the translated gitdir path
        worktree_gitdir = adapt_worktree_gitdir_for_docker(docker_path)
        extra_env = {"GIT_DIR": worktree_gitdir} if worktree_gitdir else None
        
        self.deps["run_local_tool_fn"](["git", "config", "--system", "--add", "safe.directory", docker_path], None, extra_env)
        output = self.deps["run_local_tool_fn"](cli_command, docker_path, extra_env)
        return json.dumps(analyze_delta_output(output))

    def _safeguard_code_on_local(self, cli_command: list, git_repository_path: str) -> str:
        """Handle pre-commit safeguard in local/native environment."""        
        # Detect worktree and set GIT_DIR if needed (mirrors Docker mode logic)
        gitdir = get_worktree_gitdir(git_repository_path)
        extra_env = {"GIT_DIR": gitdir} if gitdir else None
        
        output = self.deps["run_local_tool_fn"](cli_command, git_repository_path, extra_env)
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

        if os.getenv("CS_MOUNT_PATH"):
            # Docker environment - needs path translation and worktree handling
            return run_cs_cli(lambda: self._safeguard_code_on_docker(cli_command, git_repository_path))
        else:
            # Local/native binary - use paths directly
            return run_cs_cli(lambda: self._safeguard_code_on_local(cli_command, git_repository_path))

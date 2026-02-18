import json
import os
from collections.abc import Callable

from code_health_tools.delta_analysis import analyze_delta_output
from utils import (
    adapt_mounted_file_path_inside_docker,
    adapt_worktree_gitdir_for_docker,
    run_cs_cli,
)
from utils.docker_path_adapter import get_worktree_gitdir


def run_delta_cli(
    cli_command: list,
    git_repository_path: str,
    run_local_tool_fn: Callable[[list, str | None, dict | None], str],
) -> str:
    """
    Run a CodeScene delta CLI command and return parsed results as JSON.

    Handles the Docker-vs-local dispatch, worktree detection, and output
    parsing that is common to all delta-based tools.

    Args:
        cli_command: The full CLI command list (e.g. ["cs", "delta", "--output-format=json"]).
        git_repository_path: Absolute path to the Git repository.
        run_local_tool_fn: Callable that executes a CLI command with optional cwd and env.

    Returns:
        A JSON string with the parsed delta results.
    """
    if os.getenv("CS_MOUNT_PATH"):
        return run_cs_cli(lambda: _run_on_docker(cli_command, git_repository_path, run_local_tool_fn))
    else:
        return run_cs_cli(lambda: _run_on_local(cli_command, git_repository_path, run_local_tool_fn))


def _run_on_docker(
    cli_command: list,
    git_repository_path: str,
    run_local_tool_fn: Callable,
) -> str:
    """Run a delta CLI command in a Docker environment with path translation."""
    docker_path = adapt_mounted_file_path_inside_docker(git_repository_path)

    worktree_gitdir = adapt_worktree_gitdir_for_docker(docker_path)
    extra_env = {"GIT_DIR": worktree_gitdir} if worktree_gitdir else None

    run_local_tool_fn(
        ["git", "config", "--system", "--add", "safe.directory", docker_path],
        None,
        extra_env,
    )
    output = run_local_tool_fn(cli_command, docker_path, extra_env)
    return json.dumps(analyze_delta_output(output))


def _run_on_local(
    cli_command: list,
    git_repository_path: str,
    run_local_tool_fn: Callable,
) -> str:
    """Run a delta CLI command in a local/native environment."""
    gitdir = get_worktree_gitdir(git_repository_path)
    extra_env = {"GIT_DIR": gitdir} if gitdir else None

    output = run_local_tool_fn(cli_command, git_repository_path, extra_env)
    return json.dumps(analyze_delta_output(output))

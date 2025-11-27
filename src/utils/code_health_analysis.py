import json
import os
import subprocess
from errors import CodeSceneCliError
from .docker_path_adapter import adapt_mounted_file_path_inside_docker


def run_local_tool(command: list, cwd: str = None):
    """
    Runs a local command-line tool and captures its output.

    Args:
        command (list): The command and its arguments, e.g. ['ls', '-l']
        cwd (str): Optional working directory to run the command in

    Returns:
        str: Combined stdout and stderr output
    """
    env = {
        'CS_CONTEXT': 'mcp-server',
        'CS_ACCESS_TOKEN': os.getenv("CS_ACCESS_TOKEN", "")
    }

    if os.getenv("CS_ONPREM_URL"):
        env['CS_ONPREM_URL'] = os.getenv("CS_ONPREM_URL")

    result = subprocess.run(command, capture_output=True, text=True, cwd=cwd, env=env)
    if result.returncode != 0:
        raise CodeSceneCliError(f"CLI command failed: {result.stderr}")
    return result.stdout


def run_cs_cli(cli_fn) -> str:
    """
    Encapsulates the general pattern of invoking the CLI tool and
    propagating potential errors.
    """
    try:
        return cli_fn()
    except FileNotFoundError:
        return "Error: The CodeScene CLI tool, cs, isn't properly installed. See https://codescene.io/docs/cli/index.html for instructions."
    except subprocess.CalledProcessError as e:
        return f"Error: {e.stderr}"
    except Exception as e:
        return f"Error: {e}"


def code_health_from_cli_output(cli_output) -> float:
    r = json.loads(cli_output)

    if 'score' not in r:
        raise CodeSceneCliError("CLI output does not contain a 'score' field: {}".format(cli_output))

    return r['score']


def cs_cli_path():
    cs_cli_location_in_docker = '/root/.local/bin/cs'
    return os.getenv("CS_CLI_PATH", default=cs_cli_location_in_docker)


def make_cs_cli_review_command_for(cli_command: str, file_path: str):
    cs_cli = cs_cli_path()

    mount_file_path = adapt_mounted_file_path_inside_docker(file_path)

    return [cs_cli, cli_command, mount_file_path, "--output-format=json"]


def cs_cli_review_command_for(file_path: str):
    return make_cs_cli_review_command_for("review", file_path)


def analyze_code(file_path: str) -> str:
    return run_local_tool(cs_cli_review_command_for(file_path))

import json
import os
from pathlib import Path
import subprocess
import sys
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
    bundle_dir = Path(__file__).parent.parent.absolute()

    # Check for platform-specific binary name
    if sys.platform == "win32":
        internal_cs_path = bundle_dir / "cs.exe"
    else:
        internal_cs_path = bundle_dir / "cs"

    if internal_cs_path.exists():
        if not os.access(internal_cs_path, os.X_OK):
            os.chmod(internal_cs_path, 0o755)
        return str(internal_cs_path)

    if os.getenv("CS_CLI_PATH"):
        return os.getenv("CS_CLI_PATH")

    return '/root/.local/bin/cs'


def make_cs_cli_review_command_for(cli_command: str, file_path: str):
    cs_cli = cs_cli_path()

    if os.getenv("CS_MOUNT_PATH"):
        mount_file_path = adapt_mounted_file_path_inside_docker(file_path)
    else:
        mount_file_path = file_path

    return [cs_cli, cli_command, mount_file_path, "--output-format=json"]


def cs_cli_review_command_for(file_path: str):
    return make_cs_cli_review_command_for("review", file_path)


def analyze_code(file_path: str) -> str:
    return run_local_tool(cs_cli_review_command_for(file_path))

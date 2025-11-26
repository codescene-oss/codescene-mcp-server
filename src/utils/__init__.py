import json
import os
import subprocess
from pathlib import Path
import requests
from errors import CodeSceneCliError


def adapt_mounted_file_path_inside_docker(file_path: str) -> str:
    """
    Convert a host-mounted absolute file path into the path the container sees.

    - Requires the environment variable `CS_MOUNT_PATH` to be set.
    - `file_path` must be absolute and located under `CS_MOUNT_PATH`.
    - Returns a POSIX-style path rooted at '/mount' (e.g. '/mount/src/foo.py').
    """
    mount = os.getenv("CS_MOUNT_PATH")
    if not mount:
        raise CodeSceneCliError("CS_MOUNT_PATH not defined.")

    p = Path(file_path)
    if not p.is_absolute():
        raise CodeSceneCliError(f"file_path must be absolute: {file_path!r}")

    def relative_path_under_mount(file_path: Path, mount_path: Path) -> Path:
        try:
            return file_path.relative_to(mount_path)
        except ValueError:
            raise CodeSceneCliError(f"file_path is not under CS_MOUNT_PATH: {str(file_path)!r}")

    relative = relative_path_under_mount(p, Path(mount))

    # If the file points to the mount root, relative_to yields '.'
    if relative == Path("."):
        return "/mount"

    mount_posix_style = "/mount/" + relative.as_posix()
    return mount_posix_style

def get_api_url() -> str:
    url = os.getenv("CS_ONPREM_URL")
    return f"{url}/api" if url else "https://api.codescene.io"

def get_api_request_headers() -> dict:
    if os.getenv("CS_ACCESS_TOKEN") is None:
        return {}

    return {
        'Authorization': f"Bearer {os.getenv('CS_ACCESS_TOKEN')}"
    }

def query_api(endpoint, params: dict) -> dict:
    url = f"{get_api_url()}/{endpoint}"
    response = requests.get(url, params=params, headers=get_api_request_headers())

    return response.json()

def query_api_list(endpoint, params: dict, key: str) -> list:
    data = query_api(endpoint, params)
    items = data.get(key, [])

    if data.get('max_pages') == 0 or data.get('max_pages') is None:
        return items

    if data.get('max_pages') > data.get('page', 1):
        params['page'] = data.get('page', 1) + 1
        items.extend(query_api_list(endpoint, params, key))

    return items

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
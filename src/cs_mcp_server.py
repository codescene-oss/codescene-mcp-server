import subprocess
from fastmcp import FastMCP
import json
import tempfile
import os

mcp = FastMCP("CodeScene")

class CodeSceneCliError(Exception):
    """Raised when the CLI tool fails to calculate Code Health for a given file.
    """
    pass

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

def code_health_from(cli_output) -> float:
    r = json.loads(cli_output)

    if 'score' not in r:
        raise CodeSceneCliError("CLI output does not contain a 'score' field: {}".format(cli_output))

    return r['score']

def adapt_mounted_file_path_inside_docker(file_path):
    if not os.getenv("CS_MOUNT_PATH"):
        raise CodeSceneCliError("CS_MOUNT_PATH not defined.")

    mount_dir = os.getenv('CS_MOUNT_PATH').removesuffix('/')
    mount_file_path = file_path.replace(mount_dir, '/mount')

    return mount_file_path

def context_aware_path_to(file_path: str):
    """
    The MCP server executes in two contexts: docker (default distro for the MCP), and 
    as an executable Python file used during our e2e tests. (In the future, we do 
    want the e2e tests to go via the Docker distro).
    When running tests, we don't have a mount path -> shortcut that via the env.
    """
    if os.getenv("CS_MCP_RUNS_TEST_CONTEXT"):
        return file_path
    
    return adapt_mounted_file_path_inside_docker(file_path)

def cs_cli_review_command_for(file_path: str):
    cs_cli_location_in_docker = '/root/.local/bin/cs'
    cs_cli = os.getenv("CS_CLI_PATH", default=cs_cli_location_in_docker)

    mount_file_path = context_aware_path_to(file_path)

    return [cs_cli, "review", mount_file_path, "--output-format=json"]

def analyze_code(file_path: str) -> str:
    return run_local_tool(cs_cli_review_command_for(file_path))

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

@mcp.tool()
def code_health_score(file_path: str) -> str:
    """
    Calculates the code quality of the given file using the Code Health metric.
    Returns a score from 10.0 (best) down to 1.0 (worst).
    Args:
        file_path: The absolute path to the source code file to be analyzed.
    Returns:
        A string representing the Code Health score, 10.0->1.0
    """
    def calculate_code_health_of(file_path: str) -> float:
        result = analyze_code(file_path)
        return code_health_from(result)
    
    return run_cs_cli(lambda: f"Code Health score: {calculate_code_health_of(file_path)}")

@mcp.tool()
def code_health_review(file_path: str) -> str:
    """
    Performs a Code Health review of the given file_path and returns 
    a JSON object specifying all potential code smells that contribute 
    to a lower Code Health.
    Args:
        file_content: The content of the source code file to be analyzed as a base64 encoded string.
        file_ext: The file extension of the source code file to be reviewed (e.g. .py, .java).
    Returns:
        A JSON object containing score and review:
         - score: this is the Code Health score. 10.0 is best, 1.0 is worst health.
         - review: an array of category and description for each code smell.
    """
    def review_code_health_of(file_path: str) -> float:
        return analyze_code(file_path)

    return run_cs_cli(lambda: review_code_health_of(file_path))

if __name__ == "__main__":
    mcp.run()
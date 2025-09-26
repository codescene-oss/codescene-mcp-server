import subprocess
from fastmcp import FastMCP
import json

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
    result = subprocess.run(command, capture_output=True, text=True, cwd=cwd)
    if result.returncode != 0:
        raise CodeSceneCliError(f"CLI command failed: {result.stderr}")
    return result.stdout

def code_health_from(cli_output) -> float:
    r = json.loads(cli_output)
    return r['score']

def cs_cli_review_command_for(local_file):
    cs_cli = 'cs' # needs to be installed locally
    return [cs_cli, "review", local_file, "--output-format=json"]

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
        file_path: The absolute path to the source code file to be reviewed.
    Returns:
        A string representing the Code Health score, 10.0->1.0
    """
    def calculate_code_health_of(local_file: str) -> float:
        result = run_local_tool(cs_cli_review_command_for(local_file))
        return code_health_from(result)
    
    return run_cs_cli(lambda: f"Code Health score: {calculate_code_health_of(file_path)}")

@mcp.tool()
def code_health_review(file_path: str) -> str:
    """
    Performs a Code Health review of the given file_path and returns 
    a JSON object specifying all potential code smells that contribute 
    to a lower Code Health.
    Args:
        file_path: The absolute path to the source code file to be reviewed.
    Returns:
        A JSON object containing score and review:
         - score: this is the Code Health score. 10.0 is best, 1.0 is worst health.
         - review: an array of category and description for each code smell.
    """
    def review_code_health_of(local_file: str) -> float:
        return run_local_tool(cs_cli_review_command_for(local_file))
    
    return run_cs_cli(lambda: f"Code Health score: {review_code_health_of(file_path)}")
    
if __name__ == "__main__":
    mcp.run()
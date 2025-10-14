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
    result = subprocess.run(command, capture_output=True, text=True, cwd=cwd)
    if result.returncode != 0:
        raise CodeSceneCliError(f"CLI command failed: {result.stderr}")
    return result.stdout

def code_health_from(cli_output) -> float:
    r = json.loads(cli_output)
    return r['score']

def analyze_code(file_content: str, file_type: str) -> str:
    local_file = None

    try:
        with tempfile.NamedTemporaryFile(mode='w', delete=False, suffix=file_type, encoding='utf-8') as tmp:
            tmp.write(file_content)
            local_file = tmp.name

        return run_local_tool(cs_cli_review_command_for(local_file))
    
    finally:
        if local_file and os.path.exists(local_file):
            os.remove(local_file)

def cs_cli_review_command_for(local_file: str):
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
    
def file_type_from(file_path: str) -> str:
    _, file_extension = os.path.splitext(file_path)

    return file_extension

@mcp.tool()
def code_health_score(opts: dict) -> str:
    """
    Calculates the code quality of the given file using the Code Health metric.
    Returns a score from 10.0 (best) down to 1.0 (worst).
    Args:
        opts: A dictionary containing "file_content" and "file_path".
            - file_content: The content of the source code file to be analyzed.
            - file_path: The absolute path to the source code file to be reviewed.
    Returns:
        A string representing the Code Health score, 10.0->1.0
    """
    file_content = opts.get("file_content")
    file_path = opts.get("file_path")

    if not file_content or not file_path:
        return "Error: Missing required arguments 'file_content' and 'file_path'."
    
    file_type = file_type_from(file_path)
    
    def calculate_code_health_of(file_content: str, file_type: str) -> float:
        result = analyze_code(file_content, file_type)
        return code_health_from(result)
    
    return run_cs_cli(lambda: f"Code Health score: {calculate_code_health_of(file_content, file_type)}")

@mcp.tool()
def code_health_review(opts: dict) -> str:
    """
    Performs a Code Health review of the given file_path and returns 
    a JSON object specifying all potential code smells that contribute 
    to a lower Code Health.
    Args:
        opts: A dictionary containing "file_content" and "file_path".
            - file_content: The content of the source code file to be analyzed.
            - file_path: The absolute path to the source code file to be reviewed.
    Returns:
        A JSON object containing score and review:
         - score: this is the Code Health score. 10.0 is best, 1.0 is worst health.
         - review: an array of category and description for each code smell.
    """
    file_content = opts.get("file_content")
    file_path = opts.get("file_path")

    if not file_content or not file_path:
        return "Error: Missing required arguments 'file_content' and 'file_path'."
    
    file_type = file_type_from(file_path)
    
    def review_code_health_of(file_content: str, file_type: str) -> float:
        return analyze_code(file_content, file_type)
    
    return run_cs_cli(lambda: review_code_health_of(file_content, file_type))
    
if __name__ == "__main__":
    mcp.run()
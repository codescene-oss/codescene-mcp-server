import subprocess
from fastmcp import FastMCP
from fastmcp.resources import FileResource
from pathlib import Path
import json
import os
from code_health_tools.business_case import make_business_case_for

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

def _calculate_code_health_score_for(file_path: str) -> str:
    def calculate_code_health_of(file_path: str) -> float:
        result = analyze_code(file_path)
        return code_health_from(result)
    
    return run_cs_cli(lambda: calculate_code_health_of(file_path))

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
    return f"Code Health score: {_calculate_code_health_score_for(file_path)}"

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

@mcp.tool()
def code_health_refactoring_business_case(file_path: str) -> dict:
    """
    Generate a data-driven business case for refactoring a source file.

    This tool analyzes the file's current Code Health and estimates the
    business impact of improving it. The result includes quantified
    predictions for development speed and defect reduction based on
    CodeScene's empirical research.

    Args:
        file_path: Absolute path to the source code file to analyze.

    Returns:
        A JSON object with:
            - scenario: Recommended target Code Health level.
            - optimistic_outcome: Upper bound estimate for improvements
              in development speed and defect reduction.
            - pessimistic_outcome: Lower bound estimate for improvements.
            - confidence_interval: The optimistic → pessimistic range,
              representing a 90% confidence interval for the expected impact.
    """
    current_code_health = _calculate_code_health_score_for(file_path)
    return make_business_case_for(current_code_health)

# We want the MCP Server to explain its key concepts like Code Health.

def read_documentation_content_for(md_doc_name):
    return Path(f"./src/docs/code-health/{md_doc_name}").read_text(encoding="utf-8")

@mcp.tool()
def explain_how_code_health_works(context: str | None = None) -> str:
    """
    Explains CodeScene's Code Health metric for assessing code quality and maintainability for both human devs and AI.
    """
    return read_documentation_content_for('how-it-works.md')

@mcp.tool()
def explain_how_code_health_is_relevant_for_productivity_and_business(context: str | None = None) -> str:
    """
    Describes how to build a business case for Code Health improvements. 
    Covers empirical data on how healthy code lets you ship faster with 
    fewer defects.
    """
    return read_documentation_content_for('business-case.md')

def resource_title_from_md_heading_in(path: Path) -> str:
    """
    Return the first line of a markdown file, stripped of leading '#' and whitespace.
    We use that initial line as the MCP Resource name.
    """
    with path.open(encoding="utf-8") as f:
        first_line = f.readline()
        return first_line.lstrip("#").strip()

def doc_to_file_resources(doc):
    doc_path = Path(f"./src/docs/code-health/{doc['doc-path']}").resolve()
    doc_resource = FileResource(
        uri=f"file://codescene-docs/code-health/{doc['doc-path']}",
        path=doc_path,
        name=resource_title_from_md_heading_in(doc_path),
        description=doc['description'],
        mime_type="text/markdown",
        tags={"documentation"}
        )
    return doc_resource

def add_as_mcp_resources(docs_to_expose):
    """
    Expose our static docs as MCP resources.
    Use a table-driven approach for the implementation so that it is 
    simple to add more docs. (We expect this list to grow).
    """
    for doc in docs_to_expose:
        doc_resource = doc_to_file_resources(doc)
        mcp.add_resource(doc_resource)

def all_doc_resources_as_uris(docs_to_expose):
    """
    Resources tend to be passive; they're only referenced via an URI. 
    Some clients might call resources/list, but not all -> introduce a 
    tool that helps the client discover our documentation resources.
    """
    def to_uri(doc):
        return f"file://codescene-docs/code-health/{doc['doc-path']}"
    
    return [to_uri(doc) for doc in docs_to_expose]

if __name__ == "__main__":
    docs_to_expose = [
        {'doc-path': "how-it-works.md",
         'description': "Explains CodeScene's Code Health metric for assessing code quality and maintainability for both human devs and AI."},
        {'doc-path': "business-case.md",
         'description': "Describes how to build a business case for Code Health improvements."}
    ]
    add_as_mcp_resources(docs_to_expose)
    mcp.run()
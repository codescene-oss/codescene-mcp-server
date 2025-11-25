import subprocess
from fastmcp import FastMCP
from fastmcp.resources import FileResource
import requests
from pathlib import Path
import json
import os
from code_health_tools.business_case import make_business_case_for
from code_health_tools.delta_analysis import analyze_delta_output, DeltaAnalysisError

mcp = FastMCP("CodeScene")

class CodeSceneCliError(Exception):
    """Raised when the CLI tool fails to calculate Code Health for a given file.
    """
    pass

def get_api_url() -> str:
    url = os.getenv("CS_ONPREM_URL")
    return f"{url}/api" if url else "https://api.codescene.io"

def get_api_request_headers() -> dict:
    return {
        'Authorization': f"Bearer {os.getenv('CS_ACCESS_TOKEN')}"
    }

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
        file_path: The absolute path to the source code file to be analyzed.
    Returns:
        A JSON object containing score and review:
         - score: this is the Code Health score. 10.0 is best, 1.0 is worst health.
         - review: an array of category and description for each code smell.
    """
    def review_code_health_of(file_path: str) -> float:
        return analyze_code(file_path)

    return run_cs_cli(lambda: review_code_health_of(file_path))

@mcp.tool()
def select_project() -> str:
    """
    Lists all projects for an organization for selection by the user.
    The user can select the desired project by either its name or ID.
    
    Returns:
        A JSON object with the project name and ID, formatted in a Markdown table 
        with the columns "Project Name" and "Project ID". If the output contains a 
        `description` field, it indicates that a default project is being used from
        the `CS_DEFAULT_PROJECT_ID` environment variable, and the user cannot select a different project.
        Explain this to the user.
    """
    if os.getenv("CS_DEFAULT_PROJECT_ID"):
        return json.dumps({
            'id': int(os.getenv("CS_DEFAULT_PROJECT_ID")),
            'name': 'Default Project (from CS_DEFAULT_PROJECT_ID env var)',
            'description': 'Using default project from CS_DEFAULT_PROJECT_ID environment variable. If you want to be able to select a different project, unset this variable.'
        })
    try:
        url = f"{get_api_url()}/v2/projects"
        response = requests.get(url, headers=get_api_request_headers())
        data = response.json()

        return json.dumps(data)
    except Exception as e:
        return f"Error: {e}"
    
def query_api_list(endpoint, params: dict, key: str) -> list:
    url = f"{get_api_url()}/{endpoint}"
    response = requests.get(url, params=params, headers=get_api_request_headers())
    data = response.json()
    items = data.get(key, [])

    if data.get('max_pages') == 0:
        return items
    
    if data.get('max_pages') < params.get('page', 1):
        params['page'] = params.get('page', 1) + 1
        items.extend(query_api_list(endpoint, params, key))
        
    return items

@mcp.tool()
def list_technical_debt_goals_for_project(project_id: int) -> str:
    """
    Lists the technical debt goals for a project.

    Args:
        project_id: The Project ID selected by the user.
    Returns:
        A JSON array containing the path of a file and its goals, or a string error message if no project was selected.
        Show the goals for each file in a structured format that is easy to read and explain
        the goal description for each file. It also includes a description, please include that in your output.
    """
    try:
        endpoint = f"v2/projects/{project_id}/analyses/latest/files"
        params = {'page_size': 200, 'page': 1, 'filter': 'goals^not-empty', 'fields': 'path,goals'}
        files = query_api_list(endpoint, params, 'files')

        return json.dumps({
            'files': files,
            'description': f"Found {len(files)} files with technical debt goals for project ID {project_id}."
        })
    except Exception as e:
        return f"Error: {e}"
    
@mcp.tool()
def list_technical_debt_goals_for_project_file(file_path: str, project_id: int) -> str:
    """
    Lists the technical debt goals for a specific file in a project.

    Args:
        file_path: The absolute path to the source code file.
        project_id: The Project ID selected by the user.
    Returns:
        A JSON array containing the goals for the specified file, or a string error message if no project was selected.
        Show the goals in a structured format that is easy to read and explain
        the goal description. It also includes a description, please include that in your output.
    """
    try:
        endpoint = f"v2/projects/{project_id}/analyses/latest/files"
        relative_file_path = adapt_mounted_file_path_inside_docker(file_path)
        params = {'filter': f"path~{relative_file_path}", 'fields': 'goals'}
        files = query_api_list(endpoint, params, 'files')
        goals = files[0].get('goals', []) if files else []

        return json.dumps({
            'goals': goals,
            'description': f"Found {len(goals)} technical debt goals for file {relative_file_path} in project ID {project_id}."
        })
    except Exception as e:
        return f"Error: {e}"

@mcp.tool()
def pre_commit_code_health_safeguard(git_repository_path: str) -> str:
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
    cli_command = [cs_cli_path(), "delta", "--output-format=json"]

    def safeguard_code_on(git_repository_path: str) -> str:
        docker_path = adapt_mounted_file_path_inside_docker(git_repository_path)
        run_local_tool(["git", "config", "--system", "--add", "safe.directory", docker_path])
        output = run_local_tool(cli_command, cwd=docker_path)
        return json.dumps(analyze_delta_output(output))

    return run_cs_cli(lambda: safeguard_code_on(git_repository_path))

@mcp.tool()
def list_technical_debt_hotspots_for_project(project_id: int) -> str:
    """
    Lists the technical debt hotspots for a project.

    Args:
        project_id: The Project ID selected by the user.
    Returns:
        A JSON array containing the path of a file, code health score, revisions count and lines of code count.
        Describe the hotspots for each file in a structured format that is easy to read and explain.
        It also includes a description, please include that in your output.
    """
    try:
        endpoint = f"v2/projects/{project_id}/analyses/latest/technical-debt-hotspots"
        params = {'page_size': 200, 'page': 1}
        hotspots = query_api_list(endpoint, params, 'hotspots')

        return json.dumps({
            'hotspots': hotspots,
            'description': f"Found {len(hotspots)} files with technical debt hotspots for project ID {project_id}."
        })
    except Exception as e:
        return f"Error: {e}"
    
@mcp.tool()
def list_technical_debt_hotspots_for_project_file(file_path: str, project_id: int) -> str:
    """
    Lists the technical debt hotspots for a specific file in a project.
    Args:
        file_path: The absolute path to the source code file.
        project_id: The Project ID selected by the user.
    Returns:
        A JSON array containing the code health score, revisions count and lines of code count for the specified file,
        or a string error message if no project was selected.
        Describe the hotspot in a structured format that is easy to read and explain.
        It also includes a description, please include that in your output.
    """
    try:
        relative_file_path = adapt_mounted_file_path_inside_docker(file_path)
        endpoint = f"/v2/projects/{project_id}/analyses/latest/technical-debt-hotspots"
        params = {'filter': f"file_name~{relative_file_path}"}
        hotspots = query_api_list(endpoint, params, 'hotspots')
        hotspot = hotspots[0] if hotspots else {}

        return json.dumps({
            'hotspot': hotspot,
            'description': f"Found technical debt hotspot for file {relative_file_path} in project ID {project_id}."
        })
    except Exception as e:
        return f"Error: {e}"
    
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

# Offer prompts that capture the key use cases. These prompts are more than a 
# convenience; they also enable feature discoverability and guide users.

@mcp.prompt
def review_code_health(context: str | None = None) -> str:
    """
    Review Code Health and assess code quality for the current open file. 
    The file path needs to be sent to the code_health_review MCP tool when using this prompt.
    """
    return (
        "Review the Code Health of the current file using the CodeScene MCP Server and the code_health_review MCP tool.\n\n"
        "Present the Code Health review as a simple summary suitable for an experienced developer. "
        "Highlight and interpret the Code Health score.\n"
        "Keep the review brief (max two paragraphs) and format it for readability.\n"
        "List the main code smells and issues that contribute to a lower Code Health score.\n"
        "For each code smell, briefly explain why it matters and how it impacts maintainability, defects, or development speed."
    )


@mcp.prompt
def plan_code_health_refactoring(context: str | None = None) -> str:
    """
    Plan a prioritized, low-risk refactoring to remediate detected Code Health issues.
    """
    return (
        "```prompt\n"
        "---\n"
        "tools:\n"
        "  - code_health_review\n"
        "  - code_health_refactoring_business_case\n"
        "---\n\n"
        "Your task is to produce a practical, developer-friendly refactoring plan based on a CodeScene Code Health Review.\n\n"
        "Follow these steps:\n\n"
        "1. Run the `code_health_review` tool on the selected files or code changes to detect code smells.\n"
        "2. Focus the plan exclusively on the **functions/methods with the most severe and highest-impact code smells**.\n"
        "3. For each selected function/method, propose a **specific, concise remediation action**, explaining *what to change* and *why it improves readability and maintainability*.\n"
        "4. Motivate each action with the **expected impact on Code Health** and its **business value** (e.g., reduced defects, faster development, lower cognitive load).\n"
        "5. Include a **one-sentence justification of the effort–risk tradeoff** for every proposed action.\n\n"
        "**Deliverable format:**\n"
        "- **Short summary** (1–2 sentences) describing the overall refactoring plan and its expected outcome.\n"
        "- **Prioritized list of remediation tasks**. For each task, include:\n"
        "  - Function/method name  \n"
        "  - Detected code smells  \n"
        "  - Proposed remediation action  \n"
        "  - 1-line business/Code Health motivation  \n"
        "  - 1-sentence effort–risk justification\n\n"
        "Guidelines:\n"
        "- Keep the plan **pragmatic and low-risk**, emphasizing high-impact improvements first.\n"
        "- If details are missing, make **reasonable assumptions** and briefly state them.\n\n"
        "```"
    )

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
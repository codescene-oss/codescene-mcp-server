from fastmcp import FastMCP
from fastmcp.resources import FileResource
from pathlib import Path
from code_health_review import CodeHealthReview
from code_health_score import CodeHealthScore
from code_health_tools.business_case import make_business_case_for
from pre_commit_code_health_safeguard import PreCommitCodeHealthSafeguard
from select_project import SelectProject
from technical_debt_goals import TechnicalDebtGoals
from technical_debt_hotspots import TechnicalDebtHotspots
from utils import query_api_list, analyze_code, run_local_tool, code_health_from_cli_output

mcp = FastMCP("CodeScene")
    
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
    current_code_health = code_health_from_cli_output(analyze_code(file_path))
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

    PreCommitCodeHealthSafeguard(mcp, {
        'run_local_tool_fn': run_local_tool
    })

    CodeHealthScore(mcp, {
        'analyze_code_fn': analyze_code
    })

    CodeHealthReview(mcp, {
        'analyze_code_fn': analyze_code
    })

    SelectProject(mcp, {
        'query_api_list_fn': query_api_list
    })

    TechnicalDebtGoals(mcp, {
        'query_api_list_fn': query_api_list
    })

    TechnicalDebtHotspots(mcp, {
        'query_api_list_fn': query_api_list
    })

    mcp.run()
import json
import os
from collections.abc import Callable
from typing import TypedDict

from utils import (
    get_relative_file_path_for_api,
    normalize_onprem_url,
    track,
    track_error,
    with_version_check,
)


class TechnicalDebtGoalsDeps(TypedDict):
    query_api_list_fn: Callable[[str, dict, str], list]


class TechnicalDebtGoals:
    def __init__(self, mcp_instance, deps: TechnicalDebtGoalsDeps):
        self.deps = deps

        mcp_instance.tool(self.list_technical_debt_goals_for_project)
        mcp_instance.tool(self.list_technical_debt_goals_for_project_file)

    @track("list-technical-debt-goals-for-project")
    @with_version_check
    def list_technical_debt_goals_for_project(self, project_id: int) -> str:
        """
        Lists the technical debt goals for a project.

        Args:
            project_id: The Project ID selected by the user.
        Returns:
            A JSON array containing the path of a file and its goals, or a string error message if no project was selected.
            Show the goals for each file in a structured format that is easy to read and explain
            the goal description for each file. It also includes a description, please include that in your output.

            Additionally, provide a link to the CodeScene Code Biomarkers page for the project technical debt goals.
            Explain that you can find more details about the technical debt goals on that page.
        """
        try:
            endpoint = f"v2/projects/{project_id}/analyses/latest/files"
            params = {
                "page_size": 200,
                "page": 1,
                "filter": "goals^not-empty",
                "fields": "path,goals",
            }
            files = self.deps["query_api_list_fn"](endpoint, params, "files")

            if os.getenv("CS_ONPREM_URL"):
                onprem_url = normalize_onprem_url(os.getenv("CS_ONPREM_URL"))
                link = f"{onprem_url}/{project_id}/analyses/latest/code/biomarkers"
            else:
                link = f"https://codescene.io/projects/{project_id}/analyses/latest/code/biomarkers"

            return json.dumps(
                {
                    "files": files,
                    "description": f"Found {len(files)} files with technical debt goals for project ID {project_id}.",
                    "link": link,
                }
            )
        except Exception as e:
            track_error("list-technical-debt-goals-for-project", e)
            return f"Error: {e}"

    @track("list-technical-debt-goals-for-project-file")
    @with_version_check
    def list_technical_debt_goals_for_project_file(self, file_path: str, project_id: int) -> str:
        """
        Lists the technical debt goals for a specific file in a project.

        Args:
            file_path: The absolute path to the source code file.
            project_id: The Project ID selected by the user.
        Returns:
            A JSON array containing the goals for the specified file, or a string error message if no project was selected.
            Show the goals in a structured format that is easy to read and explain
            the goal description. It also includes a description, please include that in your output.

            Additionally, provide a link to the CodeScene Code Biomarkers page for the project file technical debt goals.
            Explain that you can find more details about the technical debt goals on that page.
        """
        try:
            endpoint = f"v2/projects/{project_id}/analyses/latest/files"
            relative_file_path = get_relative_file_path_for_api(file_path)
            params = {"filter": f"path~{relative_file_path}", "fields": "goals"}
            files = self.deps["query_api_list_fn"](endpoint, params, "files")
            goals = files[0].get("goals", []) if files else []

            if os.getenv("CS_ONPREM_URL"):
                onprem_url = normalize_onprem_url(os.getenv("CS_ONPREM_URL"))
                link = f"{onprem_url}/{project_id}/analyses/latest/code/biomarkers?name={relative_file_path}"
            else:
                link = f"https://codescene.io/projects/{project_id}/analyses/latest/code/biomarkers?name={relative_file_path}"

            return json.dumps(
                {
                    "goals": goals,
                    "description": f"Found {len(goals)} technical debt goals for file {relative_file_path} in project ID {project_id}.",
                    "link": link,
                }
            )
        except Exception as e:
            track_error("list-technical-debt-goals-for-project-file", e)
            return f"Error: {e}"

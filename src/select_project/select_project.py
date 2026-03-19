import json
import os
from collections.abc import Callable
from typing import TypedDict

from utils import require_access_token, select_project_properties, track, track_error, with_version_check


class SelectProjectDeps(TypedDict):
    query_api_list_fn: Callable[[str, dict, str], list]


class SelectProject:
    def __init__(self, mcp_instance, deps: SelectProjectDeps):
        self.deps = deps

        mcp_instance.tool(self.select_project)

    @require_access_token
    @track("select-project", select_project_properties)
    @with_version_check
    def select_project(self) -> str:
        """
        Lists all projects for an organization for selection by the user.
        The user can select the desired project by either its name or ID.

        When to use:
            Use this tool before project-scoped API tools so the user can pick
            the project context explicitly.

        Limitations:
            - If default_project_id is configured, the server returns that
              project and selection is effectively locked.

        Args:
            None.

        Returns:
            A JSON object with the project name and ID, formatted in a Markdown table
            with the columns "Project Name" and "Project ID". If the output contains a
            `description` field, it indicates that a default project is configured
            (`default_project_id`), and the user cannot select a different project.
            Explain this to the user.

            Additionally, a `link` field is provided to guide the user to the
            Codescene projects page where the user can find more detailed information about each project.
            Make sure to include this link in the output, and explain its purpose clearly.

        Example:
            Call without arguments. If default_project_id is configured,
            explain that the returned project is fixed unless that config is changed via set_config.
        """
        link = f"{os.getenv('CS_ONPREM_URL')}" if os.getenv("CS_ONPREM_URL") else "https://codescene.io/projects"

        default_project_id = os.getenv("CS_DEFAULT_PROJECT_ID")
        if default_project_id:
            return json.dumps(
                {
                    "id": int(default_project_id),
                    "name": "Default Project (from CS_DEFAULT_PROJECT_ID env var)",
                    "description": "Using default project from CS_DEFAULT_PROJECT_ID environment variable. If you want to be able to select a different project, unset this variable.",
                    "link": link,
                }
            )
        try:
            data = self.deps["query_api_list_fn"]("v2/projects", {}, "projects")

            return json.dumps({"projects": data, "link": link})
        except Exception as e:
            track_error("select-project", e)
            return f"Error: {e}"

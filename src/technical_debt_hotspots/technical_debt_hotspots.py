import json
import os
from typing import Callable, TypedDict

from utils import adapt_mounted_file_path_inside_docker, normalize_onprem_url


class TechnicalDebtHotspotsDeps(TypedDict):
    query_api_list_fn: Callable[[str, dict, str], list]

class TechnicalDebtHotspots:
    def __init__(self, mcp_instance, deps: TechnicalDebtHotspotsDeps):
        self.deps = deps

        mcp_instance.tool(self.list_technical_debt_hotspots_for_project)
        mcp_instance.tool(self.list_technical_debt_hotspots_for_project_file)

    def list_technical_debt_hotspots_for_project(self, project_id: int) -> str:
        """
        Lists the technical debt hotspots for a project.

        Args:
            project_id: The Project ID selected by the user.
        Returns:
            A JSON array containing the path of a file, code health score, revisions count and lines of code count.
            Describe the hotspots for each file in a structured format that is easy to read and explain.
            It also includes a description, please include that in your output.

            Additionally, a `link` field is provided to guide the user to the
            Codescene technical debt hotspots page for the project where the user can find more detailed information about each hotspot.
            Make sure to include this link in the output, and explain its purpose clearly.
        """
        try:
            endpoint = f"v2/projects/{project_id}/analyses/latest/technical-debt"
            params = {'page_size': 200, 'page': 1, 'refactoring_targets': "true"}
            hotspots = self.deps["query_api_list_fn"](endpoint, params, 'result')

            if os.getenv("CS_ONPREM_URL"):
                onprem_url = normalize_onprem_url(os.getenv("CS_ONPREM_URL"))
                link = f"{onprem_url}/{project_id}/analyses/latest/code/technical-debt/system-map#hotspots"
            else:
                link = f"https://codescene.io/projects/{project_id}/analyses/latest/code/technical-debt/system-map#hotspots"
                
            return json.dumps({
                'hotspots': hotspots,
                'description': f"Found {len(hotspots)} files with technical debt hotspots for project ID {project_id}.",
                'link': link
            })
        except Exception as e:
            return f"Error: {e}"

    def list_technical_debt_hotspots_for_project_file(self, file_path: str, project_id: int) -> str:
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

            Additionally, a `link` field is provided to guide the user to the
            Codescene technical debt hotspots page for the project where the user can find more detailed information about each hotspot.
            Make sure to include this link in the output, and explain its purpose clearly.
        """
        try:
            mounted_file_path = adapt_mounted_file_path_inside_docker(file_path)
            relative_file_path = mounted_file_path.lstrip("/mount/")
            endpoint = f"/v2/projects/{project_id}/analyses/latest/technical-debt"
            params = {'filter': f"file_name~{relative_file_path}", 'refactoring_targets': "true"}
            hotspots = self.deps["query_api_list_fn"](endpoint, params, 'result')
            hotspot = hotspots[0] if hotspots else None

            if os.getenv("CS_ONPREM_URL"):
                onprem_url = normalize_onprem_url(os.getenv("CS_ONPREM_URL"))
                link = f"{onprem_url}/{project_id}/analyses/latest/code/technical-debt/system-map#hotspots"
            else:
                link = f"https://codescene.io/projects/{project_id}/analyses/latest/code/technical-debt/system-map#hotspots"
                

            if hotspot is None:
                return json.dumps({
                    'hotspot': {},
                    'description': f"Found no technical debt hotspot for file {relative_file_path} in project ID {project_id}.",
                    'link': link
                })

            return json.dumps({
                'hotspot': hotspot,
                'description': f"Found technical debt hotspot for file {relative_file_path} in project ID {project_id}.",
                'link': link
            })
        except Exception as e:
            return f"Error: {e}"
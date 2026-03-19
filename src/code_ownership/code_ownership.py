import json
import os
from collections.abc import Callable
from itertools import groupby
from typing import TypedDict

from utils import (
    code_ownership_properties,
    get_relative_file_path_for_api,
    normalize_onprem_url,
    require_access_token,
    track,
    track_error,
    with_version_check,
)


class CodeOwnershipDeps(TypedDict):
    query_api_list_fn: Callable[[str, dict, str], list]


class CodeOwnership:
    def __init__(self, mcp_instance, deps: CodeOwnershipDeps):
        self.deps = deps

        mcp_instance.tool(self.code_ownership_for_path)

    @require_access_token
    @track("code-ownership-for-path", code_ownership_properties)
    @with_version_check
    def code_ownership_for_path(self, project_id: int, path: str) -> str:
        """
        Find the owner or owners of a specific path in a project.

        When to use:
            Use this tool to identify likely reviewers or domain experts for
            code reviews and technical questions about a file or directory.

        Limitations:
            - Requires a valid project_id.
            - Uses the latest project analysis data available in CodeScene.
            - If no matching ownership data is found, an empty JSON array is returned.

        Args:
            project_id: CodeScene project identifier.
            path: Absolute or repository-relative path to a file or directory.

        Returns:
            A list of owners and their paths that they own. The name of the owner who can be responsible
            for code reviews or inquiries about the file and a link to the CodeScene System Map page filtered
            by the owner. Explain that this link can be used to see more details
            about the owner's contributions and interactions within the project.
            You MUST always show a link after every owner. Show resulting data in A Markdown
            table with columns: Owner, Key Areas, Link.

        Example:
            Call with project_id=42 and path="/repo/src/service.py", then
            present each owner row with its corresponding system-map link.
        """
        try:
            endpoint = f"v2/projects/{project_id}/analyses/latest/files"
            relative_path = get_relative_file_path_for_api(path)
            params = {"filter": f"path~{relative_path}", "fields": "owner,path"}
            files = self.deps["query_api_list_fn"](endpoint, params, "files")

            if not files or len(files) == 0:
                return json.dumps([])

            result = []
            sorted_files = sorted(files, key=lambda x: x["owner"])

            for k, g in groupby(sorted_files, key=lambda x: x["owner"]):
                onprem_url_env = os.getenv("CS_ONPREM_URL")
                if onprem_url_env:
                    onprem_url = normalize_onprem_url(onprem_url_env)
                    link = f"{onprem_url}/{project_id}/analyses/latest/social/individuals/system-map?author=author:{k}"
                else:
                    link = f"https://codescene.io/projects/{project_id}/analyses/latest/social/individuals/system-map?author=author:{k}"

                result.append({"owner": k, "paths": [item["path"] for item in g], "link": link})

            return json.dumps(result)

        except Exception as e:
            track_error("code-ownership-for-path", e)
            return f"Error: {e}"

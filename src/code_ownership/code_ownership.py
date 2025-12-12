from itertools import groupby
import json
import os
from typing import TypedDict, Callable
from utils import adapt_mounted_file_path_inside_docker, normalize_onprem_url, track, track_error, with_version_check


class CodeOwnershipDeps(TypedDict):
    query_api_list_fn: Callable[[str, dict, str], list]

class CodeOwnership:
    def __init__(self, mcp_instance, deps: CodeOwnershipDeps):
        self.deps = deps

        mcp_instance.tool(self.code_ownership_for_path)

    @track("code-ownership-for-path")
    @with_version_check
    def code_ownership_for_path(self, project_id: int, path: str) -> str:
        """
        Find the owner or owners of a specific path in a project.

        Returns:
            A list of owners and their paths that they own. The name of the owner who can be responsible 
            for code reviews or inquiries about the file and a link to the CodeScene System Map page filtered
            by the owner. Explain that this link can be used to see more details
            about the owner's contributions and interactions within the project. 
            You MUST always show a link after every owner. Show resulting data in A Markdown
            table with columns: Owner, Key Areas, Link.
        """
        try:
            endpoint = f"v2/projects/{project_id}/analyses/latest/files"
            mounted_path = adapt_mounted_file_path_inside_docker(path)
            relative_path = mounted_path.lstrip("/mount/")
            params = {'filter': f"path~{relative_path}", 'fields': 'owner,path'}
            files = self.deps["query_api_list_fn"](endpoint, params, 'files')
            
            if not files or len(files) == 0:
                return json.dumps([])

            result = []
            sorted_files = sorted(files, key=lambda x: x['owner'])
            
            for k, g in groupby(sorted_files, key=lambda x: x['owner']):
                if os.getenv("CS_ONPREM_URL"):
                    onprem_url = normalize_onprem_url(os.getenv("CS_ONPREM_URL"))
                    link = f"{onprem_url}/{project_id}/analyses/latest/social/individuals/system-map?author=author:{k}"
                else:
                    link = f"https://codescene.io/projects/{project_id}/analyses/latest/social/individuals/system-map?author=author:{k}"

                result.append({
                    'owner': k,
                    'paths': [item['path'] for item in g],
                    'link': link
                })

            return json.dumps(result)
        
        except Exception as e:
            track_error("code-ownership-for-path", e)
            return f"Error: {e}"
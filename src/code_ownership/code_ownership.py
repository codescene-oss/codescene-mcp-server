import json
import os
from typing import TypedDict, Callable
from utils import adapt_mounted_file_path_inside_docker


class CodeOwnershipDeps(TypedDict):
    query_api_list_fn: Callable[[str, dict, str], list]

class CodeOwnership:
    def __init__(self, mcp_instance, deps: CodeOwnershipDeps):
        self.deps = deps

        mcp_instance.tool(self.code_ownership_for_path)

    def code_ownership_for_path(self, project_id: int, path: str) -> str:
        """
        Find the owner or owners of a specific path in a project.

        Returns:
            A list of owners for the given path. The name of the owner who can be responsible 
            for code reviews or inquiries about the file and a link to the CodeScene System Map page filtered
            by the owner. Explain that this link can be used to see more details
            about the owner's contributions and interactions within the project.
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

            for file in files:
                owner = file.get('owner', 'Unknown')

                if os.getenv("CS_ONPREM_URL"):
                    link = f"{os.getenv('CS_ONPREM_URL')}/{project_id}/analyses/latest/social/individuals/system-map?author=author:{owner}"
                else:
                    link = f"https://codescene.io/projects/{project_id}/analyses/latest/social/individuals/system-map?author=author:{owner}"

                result.append({
                    'owner': owner,
                    'path': file.get('path'),
                    'link': link
                })

            return json.dumps(result)
        
        except Exception as e:
            return f"Error: {e}"
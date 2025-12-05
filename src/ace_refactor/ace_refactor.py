import json
import os
from typing import Callable, TypedDict

from utils import adapt_mounted_file_path_inside_docker, normalize_onprem_url

class AceRefactorDeps(TypedDict):
    post_refactor_fn: Callable[[dict], dict]

class AceRefactor:
    def __init__(self, mcp_instance, deps: AceRefactorDeps):
        self.deps = deps

        mcp_instance.tool(self.refactor_code_snippet)

    def refactor_code_snippet(self, source_snippet: dict, code_smell: dict) -> str:
        """
        Refactor a code snippet (a single function).
        Args:
            source_snippet: A JSON object describing the code to refactor, with these properties:
              - file-type: The extension of a the file the sorce code is from
              - function-type: MemberFn or StandaloneFn, depending on what type of function it is
              - body: The full function code
            code_smell: A JSON object describing the code smell to refactor, with these properties:
              - category: The name of the code smell as described by CodeScene
              - start-line: The 0-indexed line nbr of the function where the code smell occurs
        Returns:
            A JSON object describing the refactoring, with these properties
              - code: A refactoring of the supplied code
              - declarations: Optional declarations that should go in a header file for languages like C++
              - confidence: The confidence level of the resulting refactoring
              - reasons: A list of strings describing the reasons for the assigned confidence level
        """
        try:
            payload = {
              'api-version': 'v2',
              'source-snippet': source_snippet,
              'review': [code_smell]}
            response = self.deps['post_refactor_fn'](payload)

            return json.dumps({
              'code': response['code'],
              'declarations': response.get('declarations', ''),
              'confidence': response['confidence']['description'],
              'reasons': [x['summary'] for x in response.get('reasons', [])]
            })
        except Exception as e:
            return f"Error: {e}"
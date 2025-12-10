import json
import os
import re
from typing import Callable, TypedDict, Optional
from utils import adapt_mounted_file_path_inside_docker, cs_cli_path

class AutoRefactorError(Exception):
    pass

class AutoRefactorDeps(TypedDict):
    post_refactor_fn: Callable[[dict], dict]
    run_local_tool_fn: Callable[[list, Optional[str]], str]

class AutoRefactor:
    def __init__(self, mcp_instance, deps: AutoRefactorDeps):
        self.deps = deps

        mcp_instance.tool(self.code_health_auto_refactor)

    def _run_cs_cli(self, args: list[str]):
        output = self.deps["run_local_tool_fn"]([cs_cli_path()] + args)
        return json.loads(output)
        
    def _parse_fns(self, file_path: str) -> list[dict]:
        cli_command = ["parse-fns", "--path", adapt_mounted_file_path_inside_docker(file_path)]
        return self._run_cs_cli(cli_command)

    def _review(self, file_path: str) -> dict:
        cli_command = ["review", "--output-format=json", adapt_mounted_file_path_inside_docker(file_path)]
        return self._run_cs_cli(cli_command)

    def _get_function(self, functions: list[dict], function_name: str) -> dict:
        return next((f for f in functions if f["name"] == function_name), False)

    def _get_code_smells(self, review: dict, function: dict) -> dict:
      return [
          {
            'category': code_smell['category'],
            'start-line': f['start-line'] - function['start-line'] + 1
          }
          for code_smell in review['review']
          for f in code_smell.get('functions',[])
          # For Complex Conditional, the title has ':[line-nbr]' appended
          if re.fullmatch(rf'{function['name']}(:\d+)?', f['title'])
      ]

    def code_health_auto_refactor(self, file_path: str, function_name: str) -> str:
        """
        Refactor a single function to fix the code health problem of highest priority.
        This tool is supported for these languages:
            - JavaScript/TypeScript
            - Java
            - C#
            - C++
        and for these code smells:
            - Complex Conditional
            - Bumpy Road Ahead
            - Complex Method
            - Deep, Nested Complexity
            - Large Method

        Args:
            file_path: The absolute path to the source code file containing the function to refactor.
            function_name: The name of the function to refactor. If there is a class scope prefix, it needs to be included.
        Returns:
            A JSON object describing the refactoring, with these properties
              - code: The refactored code, possibly containing multiple functions.
              - declarations: Optional (used for languages like C++). Declarations of additional functions introduced when refactoring.
              - confidence: The confidence level of the resulting refactoring.
              - reasons: A list of strings describing the reasons for the assigned confidence level.
        """
        try:
            functions = self._parse_fns(file_path)
            review = self._review(file_path)
            function = self._get_function(functions, function_name)
            if not function:
                return f"Error: Couldn't find function: {function_name}"
            code_smells = self._get_code_smells(review, function)
            if not code_smells:
                return f"Error: No code smells were found in {function_name}"
            _, ext = os.path.splitext(file_path)
            payload = {
              'api-version': 'v2',
              'source-snippet': {
                'file-type': ext[1:],
                'body': function['body'],
                'function-type': function.get('function-type', 'Unknown')
              },
              'review': code_smells
            }
            response = self.deps['post_refactor_fn'](payload)

            return json.dumps({
              'code': response['code'],
              'declarations': response.get('declarations', ''),
              'confidence': response['confidence']['description'],
              'reasons': [x['summary'] for x in response.get('reasons', [])]
            })
        except Exception as e:
            return f"Error: {e}"
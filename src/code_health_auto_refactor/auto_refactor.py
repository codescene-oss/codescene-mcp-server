import json
import os
import re
from collections.abc import Callable
from typing import TypedDict

from utils import (
    adapt_mounted_file_path_inside_docker,
    cs_cli_path,
    find_git_root,
    get_platform_details,
)
from utils.docker_path_adapter import get_relative_path_from_git_root


class AutoRefactorError(Exception):
    pass


class AutoRefactorDeps(TypedDict):
    post_refactor_fn: Callable[[dict], dict]
    run_local_tool_fn: Callable[[list, str | None], str]


class AutoRefactor:
    def __init__(self, mcp_instance, deps: AutoRefactorDeps):
        self.deps = deps

        mcp_instance.tool(self.code_health_auto_refactor)

    def _run_cs_cli(self, args: list[str], git_root: str):
        output = self.deps["run_local_tool_fn"]([cs_cli_path(get_platform_details())] + args, git_root)
        return json.loads(output)

    def _get_cli_file_path(self, file_path: str, git_root: str) -> str:
        """Get the appropriate file path for CLI commands based on environment."""
        if os.getenv("CS_MOUNT_PATH"):
            # Docker environment - use mounted path
            return adapt_mounted_file_path_inside_docker(file_path)
        else:
            # Local environment - use relative path from git root
            return get_relative_path_from_git_root(file_path, git_root)

    def _parse_fns(self, file_path: str, git_root: str) -> list[dict]:
        cli_path = self._get_cli_file_path(file_path, git_root)
        cli_command = ["parse-fns", "--path", cli_path]
        return self._run_cs_cli(cli_command, git_root)

    def _review(self, file_path: str, git_root: str) -> dict:
        cli_path = self._get_cli_file_path(file_path, git_root)
        cli_command = ["review", "--output-format=json", cli_path]
        return self._run_cs_cli(cli_command, git_root)

    def _get_function(self, functions: list[dict], function_name: str) -> dict:
        return next((f for f in functions if f["name"] == function_name), False)

    def _get_code_smells(self, review: dict, function: dict) -> dict:
        return [
            {
                "category": code_smell["category"],
                # Make start-line relative to the function start-line
                "start-line": f["start-line"] - function["start-line"] + 1,
            }
            for code_smell in review["review"]
            for f in code_smell.get("functions", [])
            # For Complex Conditional, the title has ':[line-nbr]' appended
            if re.fullmatch(rf"{function['name']}(:\d+)?", f["title"])
        ]

    def _post_refactor(self, file_path: str, function: dict, code_smells: list[dict]):
        _, ext = os.path.splitext(file_path)
        payload = {
            "api-version": "v2",
            "source-snippet": {
                "file-type": ext[1:],
                "body": function["body"],
                "function-type": function.get("function-type", "Unknown"),
            },
            "review": code_smells,
        }
        return self.deps["post_refactor_fn"](payload)

    def code_health_auto_refactor(self, file_path: str, function_name: str) -> str:
        """
        Refactor a single function to fix specific code health problems.
        Thie auto-refactor uses CodeScene ACE, and is intended as an initial
        refactoring to increase the modularity of the code so that you as an
        AI agent can continue and iterate with more specific refactorings.
        The code_health_auto_refactor tool is supported for these languages:
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
        IMPORTANT:
            - Only use this tool for functions shorter than 300 lines of code.
            - Insert any new functions close to the refactored function.

        Args:
            file_path: The absolute path to the source code file containing the function to refactor.
            function_name: The name of the function to refactor. If there is a class scope prefix, it needs to be included.
        Returns:
            A JSON object describing the refactoring, with these properties
              - code: The refactored function plus new extracted functions.
              - declarations: Optional (used for languages like C++). Declarations of additional functions introduced when refactoring.
                When present, find the right include file and insert the declarations there. Note that some C++ refactorings result
                in standalone functions; standalone functions should just be inserted in the implementation unit, not declared in
                include files.
              - confidence: The confidence level of the resulting refactoring. For low confidence, review the
                refactoring and fix any introduced problems.
              - reasons: A list of strings describing the reasons for the assigned confidence level.
                Use this list of strings to direct fixes of the refactored code.
        """
        try:
            if os.getenv("CS_ACE_ACCESS_TOKEN") is None:
                return "Error: This tool needs a token valid for CodeScene ACE in CS_ACE_ACCESS_TOKEN. See the ACE activation instructions in https://github.com/codescene-oss/codescene-mcp-server?tab=readme-ov-file#-activate-ace-in-codescene-mcp"

            git_root = find_git_root(file_path)
            functions = self._parse_fns(file_path, git_root)
            review = self._review(file_path, git_root)

            function = self._get_function(functions, function_name)
            if not function:
                return f"Error: Couldn't find function: {function_name}"

            code_smells = self._get_code_smells(review, function)
            if not code_smells:
                return f"Error: No code smells were found in {function_name}"

            response = self._post_refactor(file_path, function, code_smells)

            return json.dumps(
                {
                    "code": response["code"],
                    "declarations": response.get("declarations", ""),
                    "confidence": response["confidence"]["description"],
                    "reasons": [x["summary"] for x in response.get("reasons", [])],
                }
            )
        except Exception as e:
            return f"Error: {e}"

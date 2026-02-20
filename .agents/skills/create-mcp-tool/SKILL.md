---
name: create-mcp-tool
description: Scaffold a new MCP tool for the CodeScene MCP Server following the project's established patterns for directory structure, dependency injection, decorators, testing, and registration.
metadata:
  audience: contributors
  language: python
---

## Purpose

Use this skill when creating a new MCP tool in the CodeScene MCP Server project. It encodes the exact conventions, patterns, and file structure that all existing tools follow.

## Project Context

- **Framework:** FastMCP (`from fastmcp import FastMCP`)
- **Language:** Python
- **Source root:** `src/`
- **Server entry point:** `src/cs_mcp_server.py`
- **Shared utilities:** `src/utils/` (re-exported from `src/utils/__init__.py`)

## Step-by-Step

### 1. Create the tool directory

Create a new directory under `src/` named after the tool using snake_case:

```
src/<tool_name>/
  __init__.py
  <tool_name>.py
  test_<tool_name>.py
```

### 2. Choose a dependency pattern

Tools receive their external dependencies via injection for testability. Pick the pattern that matches your tool's data source:

| Dependency | Signature | When to use |
|---|---|---|
| `analyze_code_fn` | `Callable[[str], str]` | Analyze a local source file via the CodeScene CLI |
| `run_local_tool_fn` | `Callable[..., str]` | Run a local tool subprocess (pre-commit safeguard, change-set analysis, auto-refactor) |
| `query_api_list_fn` | `Callable[[str, dict, str], list]` | Query the CodeScene cloud/on-prem API with pagination |
| `post_refactor_fn` | `Callable[..., str]` | POST to the refactoring API endpoint |

A tool may combine multiple dependencies (see `AutoRefactor` which uses both `post_refactor_fn` and `run_local_tool_fn`).

### 3. Define the deps TypedDict and tool class

```python
from collections.abc import Callable
from typing import TypedDict

from utils import track, with_version_check


class MyToolDeps(TypedDict):
    analyze_code_fn: Callable[[str], str]


class MyTool:
    def __init__(self, mcp_instance, deps: MyToolDeps):
        self.deps = deps

        # Register each public tool method with the MCP server
        mcp_instance.tool(self.my_tool_method)
```

Key rules:
- The constructor stores `deps` on `self` and registers tool methods via `mcp_instance.tool(self.method)`.
- A single class can register multiple tool methods (see `TechnicalDebtHotspots` which registers two).
- Keep private helpers as separate methods prefixed with `_`.

### 4. Implement the tool method

```python
    @with_version_check
    @track("my-tool-event")
    def my_tool_method(self, file_path: str) -> str:
        """
        One-line summary of what this tool does.

        Detailed description that becomes the MCP tool description visible
        to LLM clients. Be specific about inputs, outputs, and how the
        LLM should present results to users.

        Args:
            file_path: The absolute path to the source code file.
        Returns:
            A description of what the return value contains and how
            the LLM should interpret/display it.
        """
        try:
            result = self.deps["analyze_code_fn"](file_path)
            return f"Result: {result}"
        except Exception as e:
            from utils import track_error
            track_error("my-tool-event", e)
            return f"Error: {e}"
```

Conventions:
- **Decorator order matters:** `@with_version_check` outermost, then `@track("event-name")`.
- **Return type is always `str`.**
- **The docstring is critical** -- it becomes the tool description that LLM clients see. Write it for an LLM audience: explain what the tool does, what arguments it needs, and how to present results.
- **Error handling:** Wrap API/CLI calls in try/except, use `track_error` to report failures, and return a user-friendly error string.

### 5. Create `__init__.py`

Re-export the class and deps TypedDict:

```python
from .<tool_name> import MyTool, MyToolDeps
```

### 6. Write tests

Use `unittest` with mocked dependencies and a throwaway `FastMCP("Test")` instance:

```python
import json
import unittest

from fastmcp import FastMCP

from .<tool_name> import MyTool


class TestMyTool(unittest.TestCase):
    def test_success_case(self):
        def mock_analyze_code(file_path: str):
            return json.dumps({"score": 9.5})

        instance = MyTool(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})
        result = instance.my_tool_method("test.py")

        self.assertIn("9.5", result)

    def test_error_case(self):
        def mock_analyze_code(file_path: str):
            return json.dumps({})

        instance = MyTool(FastMCP("Test"), {"analyze_code_fn": mock_analyze_code})
        result = instance.my_tool_method("test.py")

        self.assertIn("Error", result)
```

Key patterns:
- Mock each dependency function to return controlled data.
- Test both happy path and error/empty cases.
- Instantiate the tool class with `FastMCP("Test")` -- this registers the tool but does not start a server.

### 7. Register in `cs_mcp_server.py`

1. Add the import at the top of `src/cs_mcp_server.py`:

```python
from my_tool import MyTool
```

2. Instantiate the tool in the `# tools` section, passing the `mcp` instance and the real dependency functions:

```python
MyTool(mcp, {"analyze_code_fn": analyze_code})
```

The available real dependency functions are imported from `utils` at the top of `cs_mcp_server.py`:
- `analyze_code` -- for `analyze_code_fn`
- `run_local_tool` -- for `run_local_tool_fn`
- `query_api_list` -- for `query_api_list_fn`
- `post_refactor` -- for `post_refactor_fn`

## Canonical Example: `code_health_score`

The simplest existing tool to reference is `src/code_health_score/`. It demonstrates:
- Single dependency (`analyze_code_fn`)
- Single tool method with both decorators
- Private helper method
- Clean `__init__.py` re-export
- Unit tests with mocked deps

Refer to `src/code_health_score/score_calculator.py` as the minimal template.

## Checklist

Before considering the tool complete:

- [ ] Tool directory created under `src/` with `__init__.py`, implementation, and tests
- [ ] `TypedDict` deps defined for dependency injection
- [ ] Tool method(s) decorated with `@with_version_check` (outer) and `@track` (inner)
- [ ] Docstring written for LLM consumption (describes purpose, args, return format)
- [ ] Return type is `str`
- [ ] Error handling with `track_error` for failures
- [ ] Tests cover success and error paths using mocked deps
- [ ] Tool imported and instantiated in `src/cs_mcp_server.py`
- [ ] Run tests with `python -m pytest src/<tool_name>/`

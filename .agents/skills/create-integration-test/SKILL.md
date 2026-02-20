---
name: create-integration-test
description: Write an end-to-end integration test for the CodeScene MCP Server, covering file structure, backend abstraction, MCPClient usage, test registration, and verification.
metadata:
  audience: contributors
  language: python
---

## Purpose

Use this skill when adding an end-to-end integration test for a new or existing MCP tool or feature. It encodes the exact conventions, infrastructure, and file structure that all existing integration tests follow.

## Project Context

- **Test framework:** Custom plain-Python scripts (no pytest); exit codes `0` (pass) / `1` (fail)
- **Test location:** `tests/integration/`
- **Shell entry points:** `tests/run-integration-tests.sh` (Linux/macOS), `tests/run-integration-tests.ps1` (Windows)
- **Central orchestrator:** `tests/integration/run_all_tests.py` — builds the executable/image and runs every registered test module
- **Backend abstraction:** `ServerBackend` ABC with two implementations: `NuitkaBackend` (compiled executable) and `DockerBackend` (container). Every test must work with both.
- **Key environment variables:** `CS_ACCESS_TOKEN` (required), `CS_ONPREM_URL`, `CS_DISABLE_VERSION_CHECK`, `CS_MOUNT_PATH` (docker only)

### Infrastructure modules (all in `tests/integration/`)

| Module | Role |
|---|---|
| `mcp_client.py` | `MCPClient` class — starts the MCP server as a subprocess, communicates via JSON-RPC over stdio |
| `server_backends.py` |`NuitkaBackend`, `DockerBackend` |
| `file_utils.py` | `create_test_environment()`, `create_git_repo()`, `safe_temp_directory()`, `cleanup_dir()` |
| `response_parsers.py` | `extract_result_text()`, `extract_code_health_score()` |
| `test_output.py` | `print_header()`, `print_test()`, `print_summary()` |
| `fixtures.py` | Sample code constants with known Code Health characteristics and expected score ranges |
| `test_utils.py` | Re-exports everything above for convenience |

## Step-by-Step

### 1. Create the test file

Create `tests/integration/test_<feature>.py` with the standard boilerplate:

```python
#!/usr/bin/env python3
"""
<Feature> integration tests.

Tests that the MCP server correctly <what this validates>.

This test suite validates:
1. <First thing>
2. <Second thing>
"""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

from test_utils import (
    MCPClient,
    NuitkaBackend,
    ServerBackend,
    create_git_repo,
    extract_result_text,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)
```

Conventions:
- The `sys.path.insert` line is required — integration tests are not installed as a package.
- Import only what you need from `test_utils`. All infrastructure modules are re-exported from there.
- `NuitkaBackend` is only needed for the standalone entry point (step 5). The test logic itself is backend-agnostic — it receives a `ServerBackend` and uses `backend.get_command()` / `backend.get_env()`.
- The module docstring should explain **what** the test validates and **why** (e.g., what bug it prevents or what user scenario it covers).

### 2. Add fixtures if needed

If your test requires code samples with specific Code Health characteristics that the existing fixtures do not cover, add them to `fixtures.py`:

```python
# Description of the code and expected characteristics
NEW_SAMPLE_CODE = '''"""
Module docstring.
"""

def example():
    pass
'''
```

Then update `get_sample_files()` and `get_expected_scores()` if the new sample should be part of the standard test repository:

```python
def get_sample_files() -> dict[str, str]:
    return {
        # ... existing entries ...
        "src/path/to/new_sample.py": NEW_SAMPLE_CODE,
    }
```

Skip this step if the existing fixtures (`GOOD_PYTHON_CODE`, `COMPLEX_PYTHON_CODE`, `JAVASCRIPT_CODE`, `JAVA_CODE`) are sufficient.

### 3. Implement the backend-aware runner function

This is the core entry point that `run_all_tests.py` calls. Every test module must define one:

```python
def run_<feature>_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all <feature> tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_<feature>_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        # Create git repo with sample files
        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        # Get command and env from backend
        command = backend.get_command(repo_dir)
        env = backend.get_env(os.environ.copy(), repo_dir)

        results = [
            (
                "<Feature> - <Test Name>",
                test_<name>(command, env, repo_dir),
            ),
            # ... more test functions ...
        ]

        return print_summary(results)
```

Key rules:
- Always use `safe_temp_directory()` — it handles Windows file-locking cleanup.
- Always get `command` and `env` from the backend — never hardcode executable paths or environment.
- Collect results as `list[tuple[str, bool]]` and return `print_summary(results)`.

### 4. Implement individual test functions

Each test function follows this pattern:

```python
def test_<name>(command: list[str], env: dict, repo_dir: Path) -> bool:
    """
    Test that <what this specific test validates>.
    """
    print_header("Test: <Descriptive Name>")

    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            print_test("Server started", False)
            return False

        print_test("Server started", True)
        client.initialize()

        # Call the MCP tool under test
        test_file = repo_dir / "src/utils/calculator.py"
        print(f"\n  Analyzing: {test_file}")

        response = client.call_tool(
            "tool_name",
            {"arg": "value"},
            timeout=60,
        )
        result_text = extract_result_text(response)

        # Validate the response
        has_content = len(result_text) > 0
        print_test("Tool returned content", has_content, f"Length: {len(result_text)} chars")

        # Add more specific assertions
        has_expected = "expected_term" in result_text.lower()
        print_test("Response contains expected data", has_expected)

        return has_content and has_expected

    except Exception as e:
        print_test("<Test name>", False, str(e))
        return False
    finally:
        client.stop()
```

Conventions:
- **Always `client.stop()` in `finally`** — leaked server processes break subsequent tests.
- **Always `client.initialize()` before `call_tool()`** — the MCP protocol requires initialization.
- **Use `timeout=60`** for tool calls — Code Health analysis involves real API calls.
- **Use `extract_result_text()`** from `response_parsers.py` — never parse JSON-RPC responses manually.
- **Report every assertion via `print_test()`** — this is how results appear in the test output.
- **Return a single `bool`** — `True` if all assertions passed, `False` otherwise.

### 5. Add the NuitkaBackend convenience wrapper

This lets the test file run standalone with a pre-built executable:

```python
def run_<feature>_tests(executable: Path) -> int:
    """
    Run all <feature> tests.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = NuitkaBackend(executable=executable)
    return run_<feature>_tests_with_backend(backend)
```

### 6. Add the standalone entry point

```python
def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_<feature>.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("<Feature> Integration Tests")
    print("\nThese tests verify <brief description>.")

    return run_<feature>_tests(executable)


if __name__ == "__main__":
    sys.exit(main())
```

### 7. Register in `run_all_tests.py`

Two changes are needed:

**1. Add the import** inside `run_all_tests_with_backend()`, alongside the other test module imports:

```python
from test_<feature> import run_<feature>_tests_with_backend
```

**2. Add the test module call**, appending to `all_results`:

```python
all_results.append(_run_test_module("<Feature> Tests", run_<feature>_tests_with_backend, backend))
```

## Canonical Example: `test_business_case.py`

The simplest existing test to reference is `tests/integration/test_business_case.py`. It demonstrates:
- Three focused test functions with the standard MCPClient lifecycle
- Backend-aware runner collecting results into `print_summary()`
- NuitkaBackend convenience wrapper + standalone `main()` entry point
- Regression testing pattern (checking for specific error strings like `"no such file or directory"`)

Refer to `test_business_case.py` as the minimal template for any new test module.

## Verification

Run the full integration test suite to verify your new test is picked up and passes. Docker builds faster, so start there for a quicker feedback cycle:

**Linux/macOS:**
```bash
# Docker backend (faster build, try this first)
./tests/run-integration-tests.sh --docker

# Static/Nuitka backend (full release-like validation)
./tests/run-integration-tests.sh
```

**Windows:**
```powershell
# Docker backend (faster build, try this first)
.\tests\run-integration-tests.ps1 -Docker

# Static/Nuitka backend (full release-like validation)
.\tests\run-integration-tests.ps1
```

Both scripts accept `--help` (bash) / `-Help` (PowerShell) for additional options, including `--executable` / `-Executable` to skip the build step during iterative development.

## Checklist

Before considering the test complete:

- [ ] Test file created in `tests/integration/` with `test_` prefix
- [ ] Module docstring explains what the test validates and why
- [ ] Imports use `sys.path.insert(0, str(Path(__file__).parent))` pattern
- [ ] Backend-aware runner `run_*_with_backend(backend: ServerBackend) -> int` defined
- [ ] Each test function uses `try/finally` with `client.stop()` in `finally`
- [ ] All test functions use `print_header` and `print_test` for formatted output
- [ ] Results collected as `list[tuple[str, bool]]` and passed to `print_summary()`
- [ ] Standalone `main()` entry point accepts executable path from `sys.argv[1]`
- [ ] NuitkaBackend convenience wrapper provided
- [ ] Test registered in `run_all_tests.py` via `_run_test_module()`
- [ ] Fixtures added to `fixtures.py` if new code samples are needed
- [ ] Full suite passes with Docker: `./tests/run-integration-tests.sh --docker`
- [ ] Full suite passes with static: `./tests/run-integration-tests.sh`

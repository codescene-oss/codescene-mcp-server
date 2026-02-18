#!/usr/bin/env python3
"""
Integration tests for the analyze_change_set MCP tool.

Tests that branch-level Code Health analysis correctly:
- Passes when no code health decline exists on the current branch vs base_ref
- Fails when a commit on the current branch introduces a code health decline
"""

import json
import os
import subprocess
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

# A clean change that should not degrade Code Health
CLEAN_ADDITION = '''

def calculate_median(items: list[float]) -> float:
    """Calculate the median of all items."""
    if not items:
        return 0.0
    sorted_items = sorted(items)
    mid = len(sorted_items) // 2
    if len(sorted_items) % 2 == 0:
        return (sorted_items[mid - 1] + sorted_items[mid]) / 2
    return sorted_items[mid]
'''

# A change that introduces a Complex Conditional code smell:
# 3+ logical operators in a single conditional triggers the smell.
DEGRADING_ADDITION = '''

def validate_order(order, customer, inventory, config):
    """Validate an order with complex business rules."""
    if (order is not None and customer is not None and inventory is not None
            and config is not None and order.get("items") and customer.get("id")
            and inventory.get("stock") and config.get("enabled")
            and order.get("total") > 0 and customer.get("active")
            and not customer.get("banned") and config.get("allow_orders")):
        return True
    if (order is not None and order.get("priority") and customer is not None
            and customer.get("vip") and inventory is not None
            and inventory.get("reserved") and config is not None
            and config.get("vip_enabled") and order.get("total") > 100
            and not order.get("flagged") and customer.get("verified")
            and config.get("allow_vip")):
        return True
    return False
'''


def create_feature_branch_with_file_change(repo_dir: Path, file_path: str, additional_code: str) -> None:
    """
    Create a feature branch and commit a code change.

    Args:
        repo_dir: Path to the git repository
        file_path: Relative path to the file to modify
        additional_code: Code to append to the file
    """
    subprocess.run(
        ["git", "checkout", "-b", "feature"],
        cwd=repo_dir,
        check=True,
        capture_output=True,
    )

    full_path = repo_dir / file_path
    original = full_path.read_text()
    full_path.write_text(original + additional_code)

    subprocess.run(["git", "add", "."], cwd=repo_dir, check=True, capture_output=True)
    subprocess.run(
        ["git", "commit", "-m", "Feature branch change"],
        cwd=repo_dir,
        check=True,
        capture_output=True,
    )


def parse_quality_gates(result_text: str) -> str | None:
    """
    Extract quality_gates from the tool's JSON response.

    Returns:
        "passed", "failed", or None if parsing fails.
    """
    try:
        data = json.loads(result_text)
        return data.get("quality_gates")
    except (json.JSONDecodeError, TypeError):
        return None


def _run_change_set_analysis(command: list[str], env: dict, repo_dir: Path) -> tuple[str, str | None]:
    """
    Run analyze_change_set against 'master' and return the raw result text and quality gates.

    Returns:
        Tuple of (result_text, quality_gates) where quality_gates may be None on parse failure.
    """
    client = MCPClient(command, env=env, cwd=str(repo_dir))

    try:
        if not client.start():
            return ("", None)

        client.initialize()

        response = client.call_tool(
            "analyze_change_set",
            {"base_ref": "master", "git_repository_path": str(repo_dir)},
            timeout=60,
        )

        result_text = extract_result_text(response)
        return (result_text, parse_quality_gates(result_text))
    finally:
        client.stop()


def _setup_repo_with_branch(test_dir: Path, subdir: str, code_change: str) -> Path:
    """
    Create a git repo and a feature branch with a code change.

    Returns:
        Path to the repository on the feature branch.
    """
    repo_dir = create_git_repo(test_dir / subdir, get_sample_files())
    create_feature_branch_with_file_change(repo_dir, "src/utils/calculator.py", code_change)
    return repo_dir


def test_passes_on_clean_branch(command: list[str], env: dict, test_dir: Path) -> bool:
    """Test that analyze_change_set passes when the branch has no code health decline."""
    print_header("Test: Change Set Passes on Clean Branch")

    repo_dir = _setup_repo_with_branch(test_dir, "clean", CLEAN_ADDITION)
    result_text, quality_gates = _run_change_set_analysis(command, env, repo_dir)

    has_content = len(result_text) > 10
    print_test("Tool returned content", has_content, f"Length: {len(result_text)} chars")

    gates_passed = quality_gates == "passed"
    print_test("Quality gates passed (no degradation)", gates_passed, f"quality_gates: {quality_gates}")

    return has_content and gates_passed


def test_fails_on_degraded_branch(command: list[str], env: dict, test_dir: Path) -> bool:
    """Test that analyze_change_set fails when a commit introduces a code health decline."""
    print_header("Test: Change Set Fails on Degraded Branch")

    repo_dir = _setup_repo_with_branch(test_dir, "degraded", DEGRADING_ADDITION)
    result_text, quality_gates = _run_change_set_analysis(command, env, repo_dir)

    gates_failed = quality_gates == "failed"
    print_test("Quality gates failed (degradation detected)", gates_failed, f"quality_gates: {quality_gates}")

    has_findings = "calculator.py" in result_text
    print_test("Findings reference the degraded file", has_findings)

    return gates_failed and has_findings


def run_analyze_change_set_tests(executable: Path) -> int:
    """
    Run all analyze_change_set tests.

    Args:
        executable: Path to the cs-mcp executable

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    backend = NuitkaBackend(executable=executable)
    return run_analyze_change_set_tests_with_backend(backend)


def run_analyze_change_set_tests_with_backend(backend: ServerBackend) -> int:
    """
    Run all analyze_change_set tests using a backend.

    Args:
        backend: Server backend to use

    Returns:
        Exit code (0 for success, 1 for failure)
    """
    with safe_temp_directory(prefix="cs_mcp_change_set_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        command = backend.get_command(test_dir)
        env = backend.get_env(os.environ.copy(), test_dir)

        results = [
            ("Clean Branch Passes", test_passes_on_clean_branch(command, env, test_dir)),
            ("Degraded Branch Fails", test_fails_on_degraded_branch(command, env, test_dir)),
        ]

        return print_summary(results)


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_analyze_change_set.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Analyze Change Set Integration Tests")

    return run_analyze_change_set_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

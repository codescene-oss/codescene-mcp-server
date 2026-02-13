#!/usr/bin/env python3
"""
Technical Debt integration tests.

Tests that the MCP server correctly returns technical debt goals and hotspots
from the CodeScene API for both cloud and on-prem environments.

These are API-backed tools that require CS_ACCESS_TOKEN (cloud) and optionally
CS_ONPREM_ACCESS_TOKEN + the on-prem URL (on-prem). The tests dynamically
discover projects — finding one WITH goals/hotspots and one WITHOUT — then
validate response structure, field types, and link URL patterns.

Cloud links:   https://codescene.io/projects/{id}/analyses/latest/code/...
On-prem links: {CS_ONPREM_URL}/{id}/analyses/latest/code/...

On-prem tests are skipped when CS_ONPREM_ACCESS_TOKEN is not set.
"""

import json
import os
import sys
from dataclasses import dataclass, field
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

KNOWN_GOAL_CATEGORIES = {"supervise", "refactor", "critical-code", "no-problem"}

ONPREM_URL = "https://test-env.enterprise.codescene.io"

# Maximum number of projects to probe during discovery before giving up.
_MAX_PROJECTS_TO_PROBE = 20


# ---------------------------------------------------------------------------
# Configuration dataclasses
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class _EnvironmentConfig:
    """Describes a target environment (cloud or on-prem) for testing."""

    label: str
    env_overrides: dict[str, str]
    cloud_link_base: str | None  # e.g. "https://codescene.io/projects" — None for on-prem
    onprem_link_base: str | None  # e.g. "https://test-env.enterprise.codescene.io" — None for cloud


@dataclass(frozen=True)
class _ServerContext:
    """Bundles the command, env, and repo_dir needed by every test."""

    command: list[str]
    env: dict[str, str]
    repo_dir: Path


@dataclass(frozen=True)
class _ToolSpec:
    """Describes one tool + data-key combination for generic test execution."""

    tool_name: str
    list_key: str  # "files" or "hotspots"
    label: str  # "Goals" or "Hotspots"


@dataclass(frozen=True)
class _TestCase:
    """A fully resolved test case: which tool, which project, and whether data is expected."""

    spec: _ToolSpec
    project_id: int
    expect_data: bool


_GOALS_SPEC = _ToolSpec(
    tool_name="list_technical_debt_goals_for_project",
    list_key="files",
    label="Goals",
)

_HOTSPOTS_SPEC = _ToolSpec(
    tool_name="list_technical_debt_hotspots_for_project",
    list_key="hotspots",
    label="Hotspots",
)


# ---------------------------------------------------------------------------
# Helpers — MCP communication
# ---------------------------------------------------------------------------


def _parse_tool_json(response: dict) -> dict | None:
    """Extract and parse JSON from an MCP tool response."""
    text = extract_result_text(response)
    if not text:
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return None


def _start_client(ctx: _ServerContext) -> MCPClient | None:
    """Create, start, and initialize an MCPClient.  Returns None on failure."""
    client = MCPClient(ctx.command, env=ctx.env, cwd=str(ctx.repo_dir))
    if not client.start():
        print_test("Server started", False)
        return None
    print_test("Server started", True)
    client.initialize()
    return client


def _call_tool(client: MCPClient, spec: _ToolSpec, project_id: int) -> dict | None:
    """Call a technical debt tool and return parsed JSON (or None)."""
    response = client.call_tool(spec.tool_name, {"project_id": project_id}, timeout=30)
    return _parse_tool_json(response)


def _get_data_list(client: MCPClient, spec: _ToolSpec, project_id: int) -> list:
    """Call a tool and return just the data list (empty list on failure)."""
    data = _call_tool(client, spec, project_id)
    return data.get(spec.list_key, []) if data else []


# ---------------------------------------------------------------------------
# Helpers — link validation
# ---------------------------------------------------------------------------


def _expected_link_prefix(env_cfg: _EnvironmentConfig, project_id: int) -> str:
    """Return the expected URL prefix for a given project.

    Cloud:   https://codescene.io/projects/{id}/analyses/latest/code
    On-prem: {base_url}/{id}/analyses/latest/code
    """
    if env_cfg.cloud_link_base:
        return f"{env_cfg.cloud_link_base}/{project_id}/analyses/latest/code"
    assert env_cfg.onprem_link_base is not None
    return f"{env_cfg.onprem_link_base}/{project_id}/analyses/latest/code"


def _validate_link(link: str, env_cfg: _EnvironmentConfig, project_id: int, label: str) -> bool:
    """Check that *link* starts with the expected prefix for the environment."""
    prefix = _expected_link_prefix(env_cfg, project_id)
    ok = link.startswith(prefix)
    print_test(f"{label} link pattern", ok, link)
    return ok


# ---------------------------------------------------------------------------
# Helpers — entry validation
# ---------------------------------------------------------------------------


def _is_valid_goal_entry(file_entry: dict) -> bool:
    """Return True if a single goal file entry has the expected shape."""
    path = file_entry.get("path")
    goals = file_entry.get("goals")
    if not isinstance(path, str) or not path:
        return False
    if not isinstance(goals, list) or len(goals) == 0:
        return False
    return all(isinstance(g.get("category"), str) and "text" in g for g in goals)


def _is_valid_hotspot_entry(hotspot: dict) -> bool:
    """Return True if a single hotspot entry has the expected numeric fields."""
    has_file_name = isinstance(hotspot.get("file_name"), str) and len(hotspot["file_name"]) > 0
    has_score = isinstance(hotspot.get("code_health_score"), int | float)
    has_revisions = isinstance(hotspot.get("revisions"), int | float)
    has_loc = isinstance(hotspot.get("loc"), int | float)
    return has_file_name and has_score and has_revisions and has_loc


def _validate_entries(items: list, spec: _ToolSpec) -> bool:
    """Validate a sample of entries using the appropriate validator for *spec*."""
    validator = _is_valid_goal_entry if spec.list_key == "files" else _is_valid_hotspot_entry
    sample = items[:5]
    ok = all(validator(entry) for entry in sample)
    print_test("Sampled entries have valid structure", ok)
    return ok


# ---------------------------------------------------------------------------
# Discovery
# ---------------------------------------------------------------------------


@dataclass
class _DiscoverySlot:
    """Tracks whether a project with/without data has been found for one tool."""

    with_data: int | None = None
    without_data: int | None = None

    @property
    def complete(self) -> bool:
        return self.with_data is not None and self.without_data is not None

    def record(self, project_id: int, has_data: bool) -> None:
        """Record a probe result, filling the first open slot it matches."""
        if has_data and self.with_data is None:
            self.with_data = project_id
        elif not has_data and self.without_data is None:
            self.without_data = project_id


@dataclass
class _DiscoveredProjects:
    """Project IDs found during discovery, split by tool and presence of data."""

    goals: _DiscoverySlot = field(default_factory=_DiscoverySlot)
    hotspots: _DiscoverySlot = field(default_factory=_DiscoverySlot)

    @property
    def complete(self) -> bool:
        return self.goals.complete and self.hotspots.complete


def _list_project_ids(client: MCPClient) -> list[int]:
    """Call select_project and return a list of project IDs."""
    response = client.call_tool("select_project", {}, timeout=30)
    data = _parse_tool_json(response)
    if data is None:
        return []
    projects = data.get("projects", [])
    return [p["id"] for p in projects if isinstance(p.get("id"), int)]


def _discover_projects(client: MCPClient) -> _DiscoveredProjects:
    """Probe projects to find ones with/without goals and hotspots.

    Stops early once all four slots are filled or after ``_MAX_PROJECTS_TO_PROBE``.
    """
    ids = _list_project_ids(client)
    print_test("Listed projects", len(ids) > 0, f"Found {len(ids)} projects")

    result = _DiscoveredProjects()

    for pid in ids[:_MAX_PROJECTS_TO_PROBE]:
        if result.complete:
            break

        if not result.goals.complete:
            result.goals.record(pid, len(_get_data_list(client, _GOALS_SPEC, pid)) > 0)

        if not result.hotspots.complete:
            result.hotspots.record(pid, len(_get_data_list(client, _HOTSPOTS_SPEC, pid)) > 0)

    return result


# ---------------------------------------------------------------------------
# Generic test implementation
# ---------------------------------------------------------------------------


def _test_tool_response(
    ctx: _ServerContext,
    env_cfg: _EnvironmentConfig,
    tc: _TestCase,
) -> bool:
    """Generic test: call a tool and validate structure, link, and data presence.

    When *tc.expect_data* is True the data list must be non-empty and entries are
    validated.  When False the list must be empty.
    """
    presence = "With Data" if tc.expect_data else "Without Data"
    header = f"Test: {env_cfg.label} {tc.spec.label} — {presence} (project {tc.project_id})"
    print_header(header)

    client = _start_client(ctx)
    if client is None:
        return False

    try:
        data = _call_tool(client, tc.spec, tc.project_id)
        if data is None:
            print_test("Response is valid JSON", False)
            return False
        print_test("Response is valid JSON", True)

        items = data.get(tc.spec.list_key, [])
        items_ok = _check_data_list(items, tc.spec, tc.expect_data)

        desc = data.get("description", "")
        has_description = isinstance(desc, str) and len(desc) > 0
        print_test("Contains 'description' string", has_description)

        link_ok = _validate_link(data.get("link", ""), env_cfg, tc.project_id, tc.spec.label)

        extra_ok = _check_extra_validations(items, tc.spec, tc.expect_data)

        return items_ok and has_description and link_ok and extra_ok

    except Exception as e:
        print_test(f"{tc.spec.label} {presence}", False, str(e))
        return False
    finally:
        client.stop()


def _check_data_list(items: list, spec: _ToolSpec, expect_data: bool) -> bool:
    """Validate the data list size and, if non-empty, entry structure."""
    is_list = isinstance(items, list)
    if expect_data:
        ok = is_list and len(items) > 0
        print_test(f"Contains non-empty '{spec.list_key}' array", ok, f"{len(items)} entries")
        if ok:
            return _validate_entries(items, spec)
        return False

    ok = is_list and len(items) == 0
    print_test(f"'{spec.list_key}' array is empty", ok, f"{len(items)} entries")
    return ok


def _check_extra_validations(items: list, spec: _ToolSpec, expect_data: bool) -> bool:
    """Run tool-specific extra checks (e.g. known categories for goals)."""
    if not expect_data or spec.list_key != "files":
        return True

    found_cats = {g.get("category") for f in items for g in f.get("goals", []) if g.get("category")}
    has_known = len(found_cats & KNOWN_GOAL_CATEGORIES) > 0
    print_test("Contains known categories", has_known, f"{sorted(found_cats)}")
    return has_known


# ---------------------------------------------------------------------------
# Test suite runner for a single environment
# ---------------------------------------------------------------------------


def _run_environment_tests(
    backend: ServerBackend,
    repo_dir: Path,
    env_cfg: _EnvironmentConfig,
) -> list[tuple[str, bool]]:
    """Run all technical debt tests for one environment (cloud or on-prem)."""
    print_header(f"Environment: {env_cfg.label}")

    base_env = backend.get_env(os.environ.copy(), repo_dir)
    base_env.update(env_cfg.env_overrides)

    ctx = _ServerContext(
        command=backend.get_command(repo_dir),
        env=base_env,
        repo_dir=repo_dir,
    )

    discovered = _run_discovery(ctx, env_cfg.label)

    results: list[tuple[str, bool]] = []
    results.extend(_collect_tool_results(ctx, env_cfg, _GOALS_SPEC, discovered.goals))
    results.extend(_collect_tool_results(ctx, env_cfg, _HOTSPOTS_SPEC, discovered.hotspots))
    return results


def _run_discovery(ctx: _ServerContext, label: str) -> _DiscoveredProjects:
    """Run the discovery phase and print a summary."""
    print_header(f"{label} — Project Discovery")
    client = _start_client(ctx)
    if client is None:
        return _DiscoveredProjects()
    try:
        discovered = _discover_projects(client)
    finally:
        client.stop()

    for spec in (_GOALS_SPEC, _HOTSPOTS_SPEC):
        slot = discovered.goals if spec is _GOALS_SPEC else discovered.hotspots
        print_test(f"{label}: project WITH {spec.label}", slot.with_data is not None, str(slot.with_data))
        print_test(f"{label}: project WITHOUT {spec.label}", slot.without_data is not None, str(slot.without_data))

    return discovered


def _collect_tool_results(
    ctx: _ServerContext,
    env_cfg: _EnvironmentConfig,
    spec: _ToolSpec,
    slot: _DiscoverySlot,
) -> list[tuple[str, bool]]:
    """Build test results for one tool (goals or hotspots), both with/without data."""
    results: list[tuple[str, bool]] = []

    for expect_data, project_id in [(True, slot.with_data), (False, slot.without_data)]:
        presence = "With Data" if expect_data else "Without Data"
        name = f"{env_cfg.label} {spec.label} — {presence}"

        if project_id is not None:
            tc = _TestCase(spec=spec, project_id=project_id, expect_data=expect_data)
            results.append((name, _test_tool_response(ctx, env_cfg, tc)))
        else:
            print_test(f"{env_cfg.label}: no project {presence.lower()} for {spec.label} — FAIL", False)
            results.append((name, False))

    return results


# ---------------------------------------------------------------------------
# Environment configs
# ---------------------------------------------------------------------------


def _cloud_config() -> _EnvironmentConfig:
    """Configuration for testing against CodeScene Cloud."""
    return _EnvironmentConfig(
        label="Cloud",
        env_overrides={},
        cloud_link_base="https://codescene.io/projects",
        onprem_link_base=None,
    )


def _onprem_config() -> _EnvironmentConfig | None:
    """Configuration for testing against the on-prem instance.

    Returns None if CS_ONPREM_ACCESS_TOKEN is not set.
    """
    token = os.environ.get("CS_ONPREM_ACCESS_TOKEN")
    if not token:
        return None
    return _EnvironmentConfig(
        label="On-Prem",
        env_overrides={
            "CS_ACCESS_TOKEN": token,
            "CS_ONPREM_URL": ONPREM_URL,
        },
        cloud_link_base=None,
        onprem_link_base=ONPREM_URL,
    )


# ---------------------------------------------------------------------------
# Public entry points
# ---------------------------------------------------------------------------


def run_technical_debt_tests(executable: Path) -> int:
    """Run all technical debt tests using a Nuitka executable."""
    backend = NuitkaBackend(executable=executable)
    return run_technical_debt_tests_with_backend(backend)


def run_technical_debt_tests_with_backend(backend: ServerBackend) -> int:
    """Run all technical debt tests using a backend."""
    with safe_temp_directory(prefix="cs_mcp_technical_debt_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        all_results: list[tuple[str, bool]] = []

        all_results.extend(_run_environment_tests(backend, repo_dir, _cloud_config()))

        onprem_cfg = _onprem_config()
        if onprem_cfg is not None:
            all_results.extend(_run_environment_tests(backend, repo_dir, onprem_cfg))
        else:
            print("\nSkipping on-prem tests (CS_ONPREM_ACCESS_TOKEN not set)")

        return print_summary(all_results)


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_technical_debt.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Technical Debt Integration Tests")
    print("\nThese tests verify the list_technical_debt_goals_for_project and")
    print("list_technical_debt_hotspots_for_project tools return valid,")
    print("structured responses from the CodeScene API.")
    print("Both cloud and on-prem environments are tested when credentials")
    print("are available.")

    return run_technical_debt_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

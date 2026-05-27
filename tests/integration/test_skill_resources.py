#!/usr/bin/env python3
"""
Skill resources integration tests.

Tests that the MCP server exposes embedded skills as MCP resources
using the skill:// URI scheme.

This test suite validates:
1. resources/list returns skill resources with correct URIs and metadata
2. resources/read returns SKILL.md content for each skill
3. resources/read returns valid JSON manifests for each skill
4. resources/templates/list returns the skill file template
5. resources/read rejects unknown skill names and invalid URIs
"""

import json
import os
import sys
from contextlib import contextmanager
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from fixtures import get_sample_files

from test_utils import (
    MCPClient,
    CargoBackend,
    ServerBackend,
    create_git_repo,
    print_header,
    print_summary,
    print_test,
    safe_temp_directory,
)

EXPECTED_SKILL_NAMES = [
    "configuring-codescene-mcp",
    "explaining-code-health",
    "guiding-refactoring-with-code-health",
    "installing-and-activating-codescene-mcp",
    "making-the-business-case-for-code-health",
    "prioritizing-technical-debt",
    "risk-based-testing-with-code-health",
    "routing-work-with-code-ownership",
    "safeguarding-ai-generated-code",
]


@contextmanager
def _mcp_session(command, env, repo_dir):
    """Start an MCP client, initialize it, and yield it. Stops on exit."""
    client = MCPClient(command, env=env, cwd=str(repo_dir))
    try:
        if not client.start():
            print_test("Server started", False)
            yield None
            return
        print_test("Server started", True)
        client.initialize()
        yield client
    finally:
        client.stop()


def run_skill_resources_tests(executable: Path) -> int:
    backend = CargoBackend(executable=executable)
    return run_skill_resources_tests_with_backend(backend)


def run_skill_resources_tests_with_backend(backend: ServerBackend) -> int:
    with safe_temp_directory(prefix="cs_mcp_skill_resources_test_") as test_dir:
        print(f"\nTest directory: {test_dir}")

        print("\nCreating test repository...")
        repo_dir = create_git_repo(test_dir, get_sample_files())
        print(f"Repository: {repo_dir}")

        command = backend.get_command(repo_dir)
        env = backend.get_env(os.environ.copy(), repo_dir)

        results = [
            ("Initialize reports resources capability", test_init_capabilities(command, env, repo_dir)),
            ("List resources returns all skills", test_list_resources(command, env, repo_dir)),
            ("Read SKILL.md resource", test_read_skill_md(command, env, repo_dir)),
            ("Read manifest resource", test_read_manifest(command, env, repo_dir)),
            ("List resource templates", test_list_resource_templates(command, env, repo_dir)),
            ("Read error cases", test_read_error_cases(command, env, repo_dir)),
            ("list_skills tool", test_list_skills_tool(command, env, repo_dir)),
            ("get_skill_manifest tool", test_get_skill_manifest_tool(command, env, repo_dir)),
            ("download_skill tool", test_download_skill_tool(command, env, repo_dir)),
            ("sync_skills tool", test_sync_skills_tool(command, env, repo_dir)),
        ]

        return print_summary(results)


def test_init_capabilities(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test that initialize response advertises resources capability."""
    print_header("Test: Initialize Reports Resources Capability")

    # This test needs raw client access (pre-initialize) so doesn't use _mcp_session
    client = MCPClient(command, env=env, cwd=str(repo_dir))
    try:
        if not client.start():
            print_test("Server started", False)
            return False
        print_test("Server started", True)

        response = client.initialize()
        capabilities = response.get("result", {}).get("capabilities", {})
        has_resources = "resources" in capabilities
        print_test("Resources capability advertised", has_resources, f"capabilities keys: {list(capabilities.keys())}")
        return has_resources

    except Exception as e:
        print_test("Initialize capabilities", False, str(e))
        return False
    finally:
        client.stop()


def _verify_skill_md_metadata(resources: list[dict]) -> bool:
    """Check mime type and description on a sample SKILL.md resource."""
    sample = next((r for r in resources if r["uri"].endswith("/SKILL.md")), None)
    if not sample:
        print_test("Sample SKILL.md resource found", False)
        return False
    has_mime = sample.get("mimeType") == "text/markdown"
    print_test("SKILL.md has text/markdown mime type", has_mime, f"mimeType: {sample.get('mimeType')}")
    has_desc = bool(sample.get("description"))
    print_test("SKILL.md has description", has_desc)
    return has_mime and has_desc


def _verify_expected_uris(resources: list[dict]) -> tuple[bool, bool, bool]:
    """Verify resource count and expected URIs. Returns (count_ok, skills_ok, manifests_ok)."""
    expected_count = len(EXPECTED_SKILL_NAMES) * 2
    has_correct_count = len(resources) == expected_count
    print_test(f"Resource count matches ({expected_count})", has_correct_count, f"Actual: {len(resources)}")

    skill_md_uris = {r["uri"] for r in resources if r["uri"].endswith("/SKILL.md")}
    all_skills_present = all(f"skill://{name}/SKILL.md" in skill_md_uris for name in EXPECTED_SKILL_NAMES)
    print_test("All skill SKILL.md URIs present", all_skills_present)

    manifest_uris = {r["uri"] for r in resources if r["uri"].endswith("/_manifest")}
    all_manifests_present = all(f"skill://{name}/_manifest" in manifest_uris for name in EXPECTED_SKILL_NAMES)
    print_test("All skill _manifest URIs present", all_manifests_present)

    return has_correct_count, all_skills_present, all_manifests_present


def test_list_resources(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test that resources/list returns all embedded skills."""
    print_header("Test: List Resources Returns All Skills")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        response = client.send_request("resources/list")
        resources = response.get("result", {}).get("resources", [])
        count_ok, skills_ok, manifests_ok = _verify_expected_uris(resources)
        metadata_ok = _verify_skill_md_metadata(resources)
        return count_ok and skills_ok and manifests_ok and metadata_ok


def test_read_skill_md(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test reading a SKILL.md resource returns valid content."""
    print_header("Test: Read SKILL.md Resource")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        uri = "skill://safeguarding-ai-generated-code/SKILL.md"
        response = client.send_request("resources/read", {"uri": uri})
        contents = response.get("result", {}).get("contents", [])

        has_content = len(contents) > 0
        print_test("Response has contents", has_content)
        if not has_content:
            return False

        text = contents[0].get("text", "")
        has_text = len(text) > 50
        print_test("Content has substantial text", has_text, f"Length: {len(text)} chars")
        print_test("Content includes frontmatter", "---" in text)
        print_test("Content is skill-related", "code health" in text.lower() or "safeguard" in text.lower())
        return has_text


def _verify_manifest_files(files: list[dict]) -> bool:
    """Verify the files array in a manifest response."""
    has_files = len(files) == 1
    print_test("Manifest lists one file", has_files, f"Count: {len(files)}")
    if not files:
        return False

    f = files[0]
    has_path = f.get("path") == "SKILL.md"
    print_test("File path is SKILL.md", has_path)

    size = f.get("size", 0)
    print_test("File has positive size", size > 0, f"Size: {size}")

    hash_val = f.get("hash", "")
    print_test("File has sha256 hash", hash_val.startswith("sha256:"), f"Hash: {hash_val[:20]}...")

    return has_files and has_path


def test_read_manifest(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test that reading a _manifest resource returns valid JSON metadata."""
    print_header("Test: Read Manifest Resource")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        skill_name = "safeguarding-ai-generated-code"
        uri = f"skill://{skill_name}/_manifest"
        response = client.send_request("resources/read", {"uri": uri})
        contents = response.get("result", {}).get("contents", [])

        has_content = len(contents) > 0
        print_test("Response has contents", has_content)
        if not has_content:
            return False

        text = contents[0].get("text", "")
        try:
            manifest = json.loads(text)
        except json.JSONDecodeError:
            print_test("Manifest is valid JSON", False, f"Text: {text[:200]}")
            return False
        print_test("Manifest is valid JSON", True)

        has_skill_field = manifest.get("skill") == skill_name
        print_test("Manifest has correct skill name", has_skill_field)
        files_ok = _verify_manifest_files(manifest.get("files", []))
        return has_content and has_skill_field and files_ok


def test_list_resource_templates(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test that resources/templates/list returns the skill template."""
    print_header("Test: List Resource Templates")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        response = client.send_request("resources/templates/list")
        templates = response.get("result", {}).get("resourceTemplates", [])

        has_templates = len(templates) > 0
        print_test("Templates returned", has_templates, f"Count: {len(templates)}")
        if not has_templates:
            return False

        uri_template = templates[0].get("uriTemplate", "")
        has_skill_template = "skill://" in uri_template and "{skill_name}" in uri_template
        print_test("Template has skill:// URI pattern", has_skill_template, f"URI: {uri_template}")
        return has_skill_template


def test_read_error_cases(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test that reading invalid or unknown URIs returns errors."""
    print_header("Test: Read Error Cases")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        error_cases = [
            ("skill://nonexistent-skill/SKILL.md", "unknown skill"),
            ("file:///etc/passwd", "non-skill URI"),
        ]

        all_passed = True
        for uri, label in error_cases:
            response = client.send_request("resources/read", {"uri": uri})
            has_error = "error" in response
            print_test(f"Error returned for {label}", has_error)
            all_passed = all_passed and has_error

        return all_passed


def test_list_skills_tool(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test the list_skills tool returns all embedded skills."""
    print_header("Test: list_skills Tool")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        content = _extract_tool_text(client.call_tool("list_skills", {}))
        if content is None:
            print_test("Tool returned content", False)
            return False
        print_test("Tool returned content", True)

        has_count = f"Available skills ({len(EXPECTED_SKILL_NAMES)})" in content
        print_test("Lists correct skill count", has_count)

        has_skill = "safeguarding-ai-generated-code" in content
        print_test("Contains expected skill name", has_skill)
        return has_count and has_skill


def test_get_skill_manifest_tool(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test the get_skill_manifest tool returns valid JSON."""
    print_header("Test: get_skill_manifest Tool")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        skill_name = "safeguarding-ai-generated-code"
        content = _extract_tool_text(client.call_tool("get_skill_manifest", {"skill_name": skill_name}))
        if content is None:
            print_test("Tool returned content", False)
            return False
        print_test("Tool returned content", True)

        manifest = json.loads(content)
        has_name = manifest.get("skill") == skill_name
        print_test("Manifest has correct skill name", has_name)

        has_files = len(manifest.get("files", [])) == 1
        print_test("Manifest lists one file", has_files)
        return has_name and has_files


def test_download_skill_tool(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test the download_skill tool writes SKILL.md to disk."""
    print_header("Test: download_skill Tool")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        dest = repo_dir / "download_test"
        skill_name = "safeguarding-ai-generated-code"
        content = _extract_tool_text(client.call_tool("download_skill", {
            "skill_name": skill_name,
            "destination_dir": str(dest),
        }))
        has_content = content is not None and "Downloaded" in content
        print_test("Tool reports success", has_content)

        file_exists = (dest / skill_name / "SKILL.md").exists()
        print_test("SKILL.md written to disk", file_exists)
        return has_content and file_exists


def test_sync_skills_tool(command: list[str], env: dict, repo_dir: Path) -> bool:
    """Test the sync_skills tool downloads all skills."""
    print_header("Test: sync_skills Tool")

    with _mcp_session(command, env, repo_dir) as client:
        if client is None:
            return False
        dest = repo_dir / "sync_test"
        content = _extract_tool_text(client.call_tool("sync_skills", {"destination_dir": str(dest)}))
        has_content = content is not None and "Downloaded" in content
        print_test("Tool reports success", has_content)

        dirs = [d for d in dest.iterdir() if d.is_dir()] if dest.exists() else []
        all_synced = len(dirs) == len(EXPECTED_SKILL_NAMES)
        print_test(f"All {len(EXPECTED_SKILL_NAMES)} skills synced", all_synced, f"Actual: {len(dirs)}")
        return has_content and all_synced


def _extract_tool_text(response: dict) -> str | None:
    """Extract text content from a tools/call response."""
    content = response.get("result", {}).get("content", [])
    if not content:
        return None
    return content[0].get("text")


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python test_skill_resources.py /path/to/cs-mcp")
        return 1

    executable = Path(sys.argv[1])
    if not executable.exists():
        print(f"Error: Executable not found: {executable}")
        return 1

    print_header("Skill Resources Integration Tests")
    print("\nThese tests verify that embedded skills are exposed")
    print("as MCP resources using the skill:// URI scheme.")

    return run_skill_resources_tests(executable)


if __name__ == "__main__":
    sys.exit(main())

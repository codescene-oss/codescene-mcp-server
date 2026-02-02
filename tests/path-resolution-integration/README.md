# Path Resolution Integration Tests

This directory contains integration tests for verifying that MCP tools
correctly resolve file paths in both Docker and static executable modes.

## Background

The following MCP tools require file path resolution to convert absolute
paths to relative paths for CodeScene API calls:

- `code_ownership_for_path`
- `list_technical_debt_hotspots_for_project_file`
- `list_technical_debt_goals_for_project_file`

These tools previously only worked in Docker mode (with `CS_MOUNT_PATH` set)
and would fail with "CS_MOUNT_PATH not defined" error in static executable mode.

The fix introduces a helper function `get_relative_file_path_for_api()` that:
- In **Docker mode** (`CS_MOUNT_PATH` set): Uses the mount path to translate paths
- In **Static mode** (no `CS_MOUNT_PATH`):
  - If path is relative: Returns as-is
  - If path is absolute and in a git repo: Uses git root detection to compute relative paths
  - If path is absolute and NOT in a git repo: Returns path unchanged (graceful fallback)

## Test Files

### `test_static_variant.py`

Tests the static executable variant (without `CS_MOUNT_PATH`).

**Usage:**
```bash
python test_static_variant.py /path/to/cs-mcp
```

**What it tests:**
1. MCP server starts successfully without `CS_MOUNT_PATH`
2. `code_ownership_for_path` works with files in a git repository
3. `list_technical_debt_hotspots_for_project_file` works with files in a git repository
4. `list_technical_debt_goals_for_project_file` works with files in a git repository
5. `code_ownership_for_path` works with files OUTSIDE a git repository (no error)
6. `code_ownership_for_path` works with relative paths (no git/mount requirements)

The test creates both a temporary git repository and a non-git directory
to verify both scenarios work correctly.

### `test_docker_run.py`

Tests the Docker variant (with `CS_MOUNT_PATH` set).

**Usage:**
```bash
# With default image name (codescene-mcp)
python test_docker_run.py

# With custom image
DOCKER_IMAGE=my-codescene-mcp python test_docker_run.py

# With custom test data path
TEST_DATA_PATH=/path/to/test/data python test_docker_run.py
```

**Environment variables:**
- `DOCKER_IMAGE`: Docker image name to test (default: `codescene-mcp`)
- `TEST_DATA_PATH`: Path to test data files (creates temp if not set)

**What it tests:**
1. Docker container starts successfully with `CS_MOUNT_PATH` set
2. `code_ownership_for_path` works with path translation
3. `list_technical_debt_hotspots_for_project_file` works with path translation
4. `list_technical_debt_goals_for_project_file` works with path translation

## Running Both Tests

To ensure the fix works in both modes, run both test files:

```bash
# Test static executable mode
python tests/path-resolution-integration/test_static_variant.py ./dist/cs-mcp

# Test Docker mode
python tests/path-resolution-integration/test_docker_run.py
```

## Note

These tests verify that the path resolution doesn't fail with the
`CS_MOUNT_PATH not defined` error. They may still return API errors
(e.g., authentication errors) if proper CodeScene credentials are not
configured, but that's expected - the key verification is that the
path resolution step succeeds.

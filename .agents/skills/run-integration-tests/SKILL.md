---
name: run-integration-tests
description: Run the end-to-end integration test suite for the CodeScene MCP Server. Use when you need to verify that all MCP tools work correctly against the real CLI and server infrastructure.
metadata:
  audience: contributors
  language: python
---

## Purpose

Use this skill when you need to run the full integration test suite to verify MCP server behavior end-to-end. This is distinct from `cargo test` (unit tests) — integration tests launch the compiled MCP server binary, communicate over JSON-RPC stdio, and exercise real tool calls.

## Prerequisites

- Rust toolchain (`cargo`) — the suite builds the server before running tests
- `CS_ACCESS_TOKEN` environment variable set with a valid CodeScene access token
- Python 3.10+ (no pip dependencies required)
- Docker (only for `--docker` backend)

## How to Run

### Full suite (Cargo backend — recommended for CI and pre-PR)

```bash
make test-integration
```

This is equivalent to:

```bash
./tests/run-integration-tests.sh
```

It builds the release binary (`cargo build --release`) and runs all registered test modules.

### Full suite (Docker backend — faster rebuild cycle)

```bash
./tests/run-integration-tests.sh --docker
```

### Skip the build step (iterative development)

If you already have a built binary and want to re-run tests without rebuilding:

```bash
./tests/run-integration-tests.sh --executable target/release/cs-mcp
```

### Run a single test module standalone

Each test file can run independently with a pre-built executable:

```bash
python tests/integration/test_<module>.py target/release/cs-mcp
```

## Test Modules

| Module | What it tests |
|---|---|
| `test_code_health.py` | `code_health_score` and `code_health_review` tools |
| `test_business_case.py` | `code_health_refactoring_business_case` tool |
| `test_error_logging.py` | Error telemetry redaction and file logging |
| `test_pre_commit.py` | `pre_commit_code_health_safeguard` tool |
| `test_change_set.py` | `analyze_change_set` tool |
| `test_skills.py` | Skill resources, listing, downloading, and syncing |

All modules are registered in `tests/integration/run_all_tests.py`.

## Infrastructure

| File | Role |
|---|---|
| `tests/integration/mcp_client.py` | `MCPClient` — starts MCP server subprocess, JSON-RPC over stdio |
| `tests/integration/server_backends.py` | `CargoBackend`, `DockerBackend` abstractions |
| `tests/integration/file_utils.py` | `create_git_repo()`, `safe_temp_directory()` |
| `tests/integration/response_parsers.py` | `extract_result_text()`, `extract_code_health_score()` |
| `tests/integration/test_output.py` | `print_header()`, `print_test()`, `print_summary()` |
| `tests/integration/fixtures.py` | Sample code with known Code Health characteristics |
| `tests/integration/test_utils.py` | Re-exports everything above |

## Interpreting Results

Tests print formatted output with pass/fail indicators:

```
✓ PASS: Server started
✓ PASS: Tool returned content
✗ FAIL: Response contains expected data
```

The final summary shows total/passed/failed counts. Exit code is `0` if all pass, `1` otherwise.

## Troubleshooting

- **`CS_ACCESS_TOKEN` not set**: Export it before running. Some tests (e.g., `test_error_logging.py`) intentionally use invalid tokens but still need the env var set for the server to start.
- **Build failures**: Run `cargo build --release` separately first to isolate build errors from test errors.
- **Timeout on tool calls**: The default timeout is 60s per tool call. If the CodeScene API is slow, tests may time out — this is not a code bug.
- **Docker tests fail to connect**: Ensure Docker is running and the `host.docker.internal` hostname resolves (standard on Docker Desktop).

---
name: run-e2e-tests
description: Run the end-to-end integration test suite for the CodeScene MCP Server. Use when you need to verify that all MCP tools work correctly against the real CLI and server infrastructure.
metadata:
  audience: contributors
  language: rust
---

## Purpose

Use this skill when you need to run the full e2e test suite to verify MCP server behavior end-to-end. These tests launch the compiled MCP server binary, communicate over JSON-RPC stdio, and exercise real tool calls against the CodeScene API.

## Prerequisites

- Rust toolchain (`cargo`)
- `CS_ACCESS_TOKEN` environment variable set with a valid CodeScene access token
- `git` available in PATH
- Docker (only for Docker backend)

## How to Run

### Full suite (default — static/Cargo backend)

```bash
cargo test --test e2e
```

This builds the release binary (or uses `CS_MCP_EXECUTABLE` if set) and runs all registered test modules.

### Run a specific test

```bash
cargo test --test e2e test_business_case_basic_response
```

### Run all tests matching a pattern

```bash
cargo test --test e2e test_analytics
```

### Include ignored tests (e.g., stress tests)

```bash
cargo test --test e2e -- --ignored
```

### Run with a specific backend

```bash
# Static/Cargo backend (default)
CS_MCP_BACKEND=static cargo test --test e2e

# Docker backend
CS_MCP_BACKEND=docker cargo test --test e2e

# npm backend
CS_MCP_BACKEND=npm cargo test --test e2e
```

### Use a pre-built executable (skip build)

```bash
CS_MCP_EXECUTABLE=target/release/cs-mcp cargo test --test e2e
```

## Test Modules

| Module | What it tests |
|---|---|
| `analyze_change_set` | `analyze_change_set` tool |
| `analytics_environment_override` | `CS_ENVIRONMENT` override |
| `analytics_tracking` | Analytics event sending and enrichment |
| `bundled_docs` | `explain_code_health` and `explain_code_health_productivity` tools |
| `business_case` | `code_health_refactoring_business_case` tool |
| `cloudfront_headers` | CloudFront API client headers |
| `configure` | `get_config` and `set_config` tools |
| `enabled_tools` | Tool filtering via `CS_ENABLED_TOOLS` |
| `error_logging` | Error telemetry redaction and file logging |
| `git_subtree` | Git subtree repository support |
| `git_worktree` | Git worktree repository support |
| `platform_specific` | Path handling (absolute, relative, symlinks, spaces, unicode) |
| `relative_paths` | Relative path resolution |
| `require_access_token` | Access token validation |
| `shutdown_during_handshake` | Graceful shutdown (stdin close + SIGTERM) |
| `skill_resources` | Skill resources, listing, reading, and syncing |
| `ssl_api_ca_bundle` | SSL CA bundle configuration for API calls |
| `ssl_cli_truststore` | SSL truststore args for CLI |
| `standalone_license` | Standalone license tool filtering |
| `stress_code_health_review` | Stress test (250 iterations, `#[ignore]`) |
| `version_check` | Version check tool and background fetch |

All modules are declared in `tests/e2e/tests/mod.rs` and wrappers are in `tests/e2e/main.rs`.

## Infrastructure

| File | Role |
|---|---|
| `tests/e2e/main.rs` | Entry point, `#[test]` wrappers, `setup()`, `find_or_build_executable()`, `make_client()` |
| `tests/e2e/tests/mod.rs` | Module declarations and infrastructure re-exports |
| `tests/e2e/mcp_client.rs` | `MCPClient` — JSON-RPC over stdio |
| `tests/e2e/server_backends.rs` | `ServerBackend` trait + 3 backend implementations |
| `tests/e2e/file_utils.rs` | `create_git_repo()`, `create_temp_dir()` |
| `tests/e2e/response_parsers.rs` | `extract_result_text()`, `extract_code_health_score()` |
| `tests/e2e/fixtures.rs` | Sample code with known Code Health characteristics |
| `tests/e2e/tests/fake_http_server.rs` | `FakeHttpServer` for intercepting API calls |
| `tests/e2e/tests/fake_https_server.rs` | `FakeHttpsServer` for SSL tests |

## Interpreting Results

Standard `cargo test` output with pass/fail per test:

```
test test_business_case_basic_response ... ok
test test_analytics_events_are_sent ... ok
test test_platform_symlinks ... ignored  (Docker)
```

Exit code is `0` if all pass, non-zero otherwise.

## Troubleshooting

- **`CS_ACCESS_TOKEN` not set**: Export it before running. Some tests intentionally use invalid tokens but the env var must exist.
- **Build failures**: Run `cargo build --release` separately to isolate build errors.
- **Timeout on tool calls**: Default timeout is 60s per tool call. CodeScene API latency may cause timeouts — not a code bug.
- **Docker tests fail**: Ensure Docker is running and `host.docker.internal` resolves.
- **Tests marked `ignored`**: Stress tests are `#[ignore]` by default. Run with `--ignored` to include them.
- **Platform-specific tests skip in Docker**: Tests using `skip_if_docker()` print a message and return early — this is expected.

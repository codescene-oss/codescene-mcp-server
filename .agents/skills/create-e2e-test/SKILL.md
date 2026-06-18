---
name: create-e2e-test
description: Write an end-to-end integration test for the CodeScene MCP Server, covering file structure, backend abstraction, MCPClient usage, test registration, and verification.
metadata:
  audience: contributors
  language: rust
---

## Purpose

Use this skill when adding an end-to-end integration test for a new or existing MCP tool or feature. It encodes the exact conventions, infrastructure, and file structure that all existing e2e tests follow.

## Project Context

- **MCP Server:** Rust binary built with `cargo build --release`
- **Test framework:** Rust integration tests via `cargo test --test e2e`
- **Test location:** `tests/e2e/`
- **Entry point:** `tests/e2e/main.rs` — contains `#[test]` wrapper functions and shared setup
- **Module index:** `tests/e2e/tests/mod.rs` — declares all test modules and re-exports infrastructure
- **Backend abstraction:** `ServerBackend` trait with three implementations: `CargoBackend` (static binary), `DockerBackend` (container), `NpmBackend` (npm package). Every test must work with all backends.
- **Backend selection:** `CS_MCP_BACKEND` env var (`static` / `docker` / `npm`); `create_backend()` factory
- **Key environment variables:** `CS_ACCESS_TOKEN` (required), `CS_MCP_EXECUTABLE` (optional override), `CS_MCP_BACKEND`

### Infrastructure modules (all in `tests/e2e/`)

| Module | Role |
|---|---|
| `mcp_client.rs` | `MCPClient` — starts the MCP server as a subprocess, communicates via JSON-RPC over stdio |
| `server_backends.rs` | `ServerBackend` trait, `CargoBackend`, `DockerBackend`, `NpmBackend`, `create_backend()`, `base_env()`, `is_docker()`, `skip_if_docker()` |
| `file_utils.rs` | `create_git_repo()`, `create_temp_dir()` |
| `response_parsers.rs` | `extract_result_text()`, `extract_code_health_score()` |
| `fixtures.rs` | Sample code constants with known Code Health characteristics and expected score ranges |

### Re-exports via `tests/mod.rs`

Test modules use `use super::*;` to get all infrastructure via the re-exports in `tests/mod.rs`:

```rust
pub use crate::file_utils::{create_git_repo, create_temp_dir};
pub use crate::fixtures::get_sample_files;
pub use crate::mcp_client::MCPClient;
pub use crate::response_parsers::{extract_code_health_score, extract_result_text};
pub use crate::server_backends::{
    base_env, create_backend, docker_config_dir, fake_server_bind_host,
    fake_server_url_host, is_docker, skip_if_docker, ServerBackend,
};
pub use crate::{find_or_build_executable, make_client, setup};
pub use serde_json::json;
pub use std::path::Path;
pub use std::time::Duration;
```

## Step-by-Step

### 1. Create the test file

Create `tests/e2e/tests/<feature>.rs` with the standard boilerplate:

```rust
//! <Feature> integration tests.
//!
//! Tests that the MCP server correctly <what this validates>.
//!
//! Validates:
//! - <First thing>
//! - <Second thing>

use super::*;
```

Conventions:
- The module docstring (`//!`) should explain **what** the test validates and **why**.
- `use super::*;` imports all infrastructure via `tests/mod.rs` re-exports.
- Define constants for tool names, timeouts, expected terms, and error patterns at the top.

### 2. Add fixtures if needed

If your test requires code samples with specific Code Health characteristics that the existing fixtures do not cover, add them to `fixtures.rs`:

```rust
const NEW_SAMPLE_CODE: &str = r#""""
Module docstring.
"""

def example():
    pass
"#;
```

Then update `get_sample_files()` and optionally `get_expected_scores()` if the new sample should be part of the standard test repository.

Skip this step if the existing fixtures are sufficient.

### 3. Add helper functions to reduce duplication

Extract shared setup logic into helper functions within the test module:

```rust
const TOOL_NAME: &str = "tool_name";
const TIMEOUT: Duration = Duration::from_secs(60);

fn call_tool(client: &mut MCPClient, repo_dir: &Path) -> String {
    let test_file = repo_dir.join("src/utils/calculator.py");
    let response = client
        .call_tool(
            TOOL_NAME,
            json!({"file_path": test_file.to_string_lossy()}),
            TIMEOUT,
        )
        .expect("Tool call should succeed");
    extract_result_text(&response)
}

fn setup_and_call(command: &[String], env: &[(String, String)], repo_dir: &Path) -> String {
    let mut client = make_client(command, env, repo_dir);
    assert!(client.start(), "Server should start");
    client.initialize().expect("Initialize should succeed");
    call_tool(&mut client, repo_dir)
}
```

### 4. Implement individual test functions

Each test function is a `pub fn` (not `#[test]`) — the `#[test]` wrappers live in `main.rs`:

```rust
pub fn test_feature_basic_response() {
    let (command, env, repo_dir, _tmp) = setup();
    let result_text = setup_and_call(&command, &env, &repo_dir);

    assert!(
        !result_text.is_empty(),
        "Tool should return content"
    );
}

pub fn test_feature_contains_expected_data() {
    let (command, env, repo_dir, _tmp) = setup();
    let result_text = setup_and_call(&command, &env, &repo_dir);
    let lower = result_text.to_lowercase();

    let expected_terms = &["term1", "term2", "term3"];
    let found = expected_terms.iter().filter(|t| lower.contains(*t)).count();

    assert!(found >= 2, "Expected at least 2 terms, found {found}");
}

pub fn test_feature_no_errors() {
    let (command, env, repo_dir, _tmp) = setup();
    let result_text = setup_and_call(&command, &env, &repo_dir);
    let lower = result_text.to_lowercase();

    let error_patterns = &["traceback", "no such file", "os error 2"];
    for pattern in error_patterns {
        assert!(
            !lower.contains(pattern),
            "Response must not contain '{pattern}': {result_text}"
        );
    }
}
```

Conventions:
- **Functions are `pub fn`, not `#[test]`** — the `#[test]` attribute goes on the wrapper in `main.rs`.
- **Always hold `_tmp` (`TempDir`)** — dropping it deletes the temp directory. The variable must live until the test completes.
- **Use `setup()`** for the standard git-repo-based test setup.
- **Use `make_client()`** to create an `MCPClient` from command/env/cwd.
- **Use `Duration::from_secs(60)`** for tool call timeouts.
- **Use `extract_result_text()`** to parse JSON-RPC responses — never parse manually.
- **Docker-only considerations:** Use `skip_if_docker()` at the start of tests that cannot run in Docker (e.g., filesystem path tests). Use `fake_server_bind_host()`/`fake_server_url_host()` for tests that start HTTP servers.

### 5. Register the module in `tests/mod.rs`

Add the module declaration to `tests/e2e/tests/mod.rs`:

```rust
pub mod <feature>;
```

Keep modules in alphabetical order.

### 6. Add `#[test]` wrappers in `main.rs`

Add wrapper functions in `tests/e2e/main.rs` under a comment section header:

```rust
// --- Feature Name ---
#[test]
fn test_feature_basic_response() {
    tests::feature::test_feature_basic_response();
}

#[test]
fn test_feature_contains_expected_data() {
    tests::feature::test_feature_contains_expected_data();
}

#[test]
fn test_feature_no_errors() {
    tests::feature::test_feature_no_errors();
}
```

Conventions:
- The wrapper function name matches the test function name exactly.
- Each wrapper is a one-liner that delegates to the module function.
- Group wrappers under a `// --- Feature Name ---` comment.
- Place new sections in logical order relative to existing tests.
- For ignored tests (e.g., stress tests), add `#[ignore]` above `fn`.

## Canonical Example: `business_case.rs`

The simplest existing test to reference is `tests/e2e/tests/business_case.rs`. It demonstrates:
- Three focused test functions with the standard `setup()` + `setup_and_call()` pattern
- Constants for tool name, timeout, expected terms, and error patterns
- Helper functions to eliminate duplication
- Clean assertion messages

Refer to `business_case.rs` as the minimal template for any new test module.

## Verification

Run the e2e test suite to verify your new test is picked up and passes:

```bash
# Run all e2e tests (builds release binary first)
cargo test --test e2e

# Run a specific test by name
cargo test --test e2e test_feature_basic_response

# Run all tests matching a pattern
cargo test --test e2e test_feature
```

**Prerequisites:**
- Rust toolchain (`cargo`)
- `CS_ACCESS_TOKEN` environment variable set
- `git` in PATH

## Validate Code Health and Fix Any Issues

After the test is written and passing, run the CodeScene Code Health tools to ensure the new test code meets quality standards. **Do not consider the test complete until Code Health passes without regressions.**

| Scope | Tool | When to use |
|---|---|---|
| Single file | `code_health_review` | Quick check on the test file you created or modified |
| Staged + unstaged changes | `pre_commit_code_health_safeguard` | Before committing |
| Full branch vs base | `analyze_change_set` | Before opening a PR |

Recommended workflow:

1. Run `code_health_review` on each new/modified file.
2. If any code smells or regressions are reported, refactor and re-check.
3. Before committing, run `pre_commit_code_health_safeguard`.
4. Before opening a PR, run `analyze_change_set` against the target branch.

**Target: Code Health 10.0.** Scores of 9+ are not "good enough" — aim for optimal.

## Checklist

Before considering the test complete:

- [ ] Test file created at `tests/e2e/tests/<feature>.rs`
- [ ] Module docstring (`//!`) explains what the test validates and why
- [ ] `use super::*;` imports infrastructure
- [ ] Constants defined for tool names, timeouts, expected terms
- [ ] Helper functions extract shared logic to avoid duplication
- [ ] Each test function is `pub fn` (not `#[test]`)
- [ ] `_tmp` (`TempDir`) held alive for the duration of each test
- [ ] Module declared in `tests/e2e/tests/mod.rs` (alphabetical order)
- [ ] `#[test]` wrappers added in `tests/e2e/main.rs` under a section comment
- [ ] Fixtures added to `fixtures.rs` if new code samples needed
- [ ] `cargo test --test e2e test_<feature>` passes
- [ ] `code_health_review` passes on all new/modified files
- [ ] `pre_commit_code_health_safeguard` reports no regressions

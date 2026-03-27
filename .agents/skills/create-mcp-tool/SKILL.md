---
name: create-mcp-tool
description: Scaffold a new MCP tool for the CodeScene MCP Server following the project's established patterns for directory structure, dependency injection, decorators, testing, and registration.
metadata:
  audience: contributors
  language: rust
---

## Purpose

Use this skill when creating a new MCP tool in the CodeScene MCP Server project. It encodes the exact conventions, patterns, and file structure that all existing tools follow.

## Project Context

- **Framework:** `rmcp` (Rust MCP SDK) with `#[tool(tool_box)]` macros
- **Language:** Rust
- **Source root:** `src/`
- **Server entry point:** `src/main.rs` (contains `CodeSceneServer` struct and tool method bindings)
- **Tools directory:** `src/tools/` with `mod.rs` for module declarations and parameter type definitions
- **Dependency injection:** Via the `CodeSceneServer` struct, which holds `cli_runner: Arc<dyn CliRunner>` and `http_client: Arc<dyn HttpClient>` trait objects

## Step-by-Step

### 1. Create the tool file

Create a new `.rs` file in `src/tools/` named after the tool using snake_case:

```
src/tools/<tool_name>.rs
```

### 2. Define the parameter struct (if needed)

If the tool needs parameters beyond what already exists, add a new struct to `src/tools/mod.rs`:

```rust
/// Parameters for <description of what the tool does>.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MyToolParam {
    /// Description of this field for the JSON Schema (visible to LLM clients).
    pub some_field: String,

    /// Optional fields use Option<T> with #[serde(default)].
    #[serde(default)]
    pub optional_field: Option<String>,
}
```

Existing parameter structs that may already fit your tool:

| Struct | Fields | When to use |
|---|---|---|
| `FilePathParam` | `file_path: String` | Single-file analysis tools |
| `GitRepoParam` | `git_repository_path: String` | Repository-level tools |
| `ChangeSetParam` | `base_ref: String`, `git_repository_path: String` | Branch diff tools |
| `RefactorParam` | `file_path: String`, `function_name: String` | Function-level refactoring |
| `ProjectParam` | `project_id: i64` | Project-scoped API tools |
| `ProjectFileParam` | `file_path: String`, `project_id: i64` | Project + file API tools |
| `OptionalContext` | `context: Option<String>` | Tools that take no meaningful input |
| `GetConfigParam` | `key: Option<String>` | Config read |
| `SetConfigParam` | `key: String`, `value: String` | Config write |

Reuse an existing struct when possible. Only define a new one if no existing struct matches.

### 3. Add the module declaration

Add a `pub mod` line in `src/tools/mod.rs`, keeping the list alphabetically sorted:

```rust
pub mod my_tool;
```

### 4. Implement the tool handler

Create `src/tools/<tool_name>.rs` with a `handle` function:

```rust
use std::path::Path;

use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData;

use crate::docker;
use crate::event_properties;
use crate::tools::common::{run_review, tool_error};
use crate::tools::FilePathParam;
use crate::CodeSceneServer;

pub(crate) async fn handle(
    server: &CodeSceneServer,
    params: FilePathParam,
) -> Result<CallToolResult, ErrorData> {
    // 1. Check for access token
    if let Some(r) = server.require_token() {
        return Ok(r);
    }

    // 2. Trigger background version check
    server.version_checker.check_in_background();

    // 3. Adapt paths for Docker if needed
    let file_path = docker::adapt_path_for_docker(Path::new(&params.file_path));

    // 4. Call the CLI or HTTP client via the server's injected dependencies
    let result = run_review(Path::new(&file_path), &*server.cli_runner).await;

    // 5. Handle the result
    match result {
        Ok(output) => {
            // Track success event
            let props = event_properties::score_properties(Path::new(&params.file_path), None);
            server.track("my-tool", props);

            // Format and return
            let text = server.maybe_version_warning(&output).await;
            Ok(CallToolResult::success(vec![Content::text(text)]))
        }
        Err(e) => {
            server.track_err("my-tool", &e.to_string());
            Ok(tool_error(&format!("Error: {e}")))
        }
    }
}
```

Key conventions:
- **The handler is a free `async fn`**, not a method on a struct. It receives `&CodeSceneServer` and the parameter struct.
- **Always check for the access token first** with `server.require_token()`.
- **Always trigger the background version check** with `server.version_checker.check_in_background()`.
- **Use `docker::adapt_path_for_docker()`** when the tool receives file paths from the client.
- **Track events** with `server.track()` on success and `server.track_err()` on failure.
- **Use `server.maybe_version_warning()`** to prepend version update notices to the response.
- **Return `tool_error()`** from `crate::tools::common` for error results (sets `is_error = true`).
- **Return type is always `Result<CallToolResult, ErrorData>`.**

Choose the right dependency based on what the tool does:

| Dependency | Access via | When to use |
|---|---|---|
| `server.cli_runner` | `Arc<dyn CliRunner>` | Run the CodeScene CLI (code review, pre-commit, change-set, auto-refactor) |
| `server.http_client` | `Arc<dyn HttpClient>` | Make HTTP requests to CodeScene cloud/on-prem API |
| `server.config_data` | `Arc<ConfigData>` | Read server configuration values |

### 5. Wire the tool method on `CodeSceneServer`

In `src/main.rs`, inside the `#[tool_router] impl CodeSceneServer` block, add a new method:

```rust
#[tool(
    description = "One-line summary of what this tool does.\nAdditional detail on inputs, outputs, and how the LLM should present results.",
    input_schema = inlined_schema_for::<MyToolParam>()
)]
async fn my_tool(
    &self,
    Parameters(params): Parameters<MyToolParam>,
) -> Result<CallToolResult, ErrorData> {
    tools::my_tool::handle(self, params).await
}
```

Conventions:
- **The `description` string is critical** — it becomes the tool description visible to LLM clients. Be specific about inputs, outputs, and how the LLM should present results.
- **The method name becomes the MCP tool name** as seen by clients.
- **Use `inlined_schema_for::<ParamType>()`** to generate the JSON Schema for the parameter struct.
- **The method body is a one-liner** that delegates to the handler in `src/tools/<tool_name>.rs`.
- **For tools with no parameters**, omit `input_schema` and `Parameters`:
  ```rust
  #[tool(description = "...")]
  async fn my_tool(&self) -> Result<CallToolResult, ErrorData> {
      tools::my_tool::handle(self).await
  }
  ```

Also add the parameter struct to the import at the top of `src/main.rs` if you defined a new one:

```rust
use crate::tools::{
    ChangeSetParam, FilePathParam, GetConfigParam, GitRepoParam, MyToolParam,
    OptionalContext, OwnershipParam, ProjectFileParam, ProjectParam,
    RefactorParam, SetConfigParam,
};
```

### 6. Write inline tests

Add a `#[cfg(test)]` module at the bottom of `src/tools/<tool_name>.rs`:

```rust
#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;

    use crate::tests::{
        assert_error_contains, assert_success_contains, assert_token_error, clear_token,
        make_cli_mock_server, make_server, set_token, MockCliRunner,
    };
    use crate::tools::FilePathParam;

    #[tokio::test]
    async fn rejects_missing_token() {
        let _g = clear_token();
        let params = FilePathParam {
            file_path: "/tmp/f.rs".to_string(),
        };
        let result = make_server(false)
            .my_tool(Parameters(params))
            .await
            .unwrap();
        assert_token_error(&result);
    }

    #[tokio::test]
    async fn success_returns_expected_content() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_ok(r#"{"score":8.5,"review":[]}"#));
        let params = FilePathParam {
            file_path: "/tmp/test.rs".to_string(),
        };
        let result = server.my_tool(Parameters(params)).await.unwrap();
        assert_success_contains(&result, "expected content");
    }

    #[tokio::test]
    async fn error_returns_tool_error() {
        let _g = set_token("tok");
        let server = make_cli_mock_server(MockCliRunner::with_err(1, "tool failed"));
        let params = FilePathParam {
            file_path: "/tmp/test.rs".to_string(),
        };
        let result = server.my_tool(Parameters(params)).await.unwrap();
        assert_error_contains(&result, "tool failed");
    }
}
```

Key patterns:
- **Use `set_token()` / `clear_token()`** — these acquire a mutex guard (`TokenGuard`) that serializes tests touching `CS_ACCESS_TOKEN`. Always bind the guard to `_g` so it lives for the test's duration.
- **Use `MockCliRunner`** to inject controlled CLI responses:
  - `MockCliRunner::with_ok(output)` — simulates successful CLI execution
  - `MockCliRunner::with_err(code, stderr)` — simulates CLI failure
  - `MockCliRunner::with_responses(vec![...])` — queues multiple responses for tools that make multiple CLI calls
- **Use `make_cli_mock_server()`** to create a `CodeSceneServer` with mocked CLI and default HTTP.
- **Use `make_server_with_mocks()`** when you need to mock both CLI and HTTP.
- **Call the tool method directly** on the server instance: `server.my_tool(Parameters(params))`.
- **Use assertion helpers**: `assert_success_contains`, `assert_error_contains`, `assert_token_error`, `assert_standalone_error`.
- **Test at minimum**: missing token rejection, success path, and error path.

### 7. Validate Code Health and fix any issues

After the tool is implemented, tested, and registered, run the CodeScene Code Health tools to ensure the new code meets quality standards. **Do not consider the tool complete until Code Health passes without regressions.**

Choose the appropriate scope:

| Scope | Tool | When to use |
|---|---|---|
| Single file | `code_health_review` | Quick check on a specific file you just created or modified |
| Staged + unstaged changes | `pre_commit_code_health_safeguard` | Before committing — reviews all uncommitted changes in the repo |
| Full branch vs base | `analyze_change_set` | Before opening a PR — reviews all committed + staged + unstaged changes against a base ref |

Recommended workflow:

1. Run `code_health_review` on each new/modified file (at minimum `src/tools/<tool_name>.rs`).
2. If any code smells or regressions are reported:
   - Refactor the flagged code to resolve the issues.
   - Re-run `code_health_review` after each fix to confirm improvement.
3. Before committing, run `pre_commit_code_health_safeguard` to catch anything across all changed files.
4. Before opening a PR, run `analyze_change_set` against the target branch to ensure no regressions across the full change set.

**Target: Code Health 10.0.** Scores of 9+ are not "good enough" — aim for optimal.

## Canonical Example: `code_health_score`

The simplest existing tool to reference is `src/tools/code_health_score.rs`. It demonstrates:
- Single parameter (`FilePathParam`)
- Free `async fn handle()` receiving `&CodeSceneServer` and params
- Token check, version check, Docker path adaptation
- CLI call via `server.cli_runner`
- Event tracking on success and error
- Version warning prepended to output
- Inline `#[cfg(test)]` module with mocked dependencies

Refer to `src/tools/code_health_score.rs` as the minimal template.

## Checklist

Before considering the tool complete:

- [ ] Tool file created at `src/tools/<tool_name>.rs`
- [ ] Parameter struct defined in `src/tools/mod.rs` (or existing struct reused)
- [ ] Module declared in `src/tools/mod.rs` (`pub mod <tool_name>;`)
- [ ] Handler function `pub(crate) async fn handle()` implemented with token check, version check, and error handling
- [ ] Tool method added to `#[tool_router] impl CodeSceneServer` in `src/main.rs` with `#[tool(description = "...")]`
- [ ] Parameter struct imported in `src/main.rs` (if new)
- [ ] Inline tests cover: missing token, success path, and error path
- [ ] Tests use `MockCliRunner` / `MockHttpClient` for dependency injection
- [ ] Run tests with `cargo test`
- [ ] `code_health_review` passes on all new/modified files with no code smells
- [ ] `pre_commit_code_health_safeguard` reports no regressions before commit
- [ ] `analyze_change_set` reports no regressions before opening a PR

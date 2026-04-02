# MCP Server Integration Tests

Comprehensive integration test suite for the CodeScene MCP server. These tests validate the MCP server in realistic user environments by:

1. **Building the static executable** using `cargo build` (Rust)
2. **Moving the executable outside the repo** to avoid interference from bundled CLI tools
3. **Testing actual MCP tools** with real Code Health analysis
4. **Validating across different scenarios** (git repos, worktrees, platform-specific paths, etc.)

## Test Structure

```
tests/integration/
├── README.md                       # This file
│
│   # Infrastructure
├── run_all_tests.py               # Main test runner (builds and runs all tests)
├── test_utils.py                  # Re-exports from all infrastructure modules
├── mcp_client.py                  # MCPClient class (JSON-RPC over stdio)
├── server_backends.py             # ServerBackend ABC, CargoBackend, DockerBackend
├── file_utils.py                  # create_git_repo(), safe_temp_directory(), etc.
├── response_parsers.py            # extract_result_text(), extract_code_health_score()
├── test_output.py                 # print_header(), print_test(), print_summary()
├── fixtures.py                    # Test code samples with known Code Health characteristics
│
│   # Test suites (run by run_all_tests.py)
├── test_git_worktree.py           # Git worktree-specific tests
├── test_git_subtree.py            # Git subtree-specific tests
├── test_relative_paths.py         # Relative path resolution tests
├── test_business_case.py          # Refactoring business case tool tests
├── test_bundled_docs.py           # Bundled documentation tool tests
├── test_version_check.py          # Version check endpoint tests
├── test_analytics_tracking.py     # Analytics tracking non-blocking tests
├── test_analyze_change_set.py     # Branch-level change set analysis tests
├── test_ssl_cli_truststore.py     # SSL truststore injection E2E tests
│
│   # Standalone-only test suite (not in run_all_tests.py)
└── test_platform_specific.py      # Platform-specific path handling tests
```

## Backends

The test suite supports two backends for running the MCP server:

| Backend | Flag | Description | Use Case |
|---------|------|-------------|----------|
| **Static** | (default) | Builds static executable with Cargo | CI/CD, release testing |
| **Docker** | `--docker` | Runs in Docker container | Container testing (Linux/macOS) |

### Backend Examples

```bash
# Static backend (default) - builds and tests static executable (from repo root)
./tests/run-integration-tests.sh
python tests/integration/run_all_tests.py

# Docker backend - tests containerized server (from repo root)
./tests/run-integration-tests.sh --docker
python tests/integration/run_all_tests.py --backend docker
```

## Prerequisites

### Required

- **Python 3.10+** (Python 3.13 recommended for building)
- **Git** (for repository operations)
- **CS_ACCESS_TOKEN** environment variable set to a valid CodeScene access token
- **Rust toolchain** for building the static executable (`rustup` with stable toolchain)

### Optional

- **CS_ONPREM_URL** environment variable (defaults to https://codescene.io)
- **CS_DEFAULT_PROJECT_ID** if you want to test with a specific project

### Automatic Downloads

The test suite automatically downloads the CodeScene CLI for your platform if it's not already present in the repository root. Supported platforms:
- Linux (amd64, aarch64)
- macOS (Intel, Apple Silicon)
- Windows (amd64)

## Quick Start

### Run All Tests (Recommended)

This builds the executable automatically and runs all tests:

**Linux/macOS:**
```bash
# Set required environment variables
export CS_ACCESS_TOKEN="your_token_here"

# Run all tests
cd tests/integration
python run_all_tests.py
```

**Windows:**
```powershell
# Set required environment variables
$env:CS_ACCESS_TOKEN="your_token_here"

# Run all tests
cd tests\integration
python run_all_tests.py
```

### Run with Existing Executable

If you already have a built executable, you can skip the build step:

**Linux/macOS:**
```bash
python run_all_tests.py --executable /path/to/cs-mcp
```

**Windows:**
```powershell
python run_all_tests.py --executable C:\path\to\cs-mcp.exe
```

### Run Specific Test Suites

**Linux/macOS:**
```bash
# Platform-specific tests
python test_platform_specific.py /path/to/cs-mcp

# Git worktree tests
python test_git_worktree.py /path/to/cs-mcp
```

**Windows:**
```powershell
# Platform-specific tests
python test_platform_specific.py C:\path\to\cs-mcp.exe

# Git worktree tests
python test_git_worktree.py C:\path\to\cs-mcp.exe
```

## What Gets Tested

### 1. Core Tests (inline in `run_all_tests.py`)

- **Server Startup**: Verifies the MCP server starts and responds to initialization
- **Code Health Score**: Tests `code_health_score` tool with multiple code samples
  - Validates scores are within expected ranges
  - Tests with different code quality levels (high quality, complex, etc.)
- **Code Health Review**: Tests `code_health_review` tool for detailed analysis
- **Pre-commit Safeguard**: Tests `pre_commit_code_health_safeguard` with modified files
- **Outside Git Repo**: Tests tool behavior with files outside git repositories
- **No Bundled CLI Interference**: Validates test environment isolation (Cargo backend only)

### 2. Git Worktree Tests (`test_git_worktree.py`)

- **Code Health Score in Worktree**: Validates CLI invocation in worktrees
- **Code Health Review in Worktree**: Validates analysis tools work correctly
- **Pre-commit in Worktree**: Tests safeguard in worktree context
- **Relative Path Resolution**: Tests path resolution in worktree directories

### 3. Git Subtree Tests (`test_git_subtree.py`)

- **Code Health in Subtree**: Validates analysis on files inside nested subtree directories
- **Pre-commit in Subtree**: Tests safeguard when subtree files are modified
- **Path Resolution**: Ensures git root detection works correctly with subtrees

### 4. Relative Path Tests (`test_relative_paths.py`)

- **Relative Path Resolution**: Tests that relative file paths are handled correctly
- **Prevents "not in subpath" Regression**: Validates the fix for `find_git_root()` path resolution
- **Docker Skipping**: Relative paths are only supported in native/binary mode; Docker tests are skipped

### 5. Business Case Tests (`test_business_case.py`)

- **Refactoring Business Case**: Tests `code_health_refactoring_business_case` tool
- **Bundled Data Files**: Validates that `s_curve/regression/*.json` files are properly included in the build
- **Prevents "No such file or directory" Regression**: Ensures `defects.json` and `time.json` are accessible at runtime

### 6. Bundled Documentation Tests (`test_bundled_docs.py`)

- **Explain Code Health**: Tests `explain_code_health` tool returns meaningful content
- **Explain Code Health Productivity**: Tests `explain_code_health_productivity` tool
- **Bundled Docs Path Resolution**: Validates the `src/docs` directory is accessible at runtime

### 7. Version Check Tests (`test_version_check.py`)

- **Unreachable Endpoint**: Tool calls complete without being blocked by version check timeouts
- **Reachable Endpoint (Mock)**: Version info becomes available on subsequent calls after background fetch completes
- **No Noise**: No "VERSION UPDATE AVAILABLE" output when the check cannot reach GitHub

### 8. Analytics Tracking Tests (`test_analytics_tracking.py`)

- **Non-blocking Analytics**: Tool calls complete promptly when the analytics endpoint is unreachable
- **No Timeout Penalty**: Response times are not inflated by analytics timeout
- **Delivery When Reachable**: Analytics events are still delivered when the endpoint is reachable

### 9. Analyze Change Set Tests (`test_analyze_change_set.py`)

- **No Decline (Pass)**: Quality gate passes when no code health decline exists on the branch
- **Introduced Decline (Fail)**: Quality gate fails when a commit introduces code health regression
- **New File with Issues (Fail)**: Quality gate fails when a new file has code health issues
- **New Clean File (Pass)**: Quality gate passes when a new file has clean code health

### 10. Platform-Specific Tests (`test_platform_specific.py`) -- standalone only

> **Note:** This test suite is **not** registered in `run_all_tests.py`. It must be run standalone with a pre-built executable.

- **Absolute Paths**: Tests platform-specific absolute path handling (Windows `C:\` / Unix `/home/`)
- **Relative Paths**: Tests relative path resolution
- **Symlinks**: Tests symlink handling (Unix-like systems only)
- **Spaces in Paths**: Tests paths with spaces (e.g., `"My Documents/file.py"`)
- **Unicode in Paths**: Tests Unicode characters in file/directory names

### 11. SSL Truststore CLI Tests (`test_ssl_cli_truststore.py`)

- **Truststore Args Injected**: Validates that `REQUESTS_CA_BUNDLE` triggers Java truststore argument injection for CLI invocations
- **Truststore File Exists**: Ensures injected `-Djavax.net.ssl.trustStore=...` points to a real generated truststore file
- **No Silent Fallback**: Verifies that when CA env vars are absent, truststore args are not injected

## Test Fixtures

The test suite uses sample code files with known Code Health characteristics:

| File | Language | Expected Score | Description |
|------|----------|----------------|-------------|
| `src/utils/calculator.py` | Python | 8.5-10.0 | Simple, high-quality code |
| `src/services/order_processor.py` | Python | 7.0-9.0 | Medium complexity code |
| `src/auth/AuthService.js` | JavaScript | 7.0-10.0 | Good quality JS class |
| `src/main/java/com/example/OrderProcessor.java` | Java | 9.0-10.0 | High quality Java |

## How It Works

### Build Process

1. Runs `cargo build --release` in the project root
2. Locates the resulting binary in `target/release/`
3. Moves executable to persistent location outside repo
4. This ensures the test environment mimics actual user installations

### Test Execution

1. Creates temporary test directories for each test run
2. Generates git repositories with sample code
3. Starts MCP server as subprocess (communicates via stdio)
4. Sends JSON-RPC requests using MCP protocol
5. Validates responses contain expected Code Health data
6. Cleans up temporary directories

### Environment Isolation

Tests run in a clean environment:

- `CS_MOUNT_PATH` is **not set** (tests static executable mode)
- Executable is placed **outside repo root** (no bundled CLI available)
- Each test gets fresh temporary directories
- No interference from development environment

## Expected Output

### Successful Run

```
======================================================================
  MCP Server Comprehensive Integration Tests
======================================================================

Prerequisites OK

======================================================================
  Building Static Executable
======================================================================

  Copying source files to build directory...
  Building with Cargo (this may take several minutes)...
  Build successful: /tmp/cs_mcp_build_xyz/build/cs-mcp

Executable ready: /path/to/cs_mcp_test_bin/cs-mcp

Test directory: /tmp/cs_mcp_test_abc

======================================================================
  Test 1: MCP Server Startup
======================================================================

  ✓ PASS: Server process started
  ✓ PASS: Server responds to initialize

======================================================================
  Test 2: Code Health Score Tool
======================================================================

  ✓ PASS: Server started

  Testing: src/utils/calculator.py
  ✓ PASS: Score in expected range (8.5-10.0)
         Actual score: 9.5

  Testing: src/services/order_processor.py
  ✓ PASS: Score in expected range (7.0-9.0)
         Actual score: 7.8

[... more tests ...]

======================================================================
  Test Summary
======================================================================

  Total: 8 tests
  Passed: 8
```

### Failed Test

If a test fails, you'll see detailed error information:

```
  ✗ FAIL: Code Health Score returned
         Response: Error: Failed to analyze file: ...
```

## Troubleshooting

### Build Failures

**Error**: `cargo not found`
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Test Failures

**Error**: `CS_ACCESS_TOKEN not set`
```bash
export CS_ACCESS_TOKEN="your_token_here"
```

**Error**: `Timeout waiting for response`
- Check your network connection to CodeScene
- Verify CS_ONPREM_URL is correct
- Increase timeout in test code if analyzing large files

**Error**: `Score not in expected range`
- This might be OK if Code Health analysis has improved/changed
- Review the actual score and adjust expectations in `fixtures.py` if appropriate

### Platform Issues

**Windows**: Ensure you have Visual Studio Build Tools installed for Rust

**Linux**: Ensure you have `gcc` and development headers:
```bash
sudo apt-get install build-essential
```

**macOS**: Ensure Xcode Command Line Tools are installed:
```bash
xcode-select --install
```

## Adding New Tests

See the detailed skill guide at [`.agents/skills/create-integration-test/SKILL.md`](../../.agents/skills/create-integration-test/SKILL.md). It covers:

- File structure and boilerplate
- Backend abstraction (`ServerBackend` protocol)
- `MCPClient` usage and lifecycle
- How to register tests in `run_all_tests.py`
- Fixtures and verification checklist

The simplest existing test to use as a template is `test_business_case.py`.

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Integration Tests

on: [push, pull_request]

jobs:
  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Set up Python
        uses: actions/setup-python@v4
        with:
          python-version: '3.13'
      
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      
      - name: Run integration tests
        env:
          CS_ACCESS_TOKEN: ${{ secrets.CS_ACCESS_TOKEN }}
          CS_ONPREM_URL: ${{ secrets.CS_ONPREM_URL }}
        run: |
          cd tests/integration
          python run_all_tests.py
```

## Maintenance

### Updating Test Fixtures

When CodeScene's analysis improves or changes:

1. Review actual scores from test runs
2. Update expected score ranges in `fixtures.py`
3. Update test samples if needed to better represent edge cases

### Updating for New MCP Tools

When adding new MCP tools:

1. Add test cases following the [integration test skill guide](../../.agents/skills/create-integration-test/SKILL.md)
2. Add fixtures if needed for specific test scenarios
3. Update this README with new test descriptions

### Performance Considerations

- Building with Cargo takes 2-5 minutes
- Each test run creates fresh git repos
- Full test suite takes 5-10 minutes depending on network
- Use `--executable` flag to skip build during development

## License

Same as the main project.

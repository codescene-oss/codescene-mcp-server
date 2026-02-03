# MCP Server Integration Tests

Comprehensive integration test suite for the CodeScene MCP server. These tests validate the MCP server in realistic user environments by:

1. **Building the static executable** using Nuitka in an isolated environment
2. **Moving the executable outside the repo** to avoid interference from bundled CLI tools
3. **Testing actual MCP tools** with real Code Health analysis
4. **Validating across different scenarios** (git repos, worktrees, platform-specific paths, etc.)

## Test Structure

```
tests/integration/
├── README.md                       # This file
├── run_all_tests.py               # Main test runner (builds and runs all tests)
├── test_utils.py                  # Shared utilities (MCPClient, builders, helpers)
├── fixtures.py                    # Test code samples with known Code Health characteristics
├── test_platform_specific.py      # Platform-specific path handling tests
└── test_git_worktree.py          # Git worktree-specific tests
```

## Prerequisites

### Required

- **Python 3.10+** (Python 3.13 recommended for building)
- **Git** (for repository operations)
- **CS_ACCESS_TOKEN** environment variable set to a valid CodeScene access token
- **Nuitka** for building the static executable (`pip install nuitka`)

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

### 1. Main Test Suite (`run_all_tests.py`)

- **Server Startup**: Verifies the MCP server starts and responds to initialization
- **Code Health Score**: Tests `code_health_score` tool with multiple code samples
  - Validates scores are within expected ranges
  - Tests with different code quality levels (high quality, complex, etc.)
- **Code Health Review**: Tests `code_health_review` tool for detailed analysis
- **Pre-commit Safeguard**: Tests `pre_commit_code_health_safeguard` with modified files
- **Outside Git Repo**: Tests tool behavior with files outside git repositories
- **No Bundled CLI Interference**: Validates test environment isolation

### 2. Platform-Specific Tests (`test_platform_specific.py`)

- **Absolute Paths**: Tests platform-specific absolute path handling
  - Windows: `C:\Users\...`
  - Linux/Unix: `/home/...`
- **Relative Paths**: Tests relative path resolution
- **Symlinks**: Tests symlink handling (Unix-like systems only)
- **Spaces in Paths**: Tests paths with spaces (e.g., `"My Documents/file.py"`)
- **Unicode in Paths**: Tests Unicode characters in file/directory names

### 3. Git Worktree Tests (`test_git_worktree.py`)

- **Code Health Score in Worktree**: Validates CLI invocation in worktrees
- **Code Health Review in Worktree**: Validates analysis tools work correctly
- **Pre-commit in Worktree**: Tests safeguard in worktree context
- **Relative Path Resolution**: Tests path resolution in worktree directories

## Test Fixtures

The test suite uses sample code files with known Code Health characteristics:

| File | Language | Expected Score | Description |
|------|----------|----------------|-------------|
| `src/utils/calculator.py` | Python | 8.5-10.0 | Simple, high-quality code |
| `src/services/order_processor.py` | Python | 3.0-6.5 | Complex code with issues |
| `src/auth/AuthService.js` | JavaScript | 7.0-10.0 | Good quality JS class |
| `src/main/java/com/example/OrderProcessor.java` | Java | 6.0-9.0 | Medium complexity Java |

## How It Works

### Build Process

1. Creates an isolated build directory **outside the repo**
2. Copies source files and CLI tools to build directory
3. Runs Nuitka to create static executable
4. Moves executable to persistent location outside repo
5. This ensures the test environment mimics actual user installations

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
  Building with Nuitka (this may take several minutes)...
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
  ✓ PASS: Score in expected range (3.0-6.5)
         Actual score: 4.2

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

**Error**: `Nuitka not found`
```bash
pip install nuitka
```

**Error**: `cs/cs.exe not found`
- Ensure the CodeScene CLI is present in the repo root
- Download from: https://downloads.codescene.io/

**Error**: `Python 3.13 not found`
- Update `BuildConfig.python_executable` in code
- Or install Python 3.13

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

**Windows**: Ensure you have Visual Studio Build Tools installed for Nuitka

**Linux**: Ensure you have `gcc` and development headers:
```bash
sudo apt-get install build-essential python3-dev
```

**macOS**: Ensure Xcode Command Line Tools are installed:
```bash
xcode-select --install
```

## Adding New Tests

### 1. Add to Existing Test Suite

```python
def test_new_feature(executable: Path, repo_dir: Path) -> bool:
    """Test description."""
    print_header("Test: New Feature")
    
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        client.start()
        client.initialize()
        
        # Your test logic here
        response = client.call_tool("tool_name", {"arg": "value"})
        result_text = extract_result_text(response)
        
        # Validate response
        passed = validate_response(result_text)
        print_test("Test passed", passed, "Details...")
        
        return passed
    finally:
        client.stop()
```

### 2. Add to Test Runner

Edit `run_all_tests.py`:

```python
def run_all_tests(executable: Path) -> int:
    # ... existing code ...
    
    all_results.append(("New Feature", test_new_feature(executable, test_dir, repo_dir)))
    
    return print_summary(all_results)
```

### 3. Add New Test File

For a new test category:

1. Create `test_your_feature.py`
2. Import utilities from `test_utils.py`
3. Implement test functions
4. Add main runner
5. Update this README

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
      
      - name: Install dependencies
        run: |
          pip install nuitka
          pip install -r src/requirements.txt
      
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

1. Add test cases in appropriate test file
2. Add fixtures if needed for specific test scenarios
3. Update this README with new test descriptions

### Performance Considerations

- Building with Nuitka takes 2-5 minutes
- Each test run creates fresh git repos
- Full test suite takes 5-10 minutes depending on network
- Use `--executable` flag to skip build during development

## License

Same as the main project.

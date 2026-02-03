# Integration Tests Quick Start

## Run Tests

### Linux/macOS
```bash
# Simple - runs everything
./run-integration-tests.sh

# Or use make
make test-integration

# Run with existing executable (faster for development)
./run-integration-tests.sh --executable /path/to/cs-mcp

# Run specific test suites
make test-integration-platform
make test-integration-worktree
```

### Windows
```powershell
# Simple - runs everything
.\run-integration-tests.ps1

# Run with existing executable (faster for development)
.\run-integration-tests.ps1 -Executable C:\path\to\cs-mcp.exe

# Run specific test suites
.\run-integration-tests.ps1 -PlatformOnly -Executable C:\path\to\cs-mcp.exe
.\run-integration-tests.ps1 -WorktreeOnly -Executable C:\path\to\cs-mcp.exe
```

## Prerequisites

### Linux/macOS
```bash
# 1. Install Nuitka
pip install nuitka

# 2. Set CodeScene token
export CS_ACCESS_TOKEN="your_token_here"

# Note: CS CLI is downloaded automatically if not present
```

### Windows
```powershell
# 1. Install Nuitka
pip install nuitka

# 2. Set CodeScene token
$env:CS_ACCESS_TOKEN="your_token_here"

# Note: CS CLI is downloaded automatically if not present
```

## What's Different from Old Tests

### Old Tests (tests/path-resolution-integration, tests/ssl-integration)
- ❌ Required manually building and pointing to executable
- ❌ Tested in repo root with bundled CLI available
- ❌ Mostly tested for errors, not actual functionality
- ❌ No actual Code Health scores returned

### New Tests (tests/integration)
- ✅ Builds executable automatically in isolated environment
- ✅ Moves executable **outside repo** (no bundled CLI interference)
- ✅ Tests actual MCP tools with **real Code Health analysis**
- ✅ Validates actual scores against expected ranges
- ✅ Tests real-world scenarios (git repos, worktrees, paths, etc.)

### Linux/macOS
```bash
# 1. Make changes to MCP server code

# 2. Run integration tests
./run-integration-tests.sh

# 3. If tests fail, check logs and fix issues

# 4. Run specific tests during development
cd tests/integration
python test_platform_specific.py ../../../cs_mcp_test_bin/cs-mcp
```

### Windows
```powershell
# 1. Make changes to MCP server code

# 2. Run integration tests
.\run-integration-tests.ps1

# 3. If tests fail, check logs and fix issues

# 4. Run specific tests during development
cd tests\integration
python test_platform_specific.py ..\..\..\cs_mcp_test_bin\cs-mcp.exe
## Development Workflow

```bash
**Linux/macOS:**
```bash
export CS_ACCESS_TOKEN="get_from_codescene_settings"
```

**Windows:**
```powershell
$env:CS_ACCESS_TOKEN="get_from_codescene_settings"
```

### "Nuitka build failed"
**All platforms:**
```bash
pip install nuitka
```

**Linux:**
```bash
sudo apt-get install build-essential python3-dev
```

**macOS:**
```bash
xcode-select --install
```

**Windows:**
- Ensure Visual Studio Build Tools are installed
- Download from: https://visualstudio.microsoft.com/downloads/hon test_platform_specific.py ../../../cs_mcp_test_bin/cs-mcp
```

## Troubleshooting

### "CS_ACCESS_TOKEN not set"
```bash
export CS_ACCESS_TOKEN="get_from_codescene_settings"
```

### "Nuitka build failed"
```bash
pip install nuitka
# On Linux: sudo apt-get install build-essential python3-dev
# On macOS: xcode-select --install
```

### "No cs/cs.exe found"
Download from: https://downloads.codescene.io/

### Tests timeout
- Check network connection
- Verify CS_ONPREM_URL is correct
- Large files may need more time (increase timeout in code)

## Adding New Tests

1. Add test function in appropriate file (or create new file)
2. Use utilities from `test_utils.py`
3. Add to test runner in `run_all_tests.py`
4. Update README if adding new test category

Example:
```python
def test_new_feature(executable: Path, repo_dir: Path) -> bool:
    """Test new MCP feature."""
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        client.start()
        client.initialize()
        
        response = client.call_tool("tool_name", {"arg": "value"})
        result = extract_result_text(response)
        
        passed = validate(result)
        print_test("Feature works", passed, result)
        return passed
    finally:
        client.stop()
```

## See Also

- [Full README](tests/integration/README.md) - Comprehensive documentation
- [Old tests](tests/path-resolution-integration/) - Legacy tests (kept for reference)
- [CodeScene Docs](https://codescene.io/docs) - CodeScene documentation

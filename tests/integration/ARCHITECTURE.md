# Integration Tests Architecture

## High-Level Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  User runs: ./tests/run-integration-tests.sh                    │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│  Prerequisite Checks                                            │
│  - Python 3.10+                                                 │
│  - Git available                                                │
│  - CS_ACCESS_TOKEN set                                          │
│  - Nuitka installed                                             │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│  ExecutableBuilder.build()                                       │
│                                                                  │
│  1. Create isolated build dir: /tmp/cs_mcp_build_xyz/          │
│  2. Copy src/ files to build dir                                │
│  3. Copy cs CLI to build dir                                    │
│  4. Run Nuitka to create static executable                      │
│  5. Move executable OUTSIDE repo:                               │
│     /path/to/parent/cs_mcp_test_bin/cs-mcp                     │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│  Test Environment Setup                                          │
│                                                                  │
│  Create temp dir: /tmp/cs_mcp_test_abc/                        │
│  ├── test_repo/           (git repo with sample code)          │
│  │   ├── .git/                                                  │
│  │   ├── src/utils/calculator.py      (score 9.0+)            │
│  │   ├── src/services/order_processor.py  (score 4.0+)        │
│  │   └── src/auth/AuthService.js                              │
│  └── standalone.py        (file outside git)                   │
│                                                                  │
│  NO cs or cs.exe CLI here! (mimics user installation)          │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│  Run Test Suites                                                │
│                                                                  │
│  For each test:                                                 │
│    1. Start MCP server as subprocess                            │
│    2. MCPClient communicates via stdin/stdout                   │
│    3. Send JSON-RPC requests                                    │
│    4. Receive and validate responses                            │
│    5. Extract and check Code Health scores                      │
│    6. Stop server and cleanup                                   │
└────────────────────┬────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│  Report Results                                                  │
│                                                                  │
│  ✓ Server Startup                                               │
│  ✓ Code Health Score - calculator.py (score: 9.5)              │
│  ✓ Code Health Score - order_processor.py (score: 4.2)         │
│  ✓ Code Health Review                                          │
│  ✓ Pre-commit Safeguard                                         │
│  ✓ Outside Git Repo                                             │
│  ✓ No Bundled CLI                                               │
│                                                                  │
│  Total: 8 tests | Passed: 8 | Failed: 0                        │
└─────────────────────────────────────────────────────────────────┘
```

## Directory Layout During Test

```
File System Layout:
====================

Repository (dev-mcp):
/home/asko/Code/CodeScene/dev-mcp/
├── cs                              ← Bundled CLI (NOT USED in tests)
├── src/                            ← Source code
├── run-integration-tests.sh        ← Test runner
└── tests/integration/              ← Test suite

Build Directory (temporary):
/tmp/cs_mcp_build_xyz/
├── src/                            ← Copied source
├── cs                              ← Copied CLI
└── cs-mcp                          ← Built executable

Executable Location (isolated):
/home/asko/Code/cs_mcp_test_bin/
└── cs-mcp                          ← Test executable (OUTSIDE repo!)

Test Directory (temporary):
/tmp/cs_mcp_test_abc/
├── test_repo/                      ← Git repo with samples
│   ├── .git/
│   ├── src/utils/calculator.py
│   ├── src/services/order_processor.py
│   └── src/auth/AuthService.js
└── standalone.py                   ← Non-git file

Test runs from /tmp/cs_mcp_test_abc/test_repo
    → No cs CLI available here!
    → cs-mcp is at /home/asko/Code/cs_mcp_test_bin/cs-mcp
    → This mimics real user installations
```

## MCPClient Communication

```
Test Process                  MCP Server Process
──────────                    ──────────────────

┌─────────────┐              ┌─────────────────┐
│  Test Suite │              │   cs-mcp        │
│             │              │   (subprocess)  │
└──────┬──────┘              └────────┬────────┘
       │                              │
       │  Start subprocess            │
       ├─────────────────────────────▶│
       │                              │
       │  stdin: JSON-RPC request     │
       ├─────────────────────────────▶│
       │  {"jsonrpc": "2.0",         │
       │   "method": "initialize"... │
       │                              │
       │                              │ Process request
       │                              │ Call CodeScene backend
       │                              │ Analyze code
       │                              │
       │  stdout: JSON-RPC response   │
       │◀─────────────────────────────┤
       │  {"result": {               │
       │    "content": [{             │
       │      "text": "Score: 9.5"   │
       │                              │
       │  Extract score, validate     │
       │                              │
       │  stdin: Next request         │
       ├─────────────────────────────▶│
       │                              │
       ...                           ...
       │                              │
       │  Terminate                   │
       ├─────────────────────────────▶│
       │                              │  Exit
       │                              ×
       │
       │  Validate all results
       └─▶ Report summary
```

## Test Execution Sequence

```
┌────────────────────────────────────────────────────────────────┐
│ 1. test_server_startup()                                        │
│    ├─ Start MCP server                                          │
│    ├─ Send initialize request                                   │
│    ├─ Validate response                                         │
│    └─ Stop server                                               │
└────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ 2. test_code_health_score()                                     │
│    ├─ Start MCP server in test repo                            │
│    ├─ For each sample file:                                    │
│    │   ├─ Call code_health_score tool                          │
│    │   ├─ Extract actual score                                 │
│    │   ├─ Compare with expected range                          │
│    │   └─ Record result                                        │
│    └─ Stop server                                               │
└────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ 3. test_code_health_review()                                    │
│    ├─ Start MCP server in test repo                            │
│    ├─ Call code_health_review tool                             │
│    ├─ Validate response contains analysis                      │
│    └─ Stop server                                               │
└────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ 4. test_pre_commit_safeguard()                                  │
│    ├─ Start MCP server in test repo                            │
│    ├─ Modify and stage a file                                  │
│    ├─ Call pre_commit_code_health_safeguard                    │
│    ├─ Validate quality gate response                           │
│    ├─ Reset file                                                │
│    └─ Stop server                                               │
└────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ 5. test_outside_git_repo()                                      │
│    ├─ Create standalone file (not in git)                      │
│    ├─ Start MCP server                                          │
│    ├─ Call code_health_score on standalone file                │
│    ├─ Validate graceful handling                               │
│    └─ Stop server                                               │
└────────────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ 6. test_no_bundled_cli_interference()                           │
│    ├─ Check test directory for cs/cs.exe                       │
│    ├─ Check parent directories                                 │
│    └─ Validate none found                                      │
└────────────────────────────────────────────────────────────────┘
```

## Key Components

### ExecutableBuilder

```python
class ExecutableBuilder:
    """Builds static executable in isolated environment."""
    
    def build(self) -> Path:
        # 1. Create build dir outside repo
        # 2. Copy source files
        # 3. Copy CS CLI
        # 4. Run Nuitka
        # 5. Return path to executable
```

### MCPClient

```python
class MCPClient:
    """Communicates with MCP server via stdio."""
    
    def __init__(self, command, env, cwd):
        # Set up subprocess parameters
        
    def start(self) -> bool:
        # Start MCP server process
        # Start reader threads
        
    def send_request(self, method, params) -> dict:
        # Send JSON-RPC request via stdin
        # Wait for response from stdout
        # Return parsed response
        
    def call_tool(self, tool_name, arguments) -> dict:
        # Convenience method for calling MCP tools
        
    def stop(self):
        # Gracefully terminate server
```

### Test Utilities

```python
def create_test_environment() -> dict:
    """Create clean environment (no CS_MOUNT_PATH)."""
    
def create_git_repo(base_dir, sample_files) -> Path:
    """Create git repo with sample code."""
    
def extract_code_health_score(response_text) -> float:
    """Extract score from tool response."""
    
def extract_result_text(tool_response) -> str:
    """Extract text from MCP response structure."""
```

## Why This Architecture Works

### 1. Isolated Build
- Build happens in temp directory
- No contamination of source tree
- Reproducible builds
- Easy to parallelize

### 2. Realistic Environment
- Executable outside repo (no bundled CLI available)
- Tests run in temp directories
- Mimics user installations exactly
- Catches environment-specific bugs

### 3. Clean Separation
- Build phase separate from test phase
- Each test gets fresh environment
- No test interdependencies
- Easy to debug individual tests

### 4. Real Communication
- Uses actual MCP protocol (JSON-RPC over stdio)
- Tests real server behavior
- Validates actual responses
- End-to-end testing

### 5. Comprehensive Validation
- Not just "no errors"
- Actual Code Health scores
- Score range validation
- Complete workflow testing

## Extending the Architecture

### Adding a New Test

```python
def test_new_feature(executable: Path, repo_dir: Path) -> bool:
    """Test description."""
    env = create_test_environment()
    client = MCPClient([str(executable)], env=env, cwd=str(repo_dir))
    
    try:
        if not client.start():
            return False
        
        client.initialize()
        
        # Your test logic
        response = client.call_tool("tool_name", {"arg": "value"})
        result = extract_result_text(response)
        
        # Validation
        passed = validate(result)
        print_test("Test name", passed, result)
        
        return passed
    finally:
        client.stop()
```

### Adding a New Test Suite

```python
# Create test_your_feature.py
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))

from test_utils import MCPClient, create_test_environment, ...
from fixtures import get_sample_files

def run_your_tests(executable: Path) -> int:
    # Set up environment
    # Run tests
    # Return exit code
    
if __name__ == "__main__":
    sys.exit(run_your_tests(Path(sys.argv[1])))
```

## Summary

The architecture provides:
- ✅ Automated build in isolation
- ✅ Realistic test environments
- ✅ Actual MCP protocol communication
- ✅ Real Code Health validation
- ✅ Clean separation of concerns
- ✅ Easy to extend and maintain
- ✅ Comprehensive coverage
- ✅ Clear, actionable results

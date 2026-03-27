# Integration Tests Quick Start

## Run Tests

### Linux/macOS
```bash
# From repo root - runs everything
./tests/run-integration-tests.sh

# Or use make
make test-integration

# Run with existing executable (faster for development)
./tests/run-integration-tests.sh --executable /path/to/cs-mcp

# Run specific test suites
make test-integration-platform
make test-integration-worktree
./tests/run-integration-tests.sh --worktree-only --executable /path/to/cs-mcp
./tests/run-integration-tests.sh --subtree-only --executable /path/to/cs-mcp
```

### Windows
```powershell
# From repo root - runs everything
.\tests\run-integration-tests.ps1

# Run with existing executable (faster for development)
.\tests\run-integration-tests.ps1 -Executable C:\path\to\cs-mcp.exe

# Run specific test suites
.\tests\run-integration-tests.ps1 -PlatformOnly -Executable C:\path\to\cs-mcp.exe
.\tests\run-integration-tests.ps1 -WorktreeOnly -Executable C:\path\to\cs-mcp.exe
.\tests\run-integration-tests.ps1 -SubtreeOnly -Executable C:\path\to\cs-mcp.exe
```

### Backend Options

Tests can run with different backends:

```bash
# Static backend (default) - builds static executable with Cargo
./tests/run-integration-tests.sh

# Docker backend - runs in container (Linux/macOS only)
./tests/run-integration-tests.sh --docker
```

## Prerequisites

### Linux/macOS
```bash
# 1. Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Set CodeScene token
export CS_ACCESS_TOKEN="your_token_here"

# Note: CS CLI is downloaded automatically during cargo build
# Note: git-subtree tests will be skipped if git-subtree is not installed
#       (it's a contrib command, not always available by default)
```

### Windows
```powershell
# 1. Install Rust toolchain
# Download and run rustup-init.exe from https://rustup.rs

# 2. Set CodeScene token
$env:CS_ACCESS_TOKEN="your_token_here"

# Note: CS CLI is downloaded automatically during cargo build
```



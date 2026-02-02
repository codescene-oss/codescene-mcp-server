#!/bin/bash
# Static Binary Path Resolution Integration Test Runner
#
# This script:
# 1. Builds the cs-mcp static binary using Nuitka
# 2. Creates a test git repository
# 3. Runs the path resolution tests WITHOUT CS_MOUNT_PATH set
# 4. Verifies tools work correctly in static executable mode
# 5. Cleans up
#
# Prerequisites:
# - Python 3.13
# - Nuitka installed (pip install Nuitka)
# - Git

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

echo "============================================================"
echo "  Static Binary Path Resolution Integration Test"
echo "  Testing: cs-mcp binary without CS_MOUNT_PATH"
echo "============================================================"
echo ""

# Create temp directory for build artifacts
BUILD_DIR=$(mktemp -d)

cleanup() {
    echo ""
    echo "Cleaning up..."
    
    # Remove build artifacts
    rm -rf "$BUILD_DIR"
    
    # Note: We don't remove the cs-mcp binary or Nuitka build dirs
    # as they may be wanted for other tests
    
    echo "Cleanup complete."
}

trap cleanup EXIT

# Step 1: Check prerequisites
echo "Step 1: Checking prerequisites..."

if ! command -v python3.13 &> /dev/null; then
    echo "  ✗ Python 3.13 not found"
    echo "  Install Python 3.13 to run static variant tests"
    exit 1
fi
echo "  ✓ Python 3.13 found"

if ! python3.13 -c "import nuitka" 2>/dev/null; then
    echo "  ✗ Nuitka not found"
    echo "  Install with: pip install Nuitka"
    exit 1
fi
echo "  ✓ Nuitka found"

if ! command -v git &> /dev/null; then
    echo "  ✗ Git not found"
    exit 1
fi
echo "  ✓ Git found"

# Step 2: Download CS CLI if not present
echo ""
echo "Step 2: Checking CodeScene CLI..."

if [ ! -f "$REPO_ROOT/cs" ]; then
    echo "  Downloading CodeScene CLI..."
    
    # Detect platform
    if [[ "$OSTYPE" == "darwin"* ]]; then
        if [[ $(uname -m) == "arm64" ]]; then
            CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-macos-aarch64-latest.zip"
        else
            CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-macos-amd64-latest.zip"
        fi
    elif [[ "$OSTYPE" == "linux"* ]]; then
        if [[ $(uname -m) == "aarch64" ]]; then
            CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-linux-aarch64-latest.zip"
        else
            CLI_URL="https://downloads.codescene.io/enterprise/cli/cs-linux-amd64-latest.zip"
        fi
    else
        echo "  ✗ Unsupported platform: $OSTYPE"
        exit 1
    fi
    
    curl -fsSL "$CLI_URL" -o "$BUILD_DIR/cs.zip"
    unzip -q "$BUILD_DIR/cs.zip" -d "$REPO_ROOT"
    chmod +x "$REPO_ROOT/cs"
    echo "  ✓ CodeScene CLI downloaded"
else
    echo "  ✓ CodeScene CLI found"
fi

# Step 3: Build cs-mcp static binary (if not already built)
echo ""
echo "Step 3: Checking/Building cs-mcp static binary..."

if [ -f "$REPO_ROOT/cs-mcp" ]; then
    echo "  ✓ cs-mcp binary already exists, skipping build"
else
    echo "  Building cs-mcp (this may take several minutes)..."
    
    cd "$REPO_ROOT"
    
    # Create virtual environment if needed
    if [ ! -d ".venv" ]; then
        python3.13 -m venv .venv
    fi
    source .venv/bin/activate
    
    # Install dependencies
    pip install -q -r src/requirements.txt
    pip install -q Nuitka
    
    # Build with Nuitka
    python3.13 -m nuitka --onefile \
        --assume-yes-for-downloads \
        --include-data-dir=./src/docs=src/docs \
        --include-data-files=./cs=cs \
        --output-filename=cs-mcp \
        src/cs_mcp_server.py 2>&1 | tail -5
    
    deactivate 2>/dev/null || true
    
    if [ ! -f "$REPO_ROOT/cs-mcp" ]; then
        echo "  ✗ Failed to build cs-mcp binary"
        exit 1
    fi
    echo "  ✓ cs-mcp binary built successfully"
fi

# Step 4: Run the path resolution tests
echo ""
echo "Step 4: Running path resolution tests (static mode)..."

# Ensure CS_MOUNT_PATH is NOT set (static mode)
unset CS_MOUNT_PATH

# Set a dummy access token for API calls (they'll fail but we're testing path resolution)
export CS_ACCESS_TOKEN="${CS_ACCESS_TOKEN:-test-token}"

# Activate venv for running tests
cd "$REPO_ROOT"
if [ -d ".venv" ]; then
    source .venv/bin/activate
fi

# Run the test Python script
python3 "$SCRIPT_DIR/test_static_variant.py" "$REPO_ROOT/cs-mcp"
TEST_EXIT_CODE=$?

deactivate 2>/dev/null || true

if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo ""
    echo "============================================================"
    echo "  Static Variant Path Resolution Tests PASSED ✓"
    echo "============================================================"
else
    echo ""
    echo "============================================================"
    echo "  Static Variant Path Resolution Tests FAILED ✗"
    echo "============================================================"
fi

exit $TEST_EXIT_CODE

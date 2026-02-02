#!/bin/bash
# Docker Image Path Resolution Integration Test Runner
#
# This script tests the Docker image with CS_MOUNT_PATH set:
# 1. Builds the Docker image from the main Dockerfile
# 2. Creates test data directory
# 3. Runs `docker run -i codescene-mcp` with CS_MOUNT_PATH set
# 4. Verifies tools work correctly with path translation
# 5. Cleans up
#
# Prerequisites:
# - Docker installed

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

# Docker image name for testing
MCP_IMAGE="codescene-mcp-path-test"

echo "============================================================"
echo "  Docker Image Path Resolution Integration Test"
echo "  Testing: docker run with CS_MOUNT_PATH set"
echo "============================================================"
echo ""

# Create temp directory for test data
BUILD_DIR=$(mktemp -d)
TEST_DATA_DIR="$BUILD_DIR/test-data"
mkdir -p "$TEST_DATA_DIR"

cleanup() {
    echo ""
    echo "Cleaning up..."
    
    # Remove test image
    docker rmi "$MCP_IMAGE" 2>/dev/null || true
    
    # Remove temp directory
    rm -rf "$BUILD_DIR"
    
    echo "Cleanup complete."
}

trap cleanup EXIT

# Step 1: Build the Docker image
echo "Step 1: Building Docker image from main Dockerfile..."

docker build -t "$MCP_IMAGE" "$REPO_ROOT" 2>&1 | tail -5

if ! docker image inspect "$MCP_IMAGE" > /dev/null 2>&1; then
    echo "  ✗ Failed to build Docker image"
    exit 1
fi
echo "  ✓ Docker image built successfully"

# Step 2: Create test data
echo ""
echo "Step 2: Creating test data..."

# Create a simple test file
cat > "$TEST_DATA_DIR/TestFile.java" << 'EOF'
public class TestFile {
    public void hello() {
        System.out.println("Hello");
    }
}
EOF

echo "  ✓ Test data created at $TEST_DATA_DIR"

# Step 3: Run path resolution tests
echo ""
echo "Step 3: Running path resolution tests (Docker mode)..."

# Export environment for the test script
export DOCKER_IMAGE="$MCP_IMAGE"
export TEST_DATA_PATH="$TEST_DATA_DIR"

# Run the test Python script (runs on host, uses docker run)
python3 "$SCRIPT_DIR/test_docker_run.py"
TEST_EXIT_CODE=$?

if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo ""
    echo "============================================================"
    echo "  Docker Variant Path Resolution Tests PASSED ✓"
    echo "============================================================"
else
    echo ""
    echo "============================================================"
    echo "  Docker Variant Path Resolution Tests FAILED ✗"
    echo "============================================================"
fi

exit $TEST_EXIT_CODE

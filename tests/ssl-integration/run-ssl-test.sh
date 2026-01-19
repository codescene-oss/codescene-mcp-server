#!/bin/bash
# SSL End-to-End Integration Test Runner
#
# This script runs end-to-end SSL integration tests for the MCP server.
# It tests the ACTUAL deployment artifacts (Docker image or static binary),
# not just running Python directly.
#
# Variants:
# - docker: Builds the Docker image, runs it with docker run
# - static: Builds the cs-mcp binary with Nuitka, runs it directly
#
# Prerequisites:
# - Docker and Docker Compose installed
# - For static: Python 3.13, Nuitka, and build dependencies
#
# Usage:
#   ./run-ssl-test.sh          # Test Docker variant (default)
#   ./run-ssl-test.sh docker   # Test Docker variant
#   ./run-ssl-test.sh static   # Test static binary variant

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$SCRIPT_DIR"

VARIANT="${1:-docker}"

echo "============================================================"
echo "  MCP Server SSL End-to-End Integration Tests"
echo "  Testing variant: $VARIANT"
echo "============================================================"
echo ""

if [ "$VARIANT" = "docker" ]; then
    # Docker variant: Use docker-compose to orchestrate the test
    echo "Building and running Docker variant test..."
    echo ""
    
    if docker compose up --build --abort-on-container-exit --exit-code-from mcp-docker-test 2>&1; then
        echo ""
        echo "============================================================"
        echo "  Docker Variant Tests PASSED ✓"
        echo "============================================================"
        exit_code=0
    else
        echo ""
        echo "============================================================"
        echo "  Docker Variant Tests FAILED ✗"
        echo "============================================================"
        exit_code=1
    fi
    
    # Cleanup
    echo ""
    echo "Cleaning up Docker resources..."
    docker compose down -v --remove-orphans 2>/dev/null || true
    
    exit $exit_code

elif [ "$VARIANT" = "static" ]; then
    # Static variant: Build cs-mcp binary and test it
    echo "Building static binary variant..."
    echo ""
    
    # Run the static variant test script
    exec "$SCRIPT_DIR/run-static-test.sh"
    
else
    echo "Unknown variant: $VARIANT"
    echo "Usage: $0 [docker|static]"
    exit 1
fi

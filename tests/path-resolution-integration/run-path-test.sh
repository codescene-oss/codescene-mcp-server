#!/bin/bash
# Path Resolution Integration Test Runner
#
# This script runs both Docker and static binary path resolution tests.
#
# Usage:
#   ./run-path-test.sh          # Run both Docker and static tests
#   ./run-path-test.sh docker   # Run only Docker test
#   ./run-path-test.sh static   # Run only static test
#
# The tests verify that MCP tools correctly resolve file paths in both:
# - Docker mode (with CS_MOUNT_PATH set)
# - Static executable mode (without CS_MOUNT_PATH)
#
# Prerequisites:
# - Docker installed (for Docker tests)
# - Python 3.13 + Nuitka (for static tests)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo ""
echo "============================================================"
echo "  Path Resolution Integration Tests"
echo "============================================================"
echo ""
echo "These tests verify that MCP tools correctly resolve file paths"
echo "in both Docker and static executable modes."
echo ""

TEST_MODE="${1:-both}"

DOCKER_PASSED=false
STATIC_PASSED=false

run_docker_test() {
    echo "Running Docker path resolution tests..."
    echo ""
    
    if "$SCRIPT_DIR/run-docker-test.sh"; then
        DOCKER_PASSED=true
    fi
}

run_static_test() {
    echo "Running static binary path resolution tests..."
    echo ""
    
    if "$SCRIPT_DIR/run-static-test.sh"; then
        STATIC_PASSED=true
    fi
}

case "$TEST_MODE" in
    docker)
        run_docker_test
        ;;
    static)
        run_static_test
        ;;
    both)
        run_docker_test
        echo ""
        echo "============================================================"
        echo ""
        run_static_test
        ;;
    *)
        echo "Usage: $0 [docker|static|both]"
        exit 1
        ;;
esac

echo ""
echo "============================================================"
echo "  Final Summary"
echo "============================================================"
echo ""

if [ "$TEST_MODE" = "both" ] || [ "$TEST_MODE" = "docker" ]; then
    if $DOCKER_PASSED; then
        echo "  Docker tests:  PASSED ✓"
    else
        echo "  Docker tests:  FAILED ✗"
    fi
fi

if [ "$TEST_MODE" = "both" ] || [ "$TEST_MODE" = "static" ]; then
    if $STATIC_PASSED; then
        echo "  Static tests:  PASSED ✓"
    else
        echo "  Static tests:  FAILED ✗"
    fi
fi

echo ""

# Exit with failure if any test failed
if [ "$TEST_MODE" = "both" ]; then
    if $DOCKER_PASSED && $STATIC_PASSED; then
        echo "All path resolution tests passed! ✓"
        exit 0
    else
        echo "Some path resolution tests failed! ✗"
        exit 1
    fi
elif [ "$TEST_MODE" = "docker" ]; then
    $DOCKER_PASSED && exit 0 || exit 1
elif [ "$TEST_MODE" = "static" ]; then
    $STATIC_PASSED && exit 0 || exit 1
fi

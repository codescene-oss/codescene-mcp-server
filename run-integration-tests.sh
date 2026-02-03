#!/usr/bin/env bash
#
# Run all integration tests for the CodeScene MCP Server
#
# This script runs the comprehensive integration test suite which:
# - Builds the static executable in an isolated environment
# - Moves it outside the repo to mimic real user installations
# - Tests actual MCP tools with real Code Health analysis
# - Validates across different scenarios (git, worktrees, platform-specific)
#
# Prerequisites:
# - Python 3.10+ (3.13 recommended)
# - Git
# - CS_ACCESS_TOKEN environment variable
# - Nuitka (pip install nuitka)
#
# Usage:
#   ./run-integration-tests.sh          # Build and run all tests
#   ./run-integration-tests.sh --help   # Show help

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR="$SCRIPT_DIR/tests/integration"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check prerequisites
check_prerequisites() {
    echo "Checking prerequisites..."
    
    local missing=0
    
    # Check Python version
    if ! command -v python3 &> /dev/null; then
        echo -e "${RED}✗ Python 3 not found${NC}"
        missing=1
    else
        local python_version=$(python3 --version | awk '{print $2}')
        echo -e "${GREEN}✓ Python: $python_version${NC}"
    fi
    
    # Check Git
    if ! command -v git &> /dev/null; then
        echo -e "${RED}✗ Git not found${NC}"
        missing=1
    else
        local git_version=$(git --version | awk '{print $3}')
        echo -e "${GREEN}✓ Git: $git_version${NC}"
    fi
    
    # Check CS_ACCESS_TOKEN
    if [ -z "$CS_ACCESS_TOKEN" ]; then
        echo -e "${RED}✗ CS_ACCESS_TOKEN not set${NC}"
        echo "  Set it with: export CS_ACCESS_TOKEN='your_token_here'"
        missing=1
    else
        echo -e "${GREEN}✓ CS_ACCESS_TOKEN is set${NC}"
    fi
    
    # Check Nuitka
    if ! python3 -c "import nuitka" 2>/dev/null; then
        echo -e "${YELLOW}! Nuitka not installed (required for building)${NC}"
        echo "  Install with: pip install nuitka"
        missing=1
    else
        echo -e "${GREEN}✓ Nuitka is installed${NC}"
    fi
    
    if [ $missing -eq 1 ]; then
        echo ""
        echo -e "${RED}Some prerequisites are missing. Please install them before running tests.${NC}"
        exit 1
    fi
    
    echo ""
}

# Show help
show_help() {
    cat << EOF
Run CodeScene MCP Server Integration Tests

Usage:
  $0 [OPTIONS]

Options:
  --help              Show this help message
  --executable PATH   Use existing executable (skip build)
  --platform-only     Run only platform-specific tests
  --worktree-only     Run only git worktree tests
  --subtree-only      Run only git subtree tests
  --skip-build        Skip build step (use previously built executable)

Environment Variables:
  CS_ACCESS_TOKEN     CodeScene access token (required)
  CS_ONPREM_URL       CodeScene URL (optional, defaults to https://codescene.io)

Examples:
  # Run all tests (builds automatically)
  $0

  # Run with existing executable
  $0 --executable /path/to/cs-mcp

  # Run only platform tests
  $0 --platform-only

EOF
}

# Parse arguments
EXECUTABLE=""
TEST_MODE="all"
SKIP_BUILD=0

while [[ $# -gt 0 ]]; do
    case $1 in
        --help|-h)
            show_help
            exit 0
            ;;
        --executable)
            EXECUTABLE="$2"
            SKIP_BUILD=1
            shift 2
            ;;
        --platform-only)
            TEST_MODE="platform"
            shift
            ;;
        --worktree-only)
            TEST_MODE="worktree"
            shift
            ;;
        --subtree-only)
            TEST_MODE="subtree"
            shift
            ;;
        --skip-build)
            SKIP_BUILD=1
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Main execution
main() {
    echo "======================================================================"
    echo "  CodeScene MCP Server - Integration Tests"
    echo "======================================================================"
    echo ""
    
    check_prerequisites
    
    cd "$TEST_DIR"
    
    case $TEST_MODE in
        all)
            echo "Running comprehensive test suite..."
            if [ -n "$EXECUTABLE" ]; then
                python3 run_all_tests.py --executable "$EXECUTABLE"
            elif [ $SKIP_BUILD -eq 1 ]; then
                # Try to find previously built executable
                BUILT_EXEC="$SCRIPT_DIR/../cs_mcp_test_bin/cs-mcp"
                if [ -f "$BUILT_EXEC" ]; then
                    echo "Using previously built executable: $BUILT_EXEC"
                    python3 run_all_tests.py --executable "$BUILT_EXEC"
                else
                    echo -e "${RED}No previously built executable found${NC}"
                    echo "Run without --skip-build to build a new one"
                    exit 1
                fi
            else
                python3 run_all_tests.py
            fi
            ;;
        platform)
            echo "Running platform-specific tests..."
            if [ -z "$EXECUTABLE" ]; then
                echo -e "${RED}--platform-only requires --executable option${NC}"
                exit 1
            fi
            python3 test_platform_specific.py "$EXECUTABLE"
            ;;
        worktree)
            echo "Running git worktree tests..."
            if [ -z "$EXECUTABLE" ]; then
                echo -e "${RED}--worktree-only requires --executable option${NC}"
                exit 1
            fi
            python3 test_git_worktree.py "$EXECUTABLE"
            ;;
        subtree)
            echo "Running git subtree tests..."
            if [ -z "$EXECUTABLE" ]; then
                echo -e "${RED}--subtree-only requires --executable option${NC}"
                exit 1
            fi
            python3 test_git_subtree.py "$EXECUTABLE"
            ;;
    esac
    
    local exit_code=$?
    
    echo ""
    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}======================================================================"
        echo -e "  All tests passed! ✓"
        echo -e "======================================================================${NC}"
    else
        echo -e "${RED}======================================================================"
        echo -e "  Some tests failed ✗"
        echo -e "======================================================================${NC}"
    fi
    
    return $exit_code
}

main "$@"

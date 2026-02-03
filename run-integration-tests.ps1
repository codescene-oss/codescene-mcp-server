#!/usr/bin/env pwsh
#
# Run all integration tests for the CodeScene MCP Server (Windows/PowerShell)
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
#   .\run-integration-tests.ps1          # Build and run all tests
#   .\run-integration-tests.ps1 -Help    # Show help

param(
    [switch]$Help,
    [string]$Executable = "",
    [switch]$PlatformOnly,
    [switch]$WorktreeOnly,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

# Colors for output
function Write-Success {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Green
}

function Write-Error-Message {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Red
}

function Write-Warning-Message {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Yellow
}

function Write-Header {
    param([string]$Message)
    Write-Host ""
    Write-Host "======================================================================"
    Write-Host "  $Message"
    Write-Host "======================================================================"
    Write-Host ""
}

# Show help
function Show-Help {
    Write-Host @"
Run CodeScene MCP Server Integration Tests (Windows)

Usage:
  .\run-integration-tests.ps1 [OPTIONS]

Options:
  -Help              Show this help message
  -Executable PATH   Use existing executable (skip build)
  -PlatformOnly      Run only platform-specific tests
  -WorktreeOnly      Run only git worktree tests
  -SkipBuild         Skip build step (use previously built executable)

Environment Variables:
  CS_ACCESS_TOKEN    CodeScene access token (required)
  CS_ONPREM_URL      CodeScene URL (optional, defaults to https://codescene.io)

Examples:
  # Run all tests (builds automatically)
  .\run-integration-tests.ps1

  # Run with existing executable
  .\run-integration-tests.ps1 -Executable C:\path\to\cs-mcp.exe

  # Run only platform tests
  .\run-integration-tests.ps1 -PlatformOnly

"@
}

# Check prerequisites
function Check-Prerequisites {
    Write-Host "Checking prerequisites..."
    $missing = 0
    
    # Check Python version
    try {
        $pythonVersion = python --version 2>&1
        if ($pythonVersion -match "Python (\d+)\.(\d+)") {
            $major = [int]$matches[1]
            $minor = [int]$matches[2]
            if ($major -ge 3 -and $minor -ge 10) {
                Write-Success "✓ Python: $pythonVersion"
            } else {
                Write-Error-Message "✗ Python 3.10+ required, found: $pythonVersion"
                $missing++
            }
        } else {
            Write-Success "✓ Python: $pythonVersion"
        }
    } catch {
        Write-Error-Message "✗ Python not found"
        $missing++
    }
    
    # Check Git
    try {
        $gitVersion = git --version 2>&1
        Write-Success "✓ Git: $gitVersion"
    } catch {
        Write-Error-Message "✗ Git not found"
        $missing++
    }
    
    # Check CS_ACCESS_TOKEN
    if ($env:CS_ACCESS_TOKEN) {
        Write-Success "✓ CS_ACCESS_TOKEN is set"
    } else {
        Write-Error-Message "✗ CS_ACCESS_TOKEN not set"
        Write-Host "  Set it with: `$env:CS_ACCESS_TOKEN='your_token_here'"
        $missing++
    }
    
    # Check Nuitka
    try {
        python -c "import nuitka" 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Success "✓ Nuitka is installed"
        } else {
            Write-Warning-Message "! Nuitka not installed (required for building)"
            Write-Host "  Install with: pip install nuitka"
            $missing++
        }
    } catch {
        Write-Warning-Message "! Nuitka not installed (required for building)"
        Write-Host "  Install with: pip install nuitka"
        $missing++
    }
    
    if ($missing -gt 0) {
        Write-Host ""
        Write-Error-Message "Some prerequisites are missing. Please install them before running tests."
        exit 1
    }
    
    Write-Host ""
}

# Main execution
function Main {
    Write-Header "CodeScene MCP Server - Integration Tests (Windows)"
    
    if ($Help) {
        Show-Help
        exit 0
    }
    
    Check-Prerequisites
    
    $scriptDir = $PSScriptRoot
    $testDir = Join-Path $scriptDir "tests" "integration"
    
    Push-Location $testDir
    
    try {
        if ($PlatformOnly) {
            Write-Host "Running platform-specific tests..."
            if (-not $Executable) {
                Write-Error-Message "--PlatformOnly requires -Executable option"
                exit 1
            }
            python test_platform_specific.py $Executable
        }
        elseif ($WorktreeOnly) {
            Write-Host "Running git worktree tests..."
            if (-not $Executable) {
                Write-Error-Message "--WorktreeOnly requires -Executable option"
                exit 1
            }
            python test_git_worktree.py $Executable
        }
        else {
            Write-Host "Running comprehensive test suite..."
            if ($Executable) {
                python run_all_tests.py --executable $Executable
            }
            elseif ($SkipBuild) {
                # Try to find previously built executable
                $builtExec = Join-Path (Split-Path $scriptDir) "cs_mcp_test_bin" "cs-mcp.exe"
                if (Test-Path $builtExec) {
                    Write-Host "Using previously built executable: $builtExec"
                    python run_all_tests.py --executable $builtExec
                } else {
                    Write-Error-Message "No previously built executable found"
                    Write-Host "Run without -SkipBuild to build a new one"
                    exit 1
                }
            }
            else {
                python run_all_tests.py
            }
        }
        
        $exitCode = $LASTEXITCODE
        
        Write-Host ""
        if ($exitCode -eq 0) {
            Write-Success "======================================================================"
            Write-Success "  All tests passed! ✓"
            Write-Success "======================================================================"
        } else {
            Write-Error-Message "======================================================================"
            Write-Error-Message "  Some tests failed ✗"
            Write-Error-Message "======================================================================"
        }
        
        exit $exitCode
    }
    finally {
        Pop-Location
    }
}

# Run main function
Main

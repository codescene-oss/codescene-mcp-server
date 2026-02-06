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
    [switch]$SubtreeOnly,
    [switch]$SkipBuild,
    [switch]$Docker
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
    Write-Host "Run CodeScene MCP Server Integration Tests (Windows)"
    Write-Host ""
    Write-Host "Usage:"
    Write-Host "  .\run-integration-tests.ps1 [OPTIONS]"
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  -Help              Show this help message"
    Write-Host "  -Executable PATH   Use existing executable (skip build)"
    Write-Host "  -PlatformOnly      Run only platform-specific tests"
    Write-Host "  -WorktreeOnly      Run only git worktree tests"
    Write-Host "  -SubtreeOnly       Run only git subtree tests"
    Write-Host "  -SkipBuild         Skip build step (use previously built executable)"
    Write-Host "  -Docker            Run tests using Docker backend"
    Write-Host ""
    Write-Host "Environment Variables:"
    Write-Host "  CS_ACCESS_TOKEN    CodeScene access token (required)"
    Write-Host "  CS_ONPREM_URL      CodeScene URL (optional, defaults to https://codescene.io)"
    Write-Host ""
    Write-Host "Examples:"
    Write-Host "  # Run all tests (builds automatically)"
    Write-Host "  .\run-integration-tests.ps1"
    Write-Host ""
    Write-Host "  # Run with existing executable"
    Write-Host "  .\run-integration-tests.ps1 -Executable C:\path\to\cs-mcp.exe"
    Write-Host ""
    Write-Host "  # Run only platform tests"
    Write-Host "  .\run-integration-tests.ps1 -PlatformOnly"
    Write-Host ""
}

# Check if a command exists
function Test-CommandExists {
    param([string]$Command)
    try {
        $null = Get-Command $Command -ErrorAction Stop
        return $true
    } catch {
        return $false
    }
}

# Check Python version requirement
function Test-PythonVersion {
    try {
        $pythonVersion = python --version 2>&1
        if ($pythonVersion -match "Python (\d+)\.(\d+)") {
            $major = [int]$matches[1]
            $minor = [int]$matches[2]
            if ($major -ge 3 -and $minor -ge 10) {
                Write-Success "[OK] Python: $pythonVersion"
                return $true
            }
            Write-Error-Message "[X] Python 3.10+ required, found: $pythonVersion"
            return $false
        }
        Write-Success "[OK] Python: $pythonVersion"
        return $true
    } catch {
        Write-Error-Message "[X] Python not found"
        return $false
    }
}

# Check if Nuitka is installed
function Test-NuitkaInstalled {
    try {
        python -c "import nuitka" 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-Success "[OK] Nuitka is installed"
            return $true
        }
    } catch { }
    Write-Warning-Message "[!] Nuitka not installed (required for static backend)"
    Write-Host "  Install with: pip install nuitka"
    return $false
}

# Check Git is available
function Test-GitInstalled {
    if (Test-CommandExists "git") {
        Write-Success "[OK] Git: $(git --version)"
        return $true
    }
    Write-Error-Message "[X] Git not found"
    return $false
}

# Check CS_ACCESS_TOKEN is set
function Test-AccessToken {
    if ($env:CS_ACCESS_TOKEN) {
        Write-Success "[OK] CS_ACCESS_TOKEN is set"
        return $true
    }
    Write-Error-Message "[X] CS_ACCESS_TOKEN not set"
    Write-Host "  Set it with: `$env:CS_ACCESS_TOKEN='your_token_here'"
    return $false
}

# Check Docker is available
function Test-DockerInstalled {
    if (Test-CommandExists "docker") {
        Write-Success "[OK] Docker: $(docker --version)"
        return $true
    }
    Write-Error-Message "[X] Docker not found (required for docker backend)"
    return $false
}

# Check prerequisites based on backend
function Check-Prerequisites {
    param([string]$Backend)
    
    Write-Host "Checking prerequisites..."
    $allOk = (Test-PythonVersion) -and (Test-GitInstalled) -and (Test-AccessToken)
    
    if ($Backend -eq "static" -and -not (Test-NuitkaInstalled)) { $allOk = $false }
    if ($Backend -eq "docker" -and -not (Test-DockerInstalled)) { $allOk = $false }
    
    if (-not $allOk) {
        Write-Host ""
        Write-Error-Message "Some prerequisites are missing. Please install them before running tests."
        exit 1
    }
    
    Write-Host ""
}

# Run a specific test suite that requires an executable
function Invoke-SpecificTest {
    param([string]$TestScript, [string]$TestName)
    
    Write-Host "Running $TestName tests..."
    if (-not $Executable) {
        Write-Error-Message "-$TestName requires -Executable option"
        exit 1
    }
    python $TestScript $Executable
}

# Run comprehensive test suite
function Invoke-ComprehensiveTests {
    param([string]$Backend, [string]$RepoRoot)
    
    Write-Host "Running comprehensive test suite..."
    
    if ($Executable) {
        python run_all_tests.py --executable $Executable --backend $Backend
        return
    }
    
    if ($SkipBuild) {
        $builtExec = Join-Path (Split-Path $RepoRoot) "cs_mcp_test_bin" "cs-mcp.exe"
        if (Test-Path $builtExec) {
            Write-Host "Using previously built executable: $builtExec"
            python run_all_tests.py --executable $builtExec --backend $Backend
        } else {
            Write-Error-Message "No previously built executable found"
            Write-Host "Run without -SkipBuild to build a new one"
            exit 1
        }
        return
    }
    
    python run_all_tests.py --backend $Backend
}

# Display test results
function Show-TestResults {
    param([int]$ExitCode)
    
    Write-Host ""
    if ($ExitCode -eq 0) {
        Write-Success "======================================================================"
        Write-Success "  All tests passed!"
        Write-Success "======================================================================"
    } else {
        Write-Error-Message "======================================================================"
        Write-Error-Message "  Some tests failed"
        Write-Error-Message "======================================================================"
    }
}

# Get backend from command-line flags
function Get-Backend {
    if ($Docker) { return "docker" }
    return "static"
}

# Main execution
function Main {
    Write-Header "CodeScene MCP Server - Integration Tests (Windows)"
    
    if ($Help) {
        Show-Help
        exit 0
    }
    
    $backend = Get-Backend
    Write-Host "  Backend: $backend"
    Write-Host ""
    
    Check-Prerequisites -Backend $backend
    
    $scriptDir = $PSScriptRoot
    $testDir = Join-Path $scriptDir "integration"
    $repoRoot = Split-Path $scriptDir
    
    Push-Location $testDir
    
    try {
        if ($PlatformOnly) {
            Invoke-SpecificTest "test_platform_specific.py" "platform-specific"
        }
        elseif ($WorktreeOnly) {
            Invoke-SpecificTest "test_git_worktree.py" "git worktree"
        }
        elseif ($SubtreeOnly) {
            Invoke-SpecificTest "test_git_subtree.py" "git subtree"
        }
        else {
            Invoke-ComprehensiveTests -Backend $backend -RepoRoot $repoRoot
        }
        
        Show-TestResults -ExitCode $LASTEXITCODE
        exit $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
}

# Run main function
Main

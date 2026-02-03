# Windows Static Binary Path Resolution Integration Test Runner
#
# This script:
# 1. Checks for cs-mcp.exe (builds if missing and prerequisites are available)
# 2. Creates test git repositories with Windows paths
# 3. Runs path resolution tests to verify Windows path handling
# 4. Tests git worktree support on Windows
#
# Prerequisites:
# - Python 3.10+ (3.13 recommended)
# - Git
# - CS_ACCESS_TOKEN environment variable set
#
# Usage:
#   .\run-windows-test.ps1
#   .\run-windows-test.ps1 -Token "your_access_token"
#   .\run-windows-test.ps1 -BinaryPath "C:\path\to\cs-mcp.exe"

param(
    [Parameter(Mandatory=$false)]
    [string]$Token,
    
    [Parameter(Mandatory=$false)]
    [string]$BinaryPath
)

$ErrorActionPreference = "Stop"

# Get script directory and repo root
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Get-Item $ScriptDir).Parent.Parent.FullName

Write-Host ""
Write-Host "============================================================"
Write-Host "  Windows Static Binary Path Resolution Integration Test"
Write-Host "  Testing: cs-mcp.exe with Windows paths and worktrees"
Write-Host "============================================================"
Write-Host ""

# Set CS_ACCESS_TOKEN if provided
if ($Token) {
    $env:CS_ACCESS_TOKEN = $Token
    Write-Host "  Using provided access token"
}

# Check for access token
if (-not $env:CS_ACCESS_TOKEN) {
    Write-Host "  ERROR: CS_ACCESS_TOKEN environment variable not set" -ForegroundColor Red
    Write-Host ""
    Write-Host "  Set it with:"
    Write-Host "    `$env:CS_ACCESS_TOKEN = 'your_token_here'"
    Write-Host ""
    Write-Host "  Or pass it as a parameter:"
    Write-Host "    .\run-windows-test.ps1 -Token 'your_token_here'"
    Write-Host ""
    exit 1
}

Write-Host "  CS_ACCESS_TOKEN: [SET]" -ForegroundColor Green

# Check for Python
try {
    $pythonVersion = python --version 2>&1
    Write-Host "  Python: $pythonVersion" -ForegroundColor Green
} catch {
    Write-Host "  ERROR: Python not found in PATH" -ForegroundColor Red
    exit 1
}

# Check for Git
try {
    $gitVersion = git --version 2>&1
    Write-Host "  Git: $gitVersion" -ForegroundColor Green
} catch {
    Write-Host "  ERROR: Git not found in PATH" -ForegroundColor Red
    exit 1
}

# Determine binary path
if ($BinaryPath) {
    $Binary = $BinaryPath
} else {
    $Binary = Join-Path $RepoRoot "cs-mcp.exe"
}

if (Test-Path $Binary) {
    Write-Host "  Binary: $Binary" -ForegroundColor Green
} else {
    Write-Host "  Binary not found: $Binary" -ForegroundColor Yellow
    Write-Host "  The test script will attempt to build it..."
}

Write-Host ""
Write-Host "Running tests..."
Write-Host ""

# Change to script directory and run tests
Push-Location $ScriptDir
try {
    if ($BinaryPath) {
        python test_windows_static.py $BinaryPath
    } else {
        python test_windows_static.py
    }
    $exitCode = $LASTEXITCODE
} finally {
    Pop-Location
}

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "============================================================"
    Write-Host "  All tests passed!" -ForegroundColor Green
    Write-Host "============================================================"
} else {
    Write-Host "============================================================"
    Write-Host "  Some tests failed!" -ForegroundColor Red
    Write-Host "============================================================"
}

exit $exitCode

# CodeScene MCP Server Uninstaller for Windows
# Usage: irm https://raw.githubusercontent.com/codescene-oss/codescene-mcp-server/main/uninstall.ps1 | iex

$ErrorActionPreference = "Stop"

$installDir = "$env:LOCALAPPDATA\Programs\cs-mcp"

Write-Host "Uninstalling CodeScene MCP Server..." -ForegroundColor Cyan

# Remove install directory
if (Test-Path $installDir) {
    Remove-Item -Recurse -Force $installDir
    Write-Host "Removed $installDir" -ForegroundColor Green
} else {
    Write-Host "Install directory not found: $installDir" -ForegroundColor Yellow
}

# Remove from PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -like "*$installDir*") {
    $newPath = ($userPath -split ';' | Where-Object { $_ -ne $installDir }) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "Removed from PATH" -ForegroundColor Green
}

Write-Host ""
Write-Host "Successfully uninstalled CodeScene MCP Server!" -ForegroundColor Green

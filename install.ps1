# CodeScene MCP Server Installer for Windows
# Usage: irm https://raw.githubusercontent.com/codescene-oss/codescene-mcp-server/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$installDir = "$env:LOCALAPPDATA\Programs\cs-mcp"

Write-Host "Installing CodeScene MCP Server..." -ForegroundColor Cyan

# Create install directory
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

# Get latest release info
$latestRelease = Invoke-RestMethod "https://api.github.com/repos/codescene-oss/codescene-mcp-server/releases/latest"
$version = $latestRelease.tag_name -replace '^MCP-', ''
$downloadUrl = "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-$version/cs-mcp-windows-amd64.exe"

Write-Host "Downloading cs-mcp v$version..." -ForegroundColor Cyan

# Download the executable
Invoke-WebRequest -Uri $downloadUrl -OutFile "$installDir\cs-mcp.exe" -UseBasicParsing

# Add to user PATH if not already present
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    Write-Host "Added $installDir to PATH" -ForegroundColor Green
}

Write-Host ""
Write-Host "Successfully installed cs-mcp v$version!" -ForegroundColor Green
Write-Host "Location: $installDir\cs-mcp.exe" -ForegroundColor Gray
Write-Host ""
Write-Host "Please restart your terminal, then run 'cs-mcp --version' to verify." -ForegroundColor Yellow

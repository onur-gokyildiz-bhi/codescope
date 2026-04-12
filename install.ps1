#!/usr/bin/env pwsh
# Codescope Installer for Windows
# Usage: irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.ps1 | iex

$ErrorActionPreference = "Stop"
$REPO = "onur-gokyildiz-bhi/codescope"

Write-Host ""
Write-Host "  Codescope Installer" -ForegroundColor Cyan
Write-Host "  ===================" -ForegroundColor Cyan
Write-Host ""

# Detect architecture
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($arch -ne "X64") {
    Write-Host "  Error: Only x86_64 (64-bit) Windows is supported." -ForegroundColor Red
    Write-Host "  Detected architecture: $arch" -ForegroundColor Red
    exit 1
}

$target = "x86_64-pc-windows-msvc"

# Detect install directory: if codescope is already on PATH, update in-place.
# Otherwise fall back to ~/.local/bin (cross-platform convention).
$existing = Get-Command codescope -ErrorAction SilentlyContinue
if ($existing) {
    $INSTALL_DIR = Split-Path $existing.Source
    Write-Host "  Existing install detected: $INSTALL_DIR" -ForegroundColor Green
} else {
    $INSTALL_DIR = "$env:USERPROFILE\.local\bin"
    Write-Host "  Fresh install to: $INSTALL_DIR" -ForegroundColor Yellow
}

# Get latest release
Write-Host "  Fetching latest release..." -ForegroundColor Yellow
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$REPO/releases/latest" -Headers @{ "User-Agent" = "codescope-installer" }
    $version = $release.tag_name
    Write-Host "  Latest version: $version" -ForegroundColor Green
} catch {
    Write-Host "  Error: Could not fetch latest release from GitHub." -ForegroundColor Red
    Write-Host "  Check your internet connection or visit https://github.com/$REPO/releases" -ForegroundColor Red
    exit 1
}

# Find the Windows asset
$assetName = "codescope-$version-$target.zip"
$asset = $release.assets | Where-Object { $_.name -eq $assetName }

if (-not $asset) {
    Write-Host "  Error: Asset '$assetName' not found in release $version" -ForegroundColor Red
    Write-Host "  Available assets:" -ForegroundColor Yellow
    $release.assets | ForEach-Object { Write-Host "    - $($_.name)" }
    exit 1
}

$downloadUrl = $asset.browser_download_url

# Create install directory
if (-not (Test-Path $INSTALL_DIR)) {
    New-Item -ItemType Directory -Force -Path $INSTALL_DIR | Out-Null
}

# Stop running codescope processes before overwriting binaries
$procs = Get-Process -Name "codescope*" -ErrorAction SilentlyContinue
if ($procs) {
    Write-Host "  Stopping running codescope processes..." -ForegroundColor Yellow
    $procs | Stop-Process -Force
    Start-Sleep -Seconds 1
}

# Download
$tempZip = Join-Path $env:TEMP "codescope-$version.zip"
Write-Host "  Downloading $assetName..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $downloadUrl -OutFile $tempZip -UseBasicParsing

# Extract
Write-Host "  Extracting to $INSTALL_DIR..." -ForegroundColor Yellow
$tempExtract = Join-Path $env:TEMP "codescope-extract"
if (Test-Path $tempExtract) { Remove-Item -Recurse -Force $tempExtract }
Expand-Archive -Path $tempZip -DestinationPath $tempExtract -Force

# Copy binaries
$installed = @()
foreach ($bin in @("codescope.exe", "codescope-mcp.exe", "codescope-web.exe")) {
    $src = Join-Path $tempExtract $bin
    if (Test-Path $src) {
        Copy-Item $src "$INSTALL_DIR\$bin" -Force
        $installed += $bin
    }
}

# Cleanup
Remove-Item $tempZip -Force -ErrorAction SilentlyContinue
Remove-Item $tempExtract -Recurse -Force -ErrorAction SilentlyContinue

# Add to PATH if not already there
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$INSTALL_DIR*") {
    Write-Host "  Adding to PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$currentPath;$INSTALL_DIR", "User")
    $env:Path = "$env:Path;$INSTALL_DIR"
    Write-Host "  Added $INSTALL_DIR to user PATH" -ForegroundColor Green
} else {
    Write-Host "  Already in PATH" -ForegroundColor Green
}

# Verify
$newVersion = & "$INSTALL_DIR\codescope.exe" --version 2>$null
Write-Host ""
Write-Host "  Installation complete! ($newVersion)" -ForegroundColor Green
Write-Host ""
Write-Host "  Installed:" -ForegroundColor Cyan
foreach ($bin in $installed) {
    Write-Host "    $bin -> $INSTALL_DIR\$bin"
}
Write-Host ""
Write-Host "  Quick start:" -ForegroundColor Cyan
Write-Host "    cd your-project"
Write-Host "    codescope init"
Write-Host ""
Write-Host "  That's it! Open the project in Claude Code and" -ForegroundColor Green
Write-Host "  Codescope starts automatically with 52 MCP tools." -ForegroundColor Green
Write-Host ""
Write-Host "  NOTE: Restart your terminal for PATH changes to take effect." -ForegroundColor Yellow
Write-Host ""

#!/usr/bin/env pwsh
# Codescope update checker — run as a session start hook (Windows)
# Install: add to .claude/settings.json hooks.SessionStart

$REPO = "onur-gokyildiz-bhi/codescope"
$CACHE_DIR = "$env:USERPROFILE\.codescope"
$CACHE_FILE = "$CACHE_DIR\update-check"
$CHECK_INTERVAL = 86400  # 24 hours

# Skip if checked recently
if (Test-Path $CACHE_FILE) {
    $lines = Get-Content $CACHE_FILE -ErrorAction SilentlyContinue
    if ($lines.Count -ge 1) {
        $lastCheck = [int64]$lines[0]
        $now = [int64](Get-Date -UFormat %s)
        if (($now - $lastCheck) -lt $CHECK_INTERVAL) {
            if ($lines.Count -ge 2 -and $lines[1]) {
                Write-Host $lines[1]
            }
            exit 0
        }
    }
}

# Get current version
try {
    $current = (codescope --version 2>$null) -replace '.*?(\d+\.\d+\.\d+).*', '$1'
} catch {
    exit 0
}
if (-not $current) { exit 0 }

# Get latest version
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$REPO/releases/latest" `
        -Headers @{ "User-Agent" = "codescope-update-check" } `
        -TimeoutSec 5 -ErrorAction Stop
    $latest = $release.tag_name -replace '^v', ''
} catch {
    exit 0
}

# Save check timestamp
if (-not (Test-Path $CACHE_DIR)) { New-Item -ItemType Directory -Force -Path $CACHE_DIR | Out-Null }
$now = [int64](Get-Date -UFormat %s)

if ($current -ne $latest) {
    $msg = "[codescope] Update available: v$current -> v$latest. Run /cs-update to upgrade."
    Set-Content $CACHE_FILE "$now`n$msg"
    Write-Host $msg
} else {
    Set-Content $CACHE_FILE "$now"
}

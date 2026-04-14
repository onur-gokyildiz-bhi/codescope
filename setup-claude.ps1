#!/usr/bin/env pwsh
# Codescope — AI Agent Setup Wizard (Windows)
# Install or uninstall codescope skills, MCP config, and rules.
#
# Usage:
#   irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/setup-claude.ps1 | iex
#   setup-claude.ps1 --uninstall

$ErrorActionPreference = "Stop"
$REPO_RAW = "https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main"

# --- Mode: install or uninstall ---

$uninstallMode = $false
if ($args -contains "--uninstall" -or $args -contains "-u" -or $args -contains "uninstall") {
    $uninstallMode = $true
}

# If codescope is already installed, ask the user
if (-not $uninstallMode -and (Get-Command codescope -ErrorAction SilentlyContinue)) {
    Write-Host ""
    Write-Host "  Codescope is already installed." -ForegroundColor Green
    Write-Host ""
    Write-Host "    1) Reinstall / Update"
    Write-Host "    2) Uninstall"
    Write-Host "    3) Cancel"
    Write-Host ""
    $modeChoice = Read-Host "  Select [1-3]"
    switch ($modeChoice) {
        "2" { $uninstallMode = $true }
        "3" { Write-Host "  Cancelled."; exit 0 }
    }
}

if ($uninstallMode) {
    Write-Host ""
    Write-Host "  +==========================================+" -ForegroundColor Red
    Write-Host "  |   Codescope - Uninstall                  |" -ForegroundColor Red
    Write-Host "  +==========================================+" -ForegroundColor Red
    Write-Host ""

    # 1. Remove binaries
    Write-Host "  [1/6] Removing binaries..." -ForegroundColor Yellow
    $binDir = "$env:USERPROFILE\.local\bin"
    foreach ($bin in @("codescope.exe", "codescope-mcp.exe", "codescope-web.exe")) {
        $path = "$binDir\$bin"
        if (Test-Path $path) {
            Remove-Item $path -Force -ErrorAction SilentlyContinue
            Write-Host "         - $bin" -ForegroundColor Red
        }
    }

    # 2. Remove skills
    Write-Host "  [2/6] Removing skills..." -ForegroundColor Yellow
    $skillsDir = "$env:USERPROFILE\.claude\skills"
    $skillNames = @("codescope", "cs-search", "cs-index", "cs-stats", "cs-ask", "cs-impact", "cs-callers", "cs-file", "cs-query", "cs-update")
    $removed = 0
    foreach ($skill in $skillNames) {
        $path = "$skillsDir\$skill"
        if (Test-Path $path) {
            Remove-Item $path -Recurse -Force -ErrorAction SilentlyContinue
            $removed++
        }
    }
    Write-Host "         - $removed skills removed" -ForegroundColor Red

    # 3. Remove MCP config from ~/.claude.json
    Write-Host "  [3/6] Removing MCP config..." -ForegroundColor Yellow
    $claudeJson = "$env:USERPROFILE\.claude.json"
    if (Test-Path $claudeJson) {
        $content = Get-Content $claudeJson -Raw -ErrorAction SilentlyContinue
        if ($content -match "codescope") {
            try {
                $config = $content | ConvertFrom-Json
                if ($config.mcpServers -and $config.mcpServers.codescope) {
                    $config.mcpServers.PSObject.Properties.Remove("codescope")
                    $config | ConvertTo-Json -Depth 10 | Set-Content $claudeJson -Encoding UTF8
                    Write-Host "         - codescope removed from ~/.claude.json" -ForegroundColor Red
                }
            } catch {
                Write-Host "         ! Could not parse ~/.claude.json, remove codescope entry manually" -ForegroundColor Yellow
            }
        } else {
            Write-Host "         ~ No codescope entry in ~/.claude.json" -ForegroundColor Gray
        }
    }

    # 4. Remove rules
    Write-Host "  [4/6] Removing rules..." -ForegroundColor Yellow
    $rulesFile = "$env:USERPROFILE\.claude\rules\codescope-mandatory.md"
    if (Test-Path $rulesFile) {
        Remove-Item $rulesFile -Force -ErrorAction SilentlyContinue
        Write-Host "         - codescope-mandatory.md" -ForegroundColor Red
    } else {
        Write-Host "         ~ No rules file found" -ForegroundColor Gray
    }

    # 5. Remove marketplace plugin from settings.json
    Write-Host "  [5/6] Removing marketplace plugin..." -ForegroundColor Yellow
    $settingsFile = "$env:USERPROFILE\.claude\settings.json"
    if (Test-Path $settingsFile) {
        $settingsContent = Get-Content $settingsFile -Raw -ErrorAction SilentlyContinue
        if ($settingsContent -match "codescope") {
            try {
                $settings = $settingsContent | ConvertFrom-Json
                if ($settings.extraKnownMarketplaces -and $settings.extraKnownMarketplaces.codescope) {
                    $settings.extraKnownMarketplaces.PSObject.Properties.Remove("codescope")
                    $settings | ConvertTo-Json -Depth 10 | Set-Content $settingsFile -Encoding UTF8
                    Write-Host "         - codescope removed from marketplace plugins" -ForegroundColor Red
                }
                if ($settings.enabledPlugins) {
                    $pluginKeys = @($settings.enabledPlugins.PSObject.Properties.Name | Where-Object { $_ -match "codescope" })
                    foreach ($key in $pluginKeys) {
                        $settings.enabledPlugins.PSObject.Properties.Remove($key)
                    }
                    if ($pluginKeys.Count -gt 0) {
                        $settings | ConvertTo-Json -Depth 10 | Set-Content $settingsFile -Encoding UTF8
                        Write-Host "         - $($pluginKeys.Count) plugin entries removed" -ForegroundColor Red
                    }
                }
            } catch {
                Write-Host "         ! Could not parse settings.json, remove codescope entries manually" -ForegroundColor Yellow
            }
        } else {
            Write-Host "         ~ No codescope in settings.json" -ForegroundColor Gray
        }
    }

    # 6. Optionally remove database
    Write-Host "  [6/6] Database cleanup..." -ForegroundColor Yellow
    $dbDir = "$env:USERPROFILE\.codescope"
    if (Test-Path $dbDir) {
        Write-Host "         Database found at ~/.codescope" -ForegroundColor Yellow
        $deleteDb = Read-Host "         Delete database? This removes all indexed data. [y/N]"
        if ($deleteDb -eq "y" -or $deleteDb -eq "Y") {
            Remove-Item $dbDir -Recurse -Force -ErrorAction SilentlyContinue
            Write-Host "         - ~/.codescope removed" -ForegroundColor Red
        } else {
            Write-Host "         ~ Database kept at ~/.codescope" -ForegroundColor Gray
        }
    }

    Write-Host ""
    Write-Host "  +==========================================+" -ForegroundColor Red
    Write-Host "  |   Codescope uninstalled.                 |" -ForegroundColor Red
    Write-Host "  +==========================================+" -ForegroundColor Red
    Write-Host ""
    Write-Host "  Note: project-level .mcp.json files are not removed." -ForegroundColor Gray
    Write-Host "  Delete them manually if needed." -ForegroundColor Gray
    Write-Host ""
    exit 0
}

# ═══════════════════════════════════════════════════════════════
# INSTALL MODE (unchanged from here)
# ═══════════════════════════════════════════════════════════════

Write-Host ""
Write-Host "  +==========================================+" -ForegroundColor Cyan
Write-Host "  |   Codescope - AI Agent Setup Wizard      |" -ForegroundColor Cyan
Write-Host "  +==========================================+" -ForegroundColor Cyan
Write-Host ""

# --- Step 1: Detect or ask which CLI ---

$agents = @()
if (Get-Command claude -ErrorAction SilentlyContinue) { $agents += "claude-code" }
if (Get-Command codex -ErrorAction SilentlyContinue) { $agents += "codex-cli" }
if (Get-Command cursor -ErrorAction SilentlyContinue) { $agents += "cursor" }
if (Get-Command zed -ErrorAction SilentlyContinue) { $agents += "zed" }
if (Get-Command gemini -ErrorAction SilentlyContinue) { $agents += "gemini-cli" }

if ($agents.Count -eq 0) {
    Write-Host "  No AI agent CLI detected on PATH." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "  Which agent do you use?"
    Write-Host "    1) Claude Code"
    Write-Host "    2) Codex CLI"
    Write-Host "    3) Cursor"
    Write-Host "    4) Zed"
    Write-Host "    5) Gemini CLI"
    Write-Host "    6) All of the above"
    Write-Host ""
    $choice = Read-Host "  Select [1-6]"
    switch ($choice) {
        "1" { $agents = @("claude-code") }
        "2" { $agents = @("codex-cli") }
        "3" { $agents = @("cursor") }
        "4" { $agents = @("zed") }
        "5" { $agents = @("gemini-cli") }
        default { $agents = @("claude-code", "codex-cli", "cursor", "zed", "gemini-cli") }
    }
} elseif ($agents.Count -eq 1) {
    Write-Host "  Detected: $($agents[0])" -ForegroundColor Green
} else {
    Write-Host "  Detected: $($agents -join ', ')" -ForegroundColor Green
}
Write-Host ""

# --- Step 2: Install codescope binary if missing ---

Write-Host "  [1/5] Checking codescope binary..." -ForegroundColor Yellow
$cs = Get-Command codescope -ErrorAction SilentlyContinue
if ($cs) {
    $ver = (codescope --version 2>$null) -replace '.*?(\d+\.\d+\.\d+).*', '$1'
    Write-Host "         + codescope $ver" -ForegroundColor Green
} else {
    Write-Host "         Installing codescope..." -ForegroundColor Yellow
    irm "$REPO_RAW/install.ps1" | iex
    Write-Host ""
}

# --- Step 3: Install skills ---

Write-Host "  [2/5] Installing skills..." -ForegroundColor Yellow

$skills = @("codescope", "cs-search", "cs-index", "cs-stats", "cs-ask", "cs-impact", "cs-callers", "cs-file", "cs-query", "cs-update")

function Install-SkillsTo($dir, $label) {
    foreach ($skill in $skills) {
        $skillDir = "$dir\$skill"
        if (-not (Test-Path $skillDir)) { New-Item -ItemType Directory -Force -Path $skillDir | Out-Null }
        Invoke-WebRequest -Uri "$REPO_RAW/skills/$skill/SKILL.md" -OutFile "$skillDir\SKILL.md" -UseBasicParsing 2>$null
    }
    $refDir1 = "$dir\codescope\references"
    $refDir2 = "$dir\cs-query\references"
    if (-not (Test-Path $refDir1)) { New-Item -ItemType Directory -Force -Path $refDir1 | Out-Null }
    if (-not (Test-Path $refDir2)) { New-Item -ItemType Directory -Force -Path $refDir2 | Out-Null }
    Invoke-WebRequest -Uri "$REPO_RAW/skills/codescope/references/TOOLS.md" -OutFile "$refDir1\TOOLS.md" -UseBasicParsing 2>$null
    Invoke-WebRequest -Uri "$REPO_RAW/skills/cs-query/references/SURREALQL.md" -OutFile "$refDir2\SURREALQL.md" -UseBasicParsing 2>$null
    Write-Host "         + $($skills.Count) skills + 2 references -> $label" -ForegroundColor Green
}

foreach ($agent in $agents) {
    switch ($agent) {
        "claude-code" {
            Install-SkillsTo "$env:USERPROFILE\.claude\skills" "Claude Code (~/.claude/skills/)"
        }
        "codex-cli" {
            $codexDir = if ($env:CODEX_SKILLS_DIR) { $env:CODEX_SKILLS_DIR } else { "$env:USERPROFILE\.codex\skills" }
            Install-SkillsTo "$codexDir\codescope" "Codex CLI"
        }
        default {
            Install-SkillsTo "$env:USERPROFILE\.claude\skills" "$agent (via ~/.claude/skills/ fallback)"
        }
    }
}

# --- Step 4: Configure MCP server ---

Write-Host "  [3/5] Configuring MCP server..." -ForegroundColor Yellow

function Configure-McpJson($configFile, $label) {
    if (Test-Path $configFile) {
        $content = Get-Content $configFile -Raw -ErrorAction SilentlyContinue
        if ($content -match "codescope") {
            Write-Host "         + $label - already configured" -ForegroundColor Green
            return
        }
        try {
            $config = $content | ConvertFrom-Json
            if (-not $config.mcpServers) {
                $config | Add-Member -NotePropertyName "mcpServers" -NotePropertyValue @{} -Force
            }
            $config.mcpServers | Add-Member -NotePropertyName "codescope" -NotePropertyValue @{
                command = "codescope"
                args = @("mcp", ".", "--auto-index")
            } -Force
            $config | ConvertTo-Json -Depth 10 | Set-Content $configFile -Encoding UTF8
            Write-Host "         + $label - added codescope" -ForegroundColor Green
        } catch {
            Write-Host "         ! $label - could not parse, add manually" -ForegroundColor Yellow
        }
    } else {
        @{
            mcpServers = @{
                codescope = @{
                    command = "codescope"
                    args = @("mcp", ".", "--auto-index")
                }
            }
        } | ConvertTo-Json -Depth 10 | Set-Content $configFile -Encoding UTF8
        Write-Host "         + $label - created" -ForegroundColor Green
    }
}

# Check for marketplace install
$marketplaceDetected = $false
$settingsFile = "$env:USERPROFILE\.claude\settings.json"
if (Test-Path $settingsFile) {
    $settingsContent = Get-Content $settingsFile -Raw -ErrorAction SilentlyContinue
    if ($settingsContent -match "extraKnownMarketplaces.*codescope") {
        $marketplaceDetected = $true
        Write-Host "         ! Marketplace plugin detected — skipping global MCP config" -ForegroundColor Yellow
    }
}

foreach ($agent in $agents) {
    switch ($agent) {
        "claude-code" {
            if ($marketplaceDetected) {
                Write-Host "         -> Claude Code: using marketplace plugin (no global MCP needed)" -ForegroundColor Green
                Write-Host "         -> Run 'codescope init' in each project for .mcp.json" -ForegroundColor Cyan
            } else {
                Configure-McpJson "$env:USERPROFILE\.claude.json" "~/.claude.json (Claude Code)"
            }
        }
        "codex-cli" { Configure-McpJson "$env:USERPROFILE\.codex.json" "~/.codex.json (Codex CLI)" }
        "cursor" { Write-Host "         -> Cursor: add codescope to .cursor/mcp.json in your project" }
        "zed" { Write-Host "         -> Zed: add codescope to .zed/mcp.json in your project" }
        default { Write-Host "         -> $agent : run 'codescope init' in each project" }
    }
}

# --- Step 5: Install rules ---

Write-Host "  [4/5] Installing mandatory rules..." -ForegroundColor Yellow
foreach ($agent in $agents) {
    if ($agent -eq "claude-code") {
        # Global rules
        $rulesDir = "$env:USERPROFILE\.claude\rules"
        if (-not (Test-Path $rulesDir)) { New-Item -ItemType Directory -Force -Path $rulesDir | Out-Null }
        Invoke-WebRequest -Uri "$REPO_RAW/.claude/rules/codescope-mandatory.md" -OutFile "$rulesDir\codescope-mandatory.md" -UseBasicParsing 2>$null
        Write-Host "         + $rulesDir\codescope-mandatory.md (global)" -ForegroundColor Green

        # Project-level rules if we're in a project
        $projectClaude = Join-Path (Get-Location) ".claude"
        if (Test-Path $projectClaude) {
            $projectRules = Join-Path $projectClaude "rules"
            if (-not (Test-Path $projectRules)) { New-Item -ItemType Directory -Force -Path $projectRules | Out-Null }
            Invoke-WebRequest -Uri "$REPO_RAW/.claude/rules/codescope-mandatory.md" -OutFile "$projectRules\codescope-mandatory.md" -UseBasicParsing 2>$null
            Write-Host "         + $projectRules\codescope-mandatory.md (project)" -ForegroundColor Green
        }
    }
}

# --- Step 6: Verify ---

Write-Host "  [5/5] Verifying..." -ForegroundColor Yellow

$pass = 0; $fail = 0
if (Get-Command codescope -ErrorAction SilentlyContinue) {
    Write-Host "         + codescope binary on PATH" -ForegroundColor Green; $pass++
} else {
    Write-Host "         x codescope NOT on PATH" -ForegroundColor Red; $fail++
}
if (Get-Command codescope-mcp -ErrorAction SilentlyContinue) {
    Write-Host "         + codescope-mcp binary on PATH" -ForegroundColor Green; $pass++
} else {
    Write-Host "         x codescope-mcp NOT on PATH" -ForegroundColor Red; $fail++
}
$skillCount = (Get-ChildItem "$env:USERPROFILE\.claude\skills" -Filter "SKILL.md" -Recurse -ErrorAction SilentlyContinue | Where-Object { $_.DirectoryName -match "cs-|codescope" }).Count
if ($skillCount -ge 10) {
    Write-Host "         + $skillCount skills installed" -ForegroundColor Green; $pass++
} else {
    Write-Host "         x Only $skillCount skills (expected 10+)" -ForegroundColor Red; $fail++
}

Write-Host ""
if ($fail -eq 0) {
    Write-Host "  +==========================================+" -ForegroundColor Green
    Write-Host "  |   + Setup complete! All checks passed.   |" -ForegroundColor Green
    Write-Host "  +==========================================+" -ForegroundColor Green
} else {
    Write-Host "  ! Setup done with $fail issue(s). Run 'codescope doctor .' for details." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "  Quick start:" -ForegroundColor Cyan
Write-Host "    cd C:\path\to\project"
Write-Host "    codescope init            # index + create .mcp.json"
Write-Host "    codescope doctor .        # verify everything works"
Write-Host ""
Write-Host "  To uninstall later:" -ForegroundColor Gray
Write-Host "    setup-claude.ps1 --uninstall" -ForegroundColor Gray
Write-Host ""

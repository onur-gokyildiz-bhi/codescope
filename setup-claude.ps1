#!/usr/bin/env pwsh
# Codescope — Claude Code Integration Setup (Windows)
# Installs: MCP server config, skills, hooks, CLAUDE.md
#
# Usage: irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/setup-claude.ps1 | iex

$ErrorActionPreference = "Stop"
$REPO_RAW = "https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main"
$CLAUDE_DIR = "$env:USERPROFILE\.claude"
$SKILLS_DIR = "$CLAUDE_DIR\skills"

Write-Host ""
Write-Host "  Codescope - Claude Code Integration Setup" -ForegroundColor Cyan
Write-Host "  ==========================================" -ForegroundColor Cyan
Write-Host ""

# 1. Check codescope-mcp is installed
$mcpPath = Get-Command codescope-mcp -ErrorAction SilentlyContinue
if (-not $mcpPath) {
    Write-Host "  codescope-mcp not found. Installing..." -ForegroundColor Yellow
    irm "$REPO_RAW/install.ps1" | iex
    Write-Host ""
}

# 2. Configure MCP server in ~/.claude.json
Write-Host "  [1/4] Configuring MCP server..." -ForegroundColor Yellow
$claudeJson = "$env:USERPROFILE\.claude.json"

if (Test-Path $claudeJson) {
    $config = Get-Content $claudeJson -Raw | ConvertFrom-Json
    if ($config.mcpServers.codescope) {
        Write-Host "         Already configured in $claudeJson" -ForegroundColor Green
    } else {
        if (-not $config.mcpServers) {
            $config | Add-Member -NotePropertyName "mcpServers" -NotePropertyValue @{} -Force
        }
        $config.mcpServers | Add-Member -NotePropertyName "codescope" -NotePropertyValue @{
            command = "codescope-mcp"
            args = @(".", "--auto-index")
        } -Force
        $config | ConvertTo-Json -Depth 10 | Set-Content $claudeJson -Encoding UTF8
        Write-Host "         Added codescope to existing $claudeJson" -ForegroundColor Green
    }
} else {
    @{
        mcpServers = @{
            codescope = @{
                command = "codescope-mcp"
                args = @(".", "--auto-index")
            }
        }
    } | ConvertTo-Json -Depth 10 | Set-Content $claudeJson -Encoding UTF8
    Write-Host "         Created $claudeJson" -ForegroundColor Green
}

# 3. Install skills
Write-Host "  [2/4] Installing skills..." -ForegroundColor Yellow
$skills = @("codescope", "cs-search", "cs-index", "cs-stats", "cs-ask", "cs-impact", "cs-callers", "cs-file", "cs-query")

foreach ($skill in $skills) {
    $skillDir = "$SKILLS_DIR\$skill"
    if (-not (Test-Path $skillDir)) {
        New-Item -ItemType Directory -Force -Path $skillDir | Out-Null
    }
    Invoke-WebRequest -Uri "$REPO_RAW/templates/skills/$skill/SKILL.md" -OutFile "$skillDir\SKILL.md" -UseBasicParsing
}
Write-Host "         Installed $($skills.Count) skills to $SKILLS_DIR\" -ForegroundColor Green

# 4. Hooks template
Write-Host "  [3/4] Hooks template..." -ForegroundColor Yellow
Write-Host "         To add auto-index on session start, add to your project's"
Write-Host "         .claude\settings.json (see docs)"
Write-Host ""

# 5. CLAUDE.md template
Write-Host "  [4/4] CLAUDE.md template..." -ForegroundColor Yellow
Invoke-WebRequest -Uri "$REPO_RAW/templates/CLAUDE.md" -OutFile "$env:TEMP\codescope-CLAUDE.md" -UseBasicParsing
Write-Host "         Template saved to $env:TEMP\codescope-CLAUDE.md" -ForegroundColor Green
Write-Host "         Copy to your project: copy $env:TEMP\codescope-CLAUDE.md .\CLAUDE.md"

Write-Host ""
Write-Host "  Setup complete!" -ForegroundColor Green
Write-Host "  ===============" -ForegroundColor Green
Write-Host ""
Write-Host "  Available commands in Claude Code:" -ForegroundColor Cyan
Write-Host "    /codescope          - Main menu & routing"
Write-Host "    /cs-search <name>   - Search functions"
Write-Host "    /cs-index           - Re-index project"
Write-Host "    /cs-stats           - Codebase overview"
Write-Host "    /cs-ask <question>  - Ask in Turkish or English"
Write-Host "    /cs-impact <func>   - Impact analysis"
Write-Host "    /cs-callers <func>  - Who calls this function?"
Write-Host "    /cs-file <path>     - All entities in a file"
Write-Host "    /cs-query <surql>   - Raw SurrealQL query"
Write-Host ""
Write-Host "  Start Claude Code in any project:" -ForegroundColor Cyan
Write-Host "    cd C:\path\to\project; claude"
Write-Host ""

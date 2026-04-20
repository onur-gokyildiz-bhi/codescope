# RTK-03 — PowerShell mirror of `codescope-bash-suggest.sh`.
#
# Claude Code on Windows pipes the same JSON envelope on stdin.
# This script extracts the `command` field and nudges the model
# either toward a codescope MCP tool (for code reads / searches)
# or toward `codescope exec <cmd>` (for noisy dev commands like
# cargo, pytest, tsc). Informational only by default — set
# `$env:CODESCOPE_HOOK_BLOCK = '1'` to make matched patterns
# veto the bash run.
#
# Install via `codescope hook --agent claude-code`.

$ErrorActionPreference = 'Stop'

$payload = [Console]::In.ReadToEnd()
try {
    $parsed = $payload | ConvertFrom-Json
    $cmd = $parsed.tool_input.command
} catch {
    exit 0  # can't parse → don't get in the way
}
if (-not $cmd) { exit 0 }

# Already wrapped — nothing to suggest.
if ($cmd -match '^\s*codescope\s+exec\s') { exit 0 }

function Suggest-Tool($label, $tool) {
    [Console]::Error.WriteLine("⟿ codescope: $label → prefer ``$tool`` for 80–95% less context")
    [Console]::Error.WriteLine("  (set CODESCOPE_HOOK_BLOCK=1 to make this hard-fail)")
    if ($env:CODESCOPE_HOOK_BLOCK -eq '1') { exit 2 }
}

function Suggest-Exec($inner, $blurb) {
    [Console]::Error.WriteLine("⟿ codescope: prefix with ``codescope exec`` → $blurb")
    [Console]::Error.WriteLine("  e.g.  codescope exec $inner")
    if ($env:CODESCOPE_HOOK_BLOCK -eq '1') { exit 2 }
}

$srcRe = '\.(rs|ts|tsx|js|jsx|py|go|java|cpp|c|h|hpp|kt|swift|rb|php|sol|vue|svelte)'

# --- class (a): use an MCP tool instead ---------------------------

if ($cmd -match "^(cat|less|more|bat)\s+[^|]+$srcRe\s*$") {
    Suggest-Tool 'reading source file' 'context_bundle(file_path)'; exit 0
}
if ($cmd -match "^(head|tail)\s+.*$srcRe") {
    Suggest-Tool 'peeking at source file' 'context_bundle(file_path)'; exit 0
}
if ($cmd -match '^(grep|rg|ag)\s+' -and
    $cmd -match '(-r|--recursive|\s[^\s\.]+/[^\s]+|\s\.\s*$)') {
    Suggest-Tool 'grep across codebase' 'search(query, mode=fuzzy) or find_function(name)'; exit 0
}
if ($cmd -match '^find\s+.*-name') {
    Suggest-Tool 'find by filename' 'search(query, mode=file)'; exit 0
}
if ($cmd -match '^git\s+blame(\s|$)') {
    Suggest-Tool 'git blame' 'file_churn(path) or hotspot_detection()'; exit 0
}

# --- class (b): wrap with `codescope exec` -------------------------

if ($cmd -match '^cargo\s+(build|test|check|clippy|nextest|run|b|t|c|r)(\s|$)') {
    Suggest-Exec $cmd 'drop Compiling-line noise, keep errors + summary'; exit 0
}
if ($cmd -match '^(pytest|py\.test)(\s|$)') {
    Suggest-Exec $cmd 'collapse dot-progress lines to a count'; exit 0
}
if ($cmd -match '^(npm|pnpm|yarn)\s+(install|i|ci|add|update)(\s|$)') {
    Suggest-Exec $cmd 'drop deprecation + funding chatter'; exit 0
}
if ($cmd -match '^(npx\s+)?tsc(\s|$)') {
    Suggest-Exec $cmd 'dedupe identical diagnostics across files'; exit 0
}
if ($cmd -match '^docker\s+(build|buildx)(\s|$)') {
    Suggest-Exec $cmd 'drop layer-progress noise, keep steps'; exit 0
}
if ($cmd -match '^git\s+log(\s|$)' -and
    $cmd -notmatch '(--oneline|--pretty|--format|-p\s|--patch)') {
    Suggest-Exec $cmd 'force --oneline -n 20 unless you pass --pretty'; exit 0
}
if ($cmd -match '^git\s+diff(\s|$)' -and
    $cmd -notmatch '(--stat|--shortstat|--numstat|--name-only|--name-status)') {
    Suggest-Exec $cmd 'force --stat summary; --full to see the diff'; exit 0
}

exit 0

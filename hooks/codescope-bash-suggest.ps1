# RTK-03 — PowerShell mirror of `codescope-bash-suggest.sh`.
#
# Claude Code on Windows pipes the same JSON envelope on stdin.
# This script extracts the `command` field and prints a
# one-liner to stderr nudging the model toward the codescope
# MCP tool that answers the same question with 80–95% fewer
# tokens. Informational only by default — set
# `$env:CODESCOPE_HOOK_BLOCK = '1'` to make matched patterns
# veto the bash run.
#
# Install: drop on PATH (or set full path), and add an entry to
# `~/.claude/settings.json`:
#
#   {
#     "matcher": "Bash",
#     "hooks": [
#       { "type": "command",
#         "command": "pwsh -NoProfile -File C:\\path\\to\\codescope-bash-suggest.ps1" }
#     ]
#   }

$ErrorActionPreference = 'Stop'

$payload = [Console]::In.ReadToEnd()
# The JSON payload is small — use ConvertFrom-Json instead of
# regex. PowerShell 5.1 and 7 both ship this.
try {
    $parsed = $payload | ConvertFrom-Json
    $cmd = $parsed.tool_input.command
} catch {
    exit 0  # can't parse → don't get in the way
}
if (-not $cmd) { exit 0 }

function Suggest($label, $tool) {
    [Console]::Error.WriteLine("⟿ codescope: $label → prefer ``$tool`` for 80–95% less context")
    [Console]::Error.WriteLine("  (set CODESCOPE_HOOK_BLOCK=1 to make this hard-fail)")
    if ($env:CODESCOPE_HOOK_BLOCK -eq '1') {
        exit 2  # Claude Code treats non-zero exit as hook veto
    }
}

$srcRe = '\.(rs|ts|tsx|js|jsx|py|go|java|cpp|c|h|hpp|kt|swift|rb|php|sol|vue|svelte)'

# cat / less / more / bat on a source file
if ($cmd -match "^(cat|less|more|bat)\s+[^|]+$srcRe\s*$") {
    Suggest 'reading source file' 'context_bundle(file_path)'
    exit 0
}

# head / tail on a source file
if ($cmd -match "^(head|tail)\s+.*$srcRe") {
    Suggest 'peeking at source file' 'context_bundle(file_path)'
    exit 0
}

# grep / rg / ag with a path-like target (recursive or tree)
if ($cmd -match '^(grep|rg|ag)\s+' -and
    $cmd -match '(-r|--recursive|\s[^\s\.]+/[^\s]+|\s\.\s*$)') {
    Suggest 'grep across codebase' 'search(query, mode=fuzzy) or find_function(name)'
    exit 0
}

# find -name
if ($cmd -match '^find\s+.*-name') {
    Suggest 'find by filename' 'search(query, mode=file)'
    exit 0
}

# git log / git blame
if ($cmd -match '^git\s+(log|blame)(\s|$)') {
    Suggest 'git history on a file' 'file_churn(path) or hotspot_detection()'
    exit 0
}

exit 0

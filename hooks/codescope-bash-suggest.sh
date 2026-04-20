#!/usr/bin/env bash
# RTK-03 — Claude Code PreToolUse hook.
#
# Claude Code pipes the bash command into the hook on stdin
# wrapped in a small JSON envelope. We extract the command,
# match it against patterns that waste tokens, and print a
# short suggestion on stderr so the model sees a nudge toward
# the codescope MCP tool that would answer the same question
# with 80–95% less context.
#
# This hook is *informational only* — it always exits 0 so the
# bash command still runs. Set `CODESCOPE_HOOK_BLOCK=1` to make
# matched patterns exit non-zero and force the model to pick the
# codescope tool.
#
# Install: put this file on PATH (chmod +x) and add an entry in
# `~/.claude/settings.json` under `hooks.PreToolUse`:
#
#     {
#       "matcher": "Bash",
#       "hooks": [
#         { "type": "command", "command": "codescope-bash-suggest" }
#       ]
#     }
#
# Windows users: mirror is `codescope-bash-suggest.ps1`.

set -uo pipefail

# The stdin payload is JSON: `{ "tool_name": "Bash", "tool_input":
# { "command": "..." } }`. We don't pull `jq` just for this — a
# narrow sed pattern is enough to extract the command field.
PAYLOAD="$(cat)"
CMD="$(printf '%s' "$PAYLOAD" \
  | tr -d '\r' \
  | sed -nE 's/.*"command"[[:space:]]*:[[:space:]]*"((\\.|[^"\\])*)".*/\1/p' \
  | head -1)"

# Unescape common sequences (\" \\ \n). Good enough for suggestion
# matching — we're not round-tripping this back into the executor.
CMD="${CMD//\\\"/\"}"
CMD="${CMD//\\\\/\\}"

suggest() {
  # $1 = pattern label, $2 = recommended codescope tool.
  >&2 echo "⟿ codescope: $1 → prefer \`$2\` for 80–95% less context"
  >&2 echo "  (set CODESCOPE_HOOK_BLOCK=1 to make this hard-fail)"
  if [[ "${CODESCOPE_HOOK_BLOCK:-0}" == "1" ]]; then
    exit 2   # Claude Code treats non-zero exit as hook veto
  fi
}

# --- pattern table ---------------------------------------------
# Order matters: most specific first. Each branch prints once and
# stops (so a single `grep -r | head` doesn't fire three hints).

# cat/less/more on a code file → context_bundle
if printf '%s' "$CMD" | grep -Eq '^(cat|less|more|bat)[[:space:]]+[^|]+\.(rs|ts|tsx|js|jsx|py|go|java|cpp|c|h|hpp|kt|swift|rb|php|sol|vue|svelte)[[:space:]]*$'; then
  suggest "reading source file" "context_bundle(file_path)"
  exit 0
fi

# head/tail of a source file → same
if printf '%s' "$CMD" | grep -Eq '^(head|tail)[[:space:]]+.*\.(rs|ts|tsx|js|jsx|py|go|java|cpp|c|h|hpp|kt|swift|rb|php|vue|svelte)'; then
  suggest "peeking at source file" "context_bundle(file_path)"
  exit 0
fi

# grep/rg/ag searching for a symbol in a codebase → search
if printf '%s' "$CMD" | grep -Eq '^(grep|rg|ag)[[:space:]]+(-[a-zA-Z]+[[:space:]]+)*["'"'"']?[A-Za-z_][A-Za-z0-9_]*'; then
  # Avoid hitting on log greps ("grep ERROR app.log") — only
  # trigger when the target looks like a path tree, not a file.
  if printf '%s' "$CMD" | grep -Eq '(-r|--recursive|[[:space:]][^.[:space:]]+/[^[:space:]]+|[[:space:]]\.[[:space:]]*$)'; then
    suggest "grep across codebase" "search(query, mode=fuzzy) or find_function(name)"
    exit 0
  fi
fi

# find -name "*.ext" → search (mode=file)
if printf '%s' "$CMD" | grep -Eq '^find[[:space:]]+.*-name'; then
  suggest "find by filename" "search(query, mode=file)"
  exit 0
fi

# git log / git blame on a file → file_churn / hotspot_detection
if printf '%s' "$CMD" | grep -Eq '^git[[:space:]]+(log|blame)([[:space:]]|$)'; then
  suggest "git history on a file" "file_churn(path) or hotspot_detection()"
  exit 0
fi

exit 0

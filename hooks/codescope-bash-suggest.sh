#!/usr/bin/env bash
# RTK-03 — Claude Code PreToolUse hook.
#
# Claude Code pipes the bash command into the hook on stdin
# wrapped in a small JSON envelope. We extract the command,
# classify it as either:
#
#   (a) a task better served by a codescope MCP tool (e.g. cat
#       foo.rs → context_bundle) — suggest the tool, OR
#   (b) a command we can compress with `codescope exec` (cargo,
#       pytest, git log, grep …) — suggest prefixing it.
#
# Class (a) wins when the command is essentially "read/search
# code"; class (b) wins when it's a dev command whose output is
# noise-heavy but still needed literally.
#
# This hook is *informational only* — it always exits 0 so the
# bash command still runs. Set CODESCOPE_HOOK_BLOCK=1 to force
# matched patterns to hard-fail (exit 2), letting you bind the
# model to the suggested tool.
#
# Install via `codescope hook --agent claude-code`.

set -uo pipefail

PAYLOAD="$(cat)"
CMD="$(printf '%s' "$PAYLOAD" \
  | tr -d '\r' \
  | sed -nE 's/.*"command"[[:space:]]*:[[:space:]]*"((\\.|[^"\\])*)".*/\1/p' \
  | head -1)"
CMD="${CMD//\\\"/\"}"
CMD="${CMD//\\\\/\\}"

# If already wrapped with codescope exec, nothing to suggest.
if printf '%s' "$CMD" | grep -Eq '^[[:space:]]*codescope[[:space:]]+exec[[:space:]]'; then
  exit 0
fi

suggest_tool() {
  # $1 = pattern label, $2 = recommended codescope tool.
  >&2 echo "⟿ codescope: $1 → prefer \`$2\` for 80–95% less context"
  >&2 echo "  (set CODESCOPE_HOOK_BLOCK=1 to make this hard-fail)"
  if [[ "${CODESCOPE_HOOK_BLOCK:-0}" == "1" ]]; then
    exit 2
  fi
}

suggest_exec() {
  # $1 = inner command, $2 = typical savings blurb.
  >&2 echo "⟿ codescope: prefix with \`codescope exec\` → $2"
  >&2 echo "  e.g.  codescope exec $1"
  if [[ "${CODESCOPE_HOOK_BLOCK:-0}" == "1" ]]; then
    exit 2
  fi
}

# --- class (a): use an MCP tool instead -----------------------------

# cat/less/more/bat on a source file → context_bundle
if printf '%s' "$CMD" | grep -Eq '^(cat|less|more|bat)[[:space:]]+[^|]+\.(rs|ts|tsx|js|jsx|py|go|java|cpp|c|h|hpp|kt|swift|rb|php|sol|vue|svelte)[[:space:]]*$'; then
  suggest_tool "reading source file" "context_bundle(file_path)"
  exit 0
fi

# head/tail of a source file → same
if printf '%s' "$CMD" | grep -Eq '^(head|tail)[[:space:]]+.*\.(rs|ts|tsx|js|jsx|py|go|java|cpp|c|h|hpp|kt|swift|rb|php|vue|svelte)'; then
  suggest_tool "peeking at source file" "context_bundle(file_path)"
  exit 0
fi

# grep/rg/ag searching for a symbol in a codebase → search
if printf '%s' "$CMD" | grep -Eq '^(grep|rg|ag)[[:space:]]+(-[a-zA-Z]+[[:space:]]+)*["'"'"']?[A-Za-z_][A-Za-z0-9_]*'; then
  if printf '%s' "$CMD" | grep -Eq '(-r|--recursive|[[:space:]][^.[:space:]]+/[^[:space:]]+|[[:space:]]\.[[:space:]]*$)'; then
    suggest_tool "grep across codebase" "search(query, mode=fuzzy) or find_function(name)"
    exit 0
  fi
fi

# find -name "*.ext" → search (mode=file)
if printf '%s' "$CMD" | grep -Eq '^find[[:space:]]+.*-name'; then
  suggest_tool "find by filename" "search(query, mode=file)"
  exit 0
fi

# git blame on a file → hotspot_detection / file_churn
if printf '%s' "$CMD" | grep -Eq '^git[[:space:]]+blame([[:space:]]|$)'; then
  suggest_tool "git blame" "file_churn(path) or hotspot_detection()"
  exit 0
fi

# --- class (b): wrap with `codescope exec` -------------------------

# cargo build/test/check/clippy/run — biggest win (~80% reduction on
# build, ~90% on test with many passing).
if printf '%s' "$CMD" | grep -Eq '^cargo[[:space:]]+(build|test|check|clippy|nextest|run|b|t|c|r)([[:space:]]|$)'; then
  suggest_exec "$CMD" "drop Compiling-line noise, keep errors + summary"
  exit 0
fi

# pytest — dots/F collapse to count, failures kept verbatim.
if printf '%s' "$CMD" | grep -Eq '^(pytest|py\.test)([[:space:]]|$)'; then
  suggest_exec "$CMD" "collapse dot-progress lines to a count"
  exit 0
fi

# npm/pnpm/yarn install — drops deprecation + funding chatter.
if printf '%s' "$CMD" | grep -Eq '^(npm|pnpm|yarn)[[:space:]]+(install|i|ci|add|update)([[:space:]]|$)'; then
  suggest_exec "$CMD" "drop deprecation + funding chatter"
  exit 0
fi

# tsc — dedupes identical diagnostics across files.
if printf '%s' "$CMD" | grep -Eq '^(npx[[:space:]]+)?tsc([[:space:]]|$)'; then
  suggest_exec "$CMD" "dedupe identical diagnostics across files"
  exit 0
fi

# docker build — drops layer-progress noise.
if printf '%s' "$CMD" | grep -Eq '^docker[[:space:]]+(build|buildx)([[:space:]]|$)'; then
  suggest_exec "$CMD" "drop layer-progress noise, keep steps"
  exit 0
fi

# git log without --oneline → codescope exec (forces --oneline -n 20).
if printf '%s' "$CMD" | grep -Eq '^git[[:space:]]+log([[:space:]]|$)' \
  && ! printf '%s' "$CMD" | grep -Eq '(--oneline|--pretty|--format|-p[[:space:]]|--patch)'; then
  suggest_exec "$CMD" "force --oneline -n 20 unless you pass --pretty"
  exit 0
fi

# git diff without --stat → codescope exec (forces --stat).
if printf '%s' "$CMD" | grep -Eq '^git[[:space:]]+diff([[:space:]]|$)' \
  && ! printf '%s' "$CMD" | grep -Eq '(--stat|--shortstat|--numstat|--name-only|--name-status)'; then
  suggest_exec "$CMD" "force --stat summary; --full to see the diff"
  exit 0
fi

exit 0

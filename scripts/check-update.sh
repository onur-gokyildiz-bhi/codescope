#!/bin/bash
# Codescope update checker — run as a session start hook
# Checks GitHub for newer version, prints a notice if available
# Install: add to .claude/settings.json hooks.SessionStart

REPO="onur-gokyildiz-bhi/codescope"
CACHE_DIR="${HOME}/.codescope"
CACHE_FILE="${CACHE_DIR}/update-check"
CHECK_INTERVAL=86400  # 24 hours

# Skip if checked recently
if [ -f "$CACHE_FILE" ]; then
    last_check=$(cat "$CACHE_FILE" 2>/dev/null | head -1)
    now=$(date +%s)
    if [ -n "$last_check" ] && [ $((now - last_check)) -lt $CHECK_INTERVAL ]; then
        # Show cached result if update was available
        cached_msg=$(sed -n '2p' "$CACHE_FILE" 2>/dev/null)
        [ -n "$cached_msg" ] && echo "$cached_msg"
        exit 0
    fi
fi

# Get current version
current=$(codescope --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
if [ -z "$current" ]; then
    exit 0  # codescope not installed, skip silently
fi

# Get latest version from GitHub (with timeout)
latest=$(curl -fsSL --connect-timeout 3 --max-time 5 \
    "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
    | grep -o '"tag_name": *"[^"]*"' | head -1 | sed 's/.*"v\?\([^"]*\)"/\1/')

if [ -z "$latest" ]; then
    exit 0  # Network error, skip silently
fi

# Save check timestamp
mkdir -p "$CACHE_DIR"

if [ "$current" != "$latest" ]; then
    msg="[codescope] Update available: v${current} → v${latest}. Run /cs-update to upgrade."
    echo "$(date +%s)" > "$CACHE_FILE"
    echo "$msg" >> "$CACHE_FILE"
    echo "$msg"
else
    echo "$(date +%s)" > "$CACHE_FILE"
    # No message = up to date
fi

#!/bin/bash
# Codescope — AI Agent Integration Wizard
# Detects your CLI agent and installs codescope skills + MCP config.
#
# Usage: curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/setup-claude.sh | bash

set -euo pipefail

REPO_RAW="https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main"

echo ""
echo "  ╔══════════════════════════════════════════╗"
echo "  ║   Codescope — AI Agent Setup Wizard      ║"
echo "  ╚══════════════════════════════════════════╝"
echo ""

# ─── Step 1: Detect or ask which CLI ─────────────────────────────

detect_agents() {
    AGENTS=()
    command -v claude &>/dev/null && AGENTS+=("claude-code")
    command -v codex &>/dev/null && AGENTS+=("codex-cli")
    command -v opencode &>/dev/null && AGENTS+=("opencode")
    command -v cursor &>/dev/null && AGENTS+=("cursor")
    command -v zed &>/dev/null && AGENTS+=("zed")
    command -v gemini &>/dev/null && AGENTS+=("gemini-cli")
}

detect_agents

if [ ${#AGENTS[@]} -eq 0 ]; then
    echo "  No AI agent CLI detected on PATH."
    echo ""
    echo "  Which agent do you use?"
    echo "    1) Claude Code"
    echo "    2) Codex CLI"
    echo "    3) OpenCode"
    echo "    4) Cursor"
    echo "    5) Zed"
    echo "    6) Gemini CLI"
    echo "    7) All of the above"
    echo ""
    if [ -t 0 ]; then
        read -p "  Select [1-7]: " -n 1 -r choice
        echo ""
    else
        echo "  (non-interactive mode — installing for all agents)"
        choice="7"
    fi
    case "$choice" in
        1) AGENTS=("claude-code") ;;
        2) AGENTS=("codex-cli") ;;
        3) AGENTS=("opencode") ;;
        4) AGENTS=("cursor") ;;
        5) AGENTS=("zed") ;;
        6) AGENTS=("gemini-cli") ;;
        *) AGENTS=("claude-code" "codex-cli" "opencode" "cursor" "zed" "gemini-cli") ;;
    esac
elif [ ${#AGENTS[@]} -eq 1 ]; then
    echo "  Detected: ${AGENTS[0]}"
else
    echo "  Detected: ${AGENTS[*]}"
fi
echo ""

# ─── Step 2: Install codescope binary if missing ─────────────────

echo "  [1/5] Checking codescope binary..."
if command -v codescope &>/dev/null; then
    VERSION=$(codescope --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
    echo "         ✓ codescope $VERSION"
else
    echo "         Installing codescope..."
    curl -fsSL "$REPO_RAW/install.sh" | bash
    echo ""
fi

# ─── Step 3: Install skills per agent ────────────────────────────

echo "  [2/5] Installing skills..."

SKILLS=("codescope" "cs-search" "cs-index" "cs-stats" "cs-ask" "cs-impact" "cs-callers" "cs-file" "cs-query" "cs-update")
REFS_CODESCOPE="skills/codescope/references/TOOLS.md"
REFS_QUERY="skills/cs-query/references/SURREALQL.md"

install_skills_to() {
    local dir="$1"
    local label="$2"
    mkdir -p "$dir"
    for skill in "${SKILLS[@]}"; do
        mkdir -p "$dir/$skill"
        curl -fsSL "$REPO_RAW/skills/$skill/SKILL.md" -o "$dir/$skill/SKILL.md" 2>/dev/null
    done
    # References
    mkdir -p "$dir/codescope/references" "$dir/cs-query/references"
    curl -fsSL "$REPO_RAW/$REFS_CODESCOPE" -o "$dir/codescope/references/TOOLS.md" 2>/dev/null
    curl -fsSL "$REPO_RAW/$REFS_QUERY" -o "$dir/cs-query/references/SURREALQL.md" 2>/dev/null
    echo "         ✓ ${#SKILLS[@]} skills + 2 references → $label"
}

for agent in "${AGENTS[@]}"; do
    case "$agent" in
        claude-code)
            install_skills_to "$HOME/.claude/skills" "Claude Code (~/.claude/skills/)"
            ;;
        codex-cli)
            CODEX_DIR="${CODEX_SKILLS_DIR:-$HOME/.codex/skills}"
            install_skills_to "$CODEX_DIR/codescope" "Codex CLI ($CODEX_DIR/codescope/)"
            ;;
        opencode)
            install_skills_to "$HOME/.opencode/skills/codescope" "OpenCode (~/.opencode/skills/codescope/)"
            ;;
        cursor|zed|gemini-cli)
            # These agents read .mcp.json from project root — skills go to Claude's dir as fallback
            install_skills_to "$HOME/.claude/skills" "$agent (via ~/.claude/skills/ fallback)"
            ;;
    esac
done

# ─── Step 4: Configure MCP server ────────────────────────────────

echo "  [3/5] Configuring MCP server..."

configure_mcp_json() {
    local config_file="$1"
    local label="$2"
    if [ -f "$config_file" ]; then
        if grep -q "codescope" "$config_file" 2>/dev/null; then
            echo "         ✓ $label — already configured"
            return
        fi
    fi
    if command -v jq &>/dev/null && [ -f "$config_file" ]; then
        TMP=$(mktemp)
        jq '.mcpServers.codescope = {"command": "codescope", "args": ["mcp", ".", "--auto-index"]}' "$config_file" > "$TMP"
        mv "$TMP" "$config_file"
        echo "         ✓ $label — added codescope entry"
    else
        # Create or overwrite
        mkdir -p "$(dirname "$config_file")"
        cat > "$config_file" << 'MCPEOF'
{
  "mcpServers": {
    "codescope": {
      "command": "codescope",
      "args": ["mcp", ".", "--auto-index"]
    }
  }
}
MCPEOF
        echo "         ✓ $label — created"
    fi
}

# Check for marketplace install — avoid double MCP registration
MARKETPLACE_DETECTED=false
SETTINGS_FILE="$HOME/.claude/settings.json"
if [ -f "$SETTINGS_FILE" ]; then
    if grep -q "extraKnownMarketplaces.*codescope\|codescope.*extraKnownMarketplaces" "$SETTINGS_FILE" 2>/dev/null || \
       grep -q '"codescope"' "$SETTINGS_FILE" 2>/dev/null && grep -q 'extraKnownMarketplaces' "$SETTINGS_FILE" 2>/dev/null; then
        MARKETPLACE_DETECTED=true
        echo "         ! Codescope marketplace plugin detected in settings.json"
        echo "         ! Skipping global MCP config to avoid double registration."
        echo "         ! MCP will be configured per-project via .mcp.json instead."
        echo ""
    fi
fi

for agent in "${AGENTS[@]}"; do
    case "$agent" in
        claude-code)
            if [ "$MARKETPLACE_DETECTED" = true ]; then
                echo "         ✓ Claude Code: using marketplace plugin (no global MCP needed)"
                echo "         → Run 'codescope init' in each project for .mcp.json"
            else
                configure_mcp_json "$HOME/.claude.json" "~/.claude.json (Claude Code global)"
            fi
            ;;
        codex-cli)
            configure_mcp_json "$HOME/.codex.json" "~/.codex.json (Codex CLI global)"
            ;;
        cursor)
            echo "         → Cursor: add codescope to .cursor/mcp.json in your project"
            ;;
        zed)
            echo "         → Zed: add codescope to .zed/mcp.json in your project"
            ;;
        *)
            # OpenCode, Gemini CLI — use project-level .mcp.json (codescope init handles this)
            echo "         → $agent: run 'codescope init' in each project"
            ;;
    esac
done

# ─── Step 5: Install rules (Claude Code specific) ────────────────

echo "  [4/5] Installing mandatory rules..."

for agent in "${AGENTS[@]}"; do
    case "$agent" in
        claude-code)
            RULES_DIR="$HOME/.claude/rules"
            mkdir -p "$RULES_DIR"
            curl -fsSL "$REPO_RAW/.claude/rules/codescope-mandatory.md" -o "$RULES_DIR/codescope-mandatory.md" 2>/dev/null
            echo "         ✓ ~/.claude/rules/codescope-mandatory.md (alwaysApply)"
            ;;
        *)
            # Other agents don't have a rules system yet
            ;;
    esac
done

# ─── Step 6: Verify ──────────────────────────────────────────────

echo "  [5/5] Verifying..."

PASS=0
FAIL=0

if command -v codescope &>/dev/null; then
    echo "         ✓ codescope binary on PATH"
    PASS=$((PASS + 1))
else
    echo "         ✗ codescope binary NOT on PATH"
    FAIL=$((FAIL + 1))
fi

if command -v codescope-mcp &>/dev/null; then
    echo "         ✓ codescope-mcp binary on PATH"
    PASS=$((PASS + 1))
else
    echo "         ✗ codescope-mcp binary NOT on PATH"
    FAIL=$((FAIL + 1))
fi

SKILL_COUNT=$(find "$HOME/.claude/skills" -name "SKILL.md" -path "*/cs-*" -o -name "SKILL.md" -path "*/codescope/*" 2>/dev/null | wc -l)
if [ "$SKILL_COUNT" -ge 10 ]; then
    echo "         ✓ $SKILL_COUNT skills installed"
    PASS=$((PASS + 1))
else
    echo "         ✗ Only $SKILL_COUNT skills found (expected 10+)"
    FAIL=$((FAIL + 1))
fi

echo ""
if [ $FAIL -eq 0 ]; then
    echo "  ╔══════════════════════════════════════════╗"
    echo "  ║   ✓ Setup complete! All checks passed.   ║"
    echo "  ╚══════════════════════════════════════════╝"
else
    echo "  ⚠ Setup done with $FAIL issue(s). Run 'codescope doctor .' for details."
fi

echo ""
echo "  Quick start:"
echo "    cd /path/to/project"
echo "    codescope init            # index + create .mcp.json"
echo "    codescope doctor .        # verify everything works"
echo ""
echo "  Available skills:"
echo "    /codescope                — Main menu"
echo "    /cs-search <pattern>      — Search functions"
echo "    /cs-impact <function>     — Impact analysis"
echo "    /cs-callers <function>    — Who calls this?"
echo "    /cs-ask <question>        — Natural language (TR/EN)"
echo "    /cs-stats                 — Codebase overview"
echo "    /cs-query <surql>         — Raw SurrealQL"
echo "    /cs-update                — Self-update"
echo ""
echo "  Agents configured: ${AGENTS[*]}"
echo ""

#!/bin/bash
# Codescope — Claude Code Integration Setup
# Installs: MCP server config, skills, hooks, CLAUDE.md
#
# Usage: curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/setup-claude.sh | bash

set -euo pipefail

REPO_RAW="https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main"
CLAUDE_DIR="$HOME/.claude"
SKILLS_DIR="$CLAUDE_DIR/skills"

echo ""
echo "  Codescope — Claude Code Integration Setup"
echo "  =========================================="
echo ""

# 1. Check codescope is installed
if ! command -v codescope &> /dev/null; then
    echo "  codescope not found. Installing..."
    curl -fsSL "$REPO_RAW/install.sh" | bash
    echo ""
fi

# 2. Configure MCP server in ~/.claude.json
echo "  [1/4] Configuring MCP server..."
CLAUDE_JSON="$HOME/.claude.json"

if [ -f "$CLAUDE_JSON" ]; then
    # Check if codescope already configured
    if grep -q "codescope" "$CLAUDE_JSON" 2>/dev/null; then
        echo "         Already configured in $CLAUDE_JSON"
    else
        # Merge into existing config using python/jq
        if command -v jq &> /dev/null; then
            TMP=$(mktemp)
            jq '.mcpServers.codescope = {"command": "codescope", "args": ["mcp", ".", "--auto-index"]}' "$CLAUDE_JSON" > "$TMP"
            mv "$TMP" "$CLAUDE_JSON"
            echo "         Added codescope to existing $CLAUDE_JSON"
        else
            echo "         WARNING: jq not found, cannot merge. Add manually:"
            echo '         "codescope": {"command": "codescope", "args": ["mcp", ".", "--auto-index"]}'
        fi
    fi
else
    cat > "$CLAUDE_JSON" << 'EOF'
{
  "mcpServers": {
    "codescope": {
      "command": "codescope",
      "args": ["mcp", ".", "--auto-index"]
    }
  }
}
EOF
    echo "         Created $CLAUDE_JSON"
fi

# 3. Install skills
echo "  [2/4] Installing skills..."
SKILLS=("codescope" "cs-search" "cs-index" "cs-stats" "cs-ask" "cs-impact" "cs-callers" "cs-file" "cs-query" "cs-update")

for skill in "${SKILLS[@]}"; do
    mkdir -p "$SKILLS_DIR/$skill"
    curl -fsSL "$REPO_RAW/templates/skills/$skill/SKILL.md" -o "$SKILLS_DIR/$skill/SKILL.md"
done
echo "         Installed ${#SKILLS[@]} skills to $SKILLS_DIR/"

# 4. Show hook template
echo "  [3/4] Hooks template..."
echo "         To add auto-index on session start, add to your project's"
echo "         .claude/settings.json:"
echo ""
echo '         {
           "hooks": {
             "SessionStart": [{
               "hooks": [{
                 "type": "command",
                 "command": "echo Codescope ready",
                 "timeout": 5
               }]
             }]
           }
         }'
echo ""

# 5. CLAUDE.md template
echo "  [4/4] CLAUDE.md template..."
curl -fsSL "$REPO_RAW/templates/CLAUDE.md" -o "/tmp/codescope-CLAUDE.md"
echo "         Template saved to /tmp/codescope-CLAUDE.md"
echo "         Copy to your project: cp /tmp/codescope-CLAUDE.md ./CLAUDE.md"

echo ""
echo "  Setup complete!"
echo "  ==============="
echo ""
echo "  Available commands in Claude Code:"
echo "    /codescope          — Main menu & routing"
echo "    /cs-search <name>   — Search functions"
echo "    /cs-index           — Re-index project"
echo "    /cs-stats           — Codebase overview"
echo "    /cs-ask <question>  — Ask in Turkish or English"
echo "    /cs-impact <func>   — Impact analysis"
echo "    /cs-callers <func>  — Who calls this function?"
echo "    /cs-file <path>     — All entities in a file"
echo "    /cs-query <surql>   — Raw SurrealQL query"
echo "    /cs-update          — Check & install updates"
echo ""
echo "  Auto update check (optional):"
echo '    Add to .claude/settings.json:'
echo '    {"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"bash ~/.codescope/check-update.sh","timeout":10}]}]}}'
echo ""
echo "  Start Claude Code in any project:"
echo "    cd /path/to/project && claude"
echo ""

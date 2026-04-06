# Agent Configuration Templates

Ready-to-use configuration files for connecting Codescope to various AI agents.

## Supported Agents

| Agent | Config File | Where to Place |
|-------|------------|----------------|
| **Claude Code** | `claude-code.json` | `~/.claude.json` or `.mcp.json` in project root |
| **Cursor** | `cursor.json` | `.cursor/mcp.json` in project root |
| **Zed** | `zed.json` | `~/.config/zed/settings.json` (merge into `context_servers`) |
| **Codex CLI** | `codex-cli.yaml` | `~/.codex/config.yaml` |
| **Gemini CLI** | `gemini-cli.json` | `~/.gemini/settings.json` |

## Quick Setup (All Agents)

### Claude Code
```bash
cp configs/claude-code.json ~/.claude.json
# Or for project-level:
cp configs/claude-code.json .mcp.json
```

### Cursor
```bash
mkdir -p .cursor
cp configs/cursor.json .cursor/mcp.json
```

### Zed
Merge `configs/zed.json` into `~/.config/zed/settings.json`.

### Codex CLI
```bash
mkdir -p ~/.codex
cp configs/codex-cli.yaml ~/.codex/config.yaml
```

## Custom Binary Path

If `codescope` is not in PATH, replace `"codescope"` with the full path:

```json
"command": "/path/to/codescope"
```

Windows:
```json
"command": "C:\\Users\\you\\AppData\\Local\\codescope\\bin\\codescope.exe"
```

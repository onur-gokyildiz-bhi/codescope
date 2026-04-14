---
name: demo
description: Set up a demo session — pick a sample repo, index it, open web UI, prep the narrative.
---

# /demo

Prepare codescope for a live demo or screen recording. The 30-second pitch in terminal + browser.

## When to invoke

- Launch recording (HN thumbnail video)
- Conference talk prep
- Sales/support call with a potential user

## Pick a sample repo

The best demo repos are medium-size, well-known, and have interesting call graphs.

| Repo | Size | Language | Why it demos well |
|---|---|---|---|
| tokio | 769 files | Rust | Deep async call chains, `Runtime::spawn` blast radius |
| FastAPI | 2,713 files | Python | Route handler cross-refs, dependency injection graph |
| axum | 296 files | Rust | Middleware layering visible, smaller than tokio |
| Gin | 108 files | Go | Small enough to show whole graph zoomed out |
| ripgrep | 101 files | Rust | Famous, familiar, compiles fast for live index demo |

Default for first demo: **axum** (296 files, meaningful graph, indexes in ~11s).

## Demo script (recording version)

```bash
# Scene 1: fresh install (show this once, can be trimmed)
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash

# Scene 2: index a real repo
git clone https://github.com/tokio-rs/axum /tmp/demo-axum
cd /tmp/demo-axum
codescope init --daemon  # spins up MCP + Web UI on port 9877

# Scene 3: open the web UI
# Show the 3D graph. Click a central node (e.g. `Router`), watch neighborhood expand.

# Scene 4: Claude Code
# Open Claude Code in /tmp/demo-axum. Ask:
#   "Who calls Router::route transitively within 3 hops?"
# Show the response — instant, exact, small token count.

# Scene 5: knowledge save
# Claude: "This codebase uses a layered middleware pattern."
# codescope: knowledge saved to graph.
# Next session: agent already knows.
```

## Narrative beats

1. "You ask, it reads 40 files. Watch." (show the pain)
2. "With codescope: one query, one answer, sub-millisecond." (show the fix)
3. "And it remembers." (show cross-session memory)
4. "Works with whatever editor you already use." (LSP demo in VS Code)

## Pre-recording checklist

- [ ] Binary is latest version (`codescope --version`)
- [ ] Terminal font ≥ 16pt for readability
- [ ] Web UI: dark mode, hide browser chrome (F11 fullscreen)
- [ ] No personal/sensitive data in any open window
- [ ] MCP daemon running, reconnect tested
- [ ] One narrative per scene, no tangents

## Codescope-first rule

During demo:
- Use `search(mode="fuzzy", query=...)` / `find_callers(...)` / `impact_analysis(...)` visibly
- The tool names should appear on screen — viewers should be able to tell this is graph, not RAG

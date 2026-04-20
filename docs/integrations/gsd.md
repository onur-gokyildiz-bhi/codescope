# Using codescope with GSD (Get Shit Done)

> **TL;DR** — [GSD](https://github.com/gsd-build/get-shit-done) is a
> spec-driven workflow layer for Claude Code (and 13 other AI
> runtimes). codescope is a code-intelligence layer. Install both.
> GSD subagents automatically use codescope MCP tools — no extra
> wiring needed.

## What GSD is

GSD turns a loose idea into shipped code through a structured
pipeline:

```
/gsd-new-project      → PROJECT.md + REQUIREMENTS.md + ROADMAP.md
/gsd-discuss-phase N  → lock your decisions for phase N
/gsd-plan-phase N     → 2-3 atomic plans with XML structure
/gsd-execute-phase N  → wave execution: parallel where possible,
                        fresh context per plan, atomic commits
/gsd-verify-work N    → UAT: user confirms each deliverable works
/gsd-ship N           → PR from verified work
```

Its 30+ specialised subagents (`gsd-planner`, `gsd-executor`,
`gsd-code-reviewer`, `gsd-debugger`, `gsd-pattern-mapper`,
`gsd-codebase-mapper`, …) run Claude Code subprocesses to do the
real work.

## Why they pair

GSD manages the **workflow**. codescope provides the **code
context** GSD's subagents need to do their work well.

| Layer | Tool | Answers |
|-------|------|---------|
| Workflow / planning | **GSD** | "how do I systematically build this feature?" |
| Code semantics | **codescope** | "what does this code do, who calls it, what breaks if I change it?" |
| Generic tool output | [context-mode](https://github.com/mksglu/context-mode) | "how do I not dump 50 KB of tool output into the context?" |
| Shell output | [RTK](https://github.com/rtk-ai/rtk) | "why is `git status` eating 3 KB per call?" |

Without codescope, `gsd-map-codebase` and `gsd-pattern-mapper`
fall back to Read / Grep on your source tree — exactly the
behaviour that wastes tokens at scale. With codescope installed,
those subagents see `context_bundle`, `search_functions`,
`find_callers`, `impact_analysis`, `code_health` on the MCP
surface and prefer them automatically.

## Install

Run both installers. Order doesn't matter.

```bash
# codescope (one-liner installer — see README for platforms)
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash
# or Homebrew:
brew install onur-gokyildiz-bhi/codescope/codescope
codescope start                # bring up the bundled surreal server
cd your-project
codescope init --agent claude-code

# GSD
npx get-shit-done-cc@latest --claude --local
```

Codescope's `codescope init` adds the MCP config. GSD installs
its slash commands and subagents into `.claude/commands/gsd/`.
When you then run `/gsd-new-project`, the spawned Claude Code
subprocesses have both surfaces available.

## Which GSD command benefits from which codescope tool

| GSD command / agent | Codescope tool it naturally calls |
|---------------------|-----------------------------------|
| `/gsd-map-codebase` · `gsd-codebase-mapper` | `context_bundle(file)` · `graph_stats()` · `search(query, mode=fuzzy)` |
| `/gsd-plan-phase` · `gsd-phase-researcher` | `find_callers(name)` · `impact_analysis(name, depth=3)` · `knowledge_search(topic)` |
| `gsd-code-reviewer` | `edit_preflight` · `type_hierarchy(name)` · `refactor(mode=find_unused)` |
| `gsd-debugger` · `/gsd-debug` | `find_callers` · `impact_analysis` · `file_churn(path)` · `hotspot_detection()` |
| `gsd-pattern-mapper` | `search(mode=neighborhood)` · `explore(entity)` · `community_detection()` |
| `gsd-security-auditor` | `http_analysis(mode=calls)` · `knowledge_search(kind=problem)` |

None of these hook points exist in code. The model picks them
up from the `ServerInfo.instructions` codescope injects at MCP
initialize (see [`codescope doctor`](../../README.md) → routing
rules).

## State overlap: `STATE.md` ↔ codescope `knowledge`

GSD tracks current-milestone progress in `STATE.md`. Codescope's
`knowledge_save(status:done, shipped:YYYY-MM-DD, vX.Y.Z)` tracks
the same thing in a queryable graph. Keep both:

- `STATE.md` is for humans + GSD agents walking the plan.
- `knowledge` is for cross-session + cross-project recall
  (`knowledge_search("surreal migration")` finds prior work
  across every repo you've ever used codescope in).

A lightweight convention that keeps them in sync: at the end of
each phase's `/gsd-ship N`, also run
`knowledge_save(title=<phase title>, tags=[status:done, vX.Y.Z,
<area>])` so the phase ends up in both places. GSD doesn't do this
automatically; we're not adding it to codescope either — too tight
a coupling for a fast-moving external tool.

## Dream + GSD

codescope's [Dream view](../../README.md) reads tag-clustered
`knowledge` entries as narrated arcs. If you follow the convention
above, every GSD phase becomes a Dream scene, and each milestone
becomes an arc — retrospectives for free.

## Gotchas

- **Don't run both installers with `--dangerously-skip-permissions`
  at the same time on a fresh machine** until you've verified both
  produced the expected files. GSD's recommended install flag plus
  codescope's `init` both write config; nothing destructive, but
  easier to audit one at a time.
- **`codescope hook install` and GSD's hooks coexist.** Both go
  into `~/.claude/settings.json` under `hooks.PreToolUse[]`;
  codescope's installer appends, GSD's installer appends. Order is
  preserved. If a command matches both, both hooks fire.
- **Daemon port conflict.** codescope's HTTP daemon runs on 9877;
  GSD doesn't run a daemon, so no clash. If you run `codescope
  daemon-start` expect to find it at `http://127.0.0.1:9877/mcp`.

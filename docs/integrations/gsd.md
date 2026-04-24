# Using codescope with GSD (Get Shit Done)

> **TL;DR** — [GSD](https://github.com/gsd-build/get-shit-done) is a
> spec-driven workflow layer. codescope is a code-intelligence
> layer. Install both — they don't overlap.
>
> GSD ships in two generations:
>
> - **v1** (`get-shit-done-cc`) — a prompt framework that hijacks
>   Claude Code via slash commands + subagents. Runs inside an
>   existing Claude session.
> - **v2** (`gsd-pi`) — a standalone CLI agent built on the Pi SDK
>   with its own MCP client, crash recovery, cost tracking, and
>   autonomous mode.
>
> codescope works with both. v2 is the current path.

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

### codescope (either path)

```bash
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash
# or Homebrew:
brew install onur-gokyildiz-bhi/codescope/codescope
codescope start                # bring up the bundled surreal server
cd your-project
codescope init --agent claude-code
```

### GSD v2 (recommended)

```bash
npm install -g gsd-pi@latest
gsd config                     # one-time wizard: auth, default model
```

GSD v2 has a built-in MCP client. To let it see codescope's tools
on this project, enter the project dir and either:

- Use `/gsd` inside Claude Code (picks up the project's `.mcp.json`
  transitively via the Claude Code runtime), **or**
- Run `gsd` standalone and install codescope as an MCP extension
  from the universal-config discovery flow (one-time `gsd mcp`).

`gsd auto` runs without the TUI. `gsd headless` is for CI / scripts.

### GSD v1 (legacy, prompt framework)

```bash
npx get-shit-done-cc@latest --claude --local
```

Installs slash commands (`/gsd-new-project`, `/gsd-plan-phase`,
etc.) and 30+ subagents into `.claude/commands/gsd/`. When invoked
inside a Claude Code session, spawned subprocesses inherit the
project's `.mcp.json` and can reach codescope. Still works, but
`gsd-pi` is the maintained path.

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

## State overlap: GSD graph ↔ codescope `knowledge`

Both tools keep their own graph:

- **GSD v2** writes `.gsd/graphs/graph.json` from its `.gsd/`
  artifacts (milestones, slices, tasks, rules, patterns, lessons).
  `gsd graph build/query/status/diff` operates on it.
- **codescope** writes to per-repo SurrealDB tables (functions,
  calls, knowledge, conversations, …).

They don't overlap in content — GSD's graph is about *the plan*,
codescope's is about *the code*. Query whichever fits the question.

GSD v1 used `STATE.md` for progress. Codescope's
`knowledge_save(tags=[status:done, shipped:YYYY-MM-DD, vX.Y.Z])`
tracks the same thing in a cross-project graph. Keep both:

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

# Quickstart — 60 seconds to your first graph query

This walkthrough takes you from zero to a working code knowledge graph and shows what you should see at each step. If anything looks different, skip to [troubleshooting](troubleshooting.md).

## 1. Install

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.ps1 | iex
```

Verify:

```bash
codescope --version
# codescope 0.5.0
```

The installer drops two binaries into `~/.local/bin` (Unix) or `%USERPROFILE%\.local\bin` (Windows): `codescope` and `codescope-mcp`.

## 2. Index a project

Pick any real codebase you own. Ripgrep is a great first target — small enough to finish in under a minute, interesting enough to produce a meaningful graph.

```bash
cd ~/src/ripgrep
codescope init
```

`init` is a one-shot command that:

1. Indexes the current directory into a local SurrealDB graph
2. Writes a `.mcp.json` file so Claude Code / Cursor / Zed can discover the MCP server
3. Embeds functions for semantic search

Expected output (elided):

```
Indexing ripgrep...
  Files:      142 parsed / 216 seen
  Entities:   4,623 functions, classes, imports
  Relations:  16,535 edges (calls, contains, imports, inherits)
  Time:       37s
  DB size:    22 MB at ~/.codescope/db/ripgrep

Wrote .mcp.json (project-scoped MCP server)
Codescope is ready. Open this folder in Claude Code.
```

If indexing is dramatically slower or fails on a specific file, jump to [troubleshooting](troubleshooting.md#indexing).

## 3. Run your first queries from the CLI

Before wiring up an agent, sanity-check the graph directly:

```bash
codescope search "parse"
```

Expected: a list of function names matching `parse`, each with file path and line number.

```bash
codescope query "SELECT count() FROM \`function\` GROUP ALL"
```

Expected: a single row with the total function count (around 4,600 for ripgrep). Note the backticks around `function` — it is a reserved word in SurrealQL.

```bash
codescope query "SELECT name, <-calls<-\`function\`.name AS callers FROM \`function\` WHERE name = 'build' LIMIT 1"
```

Expected: a single row with `build` and a non-empty array of callers. This is a native graph traversal — it should take under 5 ms on a cold database. If this returns an empty array, your calls table did not populate; see [troubleshooting](troubleshooting.md#empty-graph).

## 4. Hook up an AI agent

Codescope's primary interface is the MCP server. `codescope init` already wrote `.mcp.json` in your project, so any MCP-aware client will pick it up automatically.

**Claude Code** — just open the project folder. Claude Code reads `.mcp.json` on startup and exposes the 52 codescope tools.

**Cursor / Zed / Codex CLI / Gemini CLI** — same story, they each read `.mcp.json` from the workspace root.

Verify the tools are exposed by asking the agent:

> "Which MCP tools do you have available?"

Expected: a list that includes `search_functions`, `find_callers`, `impact_analysis`, `type_hierarchy`, `explore`, `context_bundle`, and about 46 more.

## 5. Ask an interesting question

The whole point of graph-first code intelligence is questions that grep and embeddings cannot answer. Try one:

> "What functions transitively depend on `parse_args`? Show me 3 hops out."

The agent should call `impact_analysis` with `depth=3` and return a tree of direct callers, their callers, and so on. Expected latency: under 10 ms for the underlying query. This is the kind of question a vector database cannot answer at all — embeddings do not know about call relationships.

Another:

> "Show me the type hierarchy for the `Matcher` trait."

Agent should call `type_hierarchy`, returning parents (what `Matcher` extends), subtypes, implementors, and interfaces in one response.

And the token-efficient variant of "read the whole file":

> "What's in `crates/core/src/parser.rs`?"

Agent should call `context_bundle`, returning functions, classes, imports, cross-file callers, and relationships — typically around 500 tokens compared to 40,000+ for reading the file directly.

## What just happened

You now have:

- A local SurrealDB graph with every function, class, call edge, import, and type relationship from ripgrep
- An MCP server exposing 52 graph-query tools to any agent that speaks MCP
- Sub-millisecond query latency for structural questions your agent would otherwise try to answer by grepping or reading files

No cloud calls, no telemetry, no waiting on embeddings. Everything ran locally and the graph lives under `~/.codescope/db/<repo>`.

## Where to go next

- [Benchmarks](../BENCHMARKS.md) — real latency numbers across ripgrep, axum, tokio, and gin
- [Tool reference](../README.md#tools) — the full catalogue of 52 MCP tools grouped by category
- [Troubleshooting](troubleshooting.md) — common issues and their fixes
- [Contributing](../CONTRIBUTING.md) — build from source, add a language, open a PR

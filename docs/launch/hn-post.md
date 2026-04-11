# Hacker News launch post — draft

**Status:** draft for final review. Do not post until the sprint is closed and the repo is clean.

**Recommended posting window:** Tuesday 6:00-7:30 AM PST (peak HN engagement for technical Show HN posts).

**Title:** `Show HN: Codescope – Graph-first code knowledge engine for AI agents (Rust)`

Character count: 77. HN title limit is 80. Good.

Alternative titles (if the first one gets buried or the conversation needs redirecting):
- `Show HN: Sub-millisecond 3-hop code impact analysis via graph traversal (Rust, MCP)`
- `Show HN: I built the missing graph layer for Claude Code / Cursor`

---

## Body

Hi HN,

I've been building Codescope for a while and the OSS launch is today. It's a Rust code intelligence engine built around a SurrealDB knowledge graph and 52 MCP tools for AI agents.

The pitch in one line: **most AI code context tools (Cursor, Windsurf, Continue) are embeddings-first. Codescope is graph-first.** Embeddings are great for "find code that *means* X" but they can't answer structural questions like:

- "What functions transitively depend on `parse_config`?"
- "If I change `User::email`, what breaks across the codebase?"
- "Show me the call graph three hops out from `main`."

These are graph traversal questions and a vector database cannot answer them at all. Codescope parses your code with tree-sitter into 4.6k–13.6k entities plus 11k–45k relations (across the four repos I benchmarked), stores them in SurrealDB, and exposes 52 MCP tools so any agent that speaks MCP can walk the graph.

**Benchmarks** (Windows 11, Rust 1.91, SurrealKV embedded, cold start, full methodology in BENCHMARKS.md):

| Repo | Entities | Edges | 2-hop impact | 3-hop impact |
|------|----------|-------|--------------|--------------|
| ripgrep | 4.6k | 16.5k | 0.64 ms | 0.92 ms |
| axum | 5.3k | 15.1k | 0.56 ms | 0.52 ms |
| tokio | 13.6k | 44.7k | 0.68 ms | 0.49 ms |
| gin (Go) | 2.4k | 11.3k | 1.44 ms | 1.29 ms |

The interesting result: **multi-hop traversal stays sub-millisecond regardless of repo size** because it walks indexed graph edges in a single statement. The same question answered with a `WHERE out.name = 'X'` filter on the calls table scales linearly with edge count (40 ms on gin, 118 ms on tokio) — so multi-hop is actually *faster* than single-hop with a WHERE clause, because WHERE has no graph index. This is the property that makes graph-first viable for agents: "who transitively depends on this?" is bounded by graph fan-out, not corpus size.

**What you can do with it:**

- `impact_analysis("parse_config", depth=3)` — transitive BFS through the call graph
- `type_hierarchy("Matcher")` — parents, subtypes, implementors, interfaces in one response
- `find_callers`, `find_callees`, `context_bundle`, `explore` — the usual LSP-adjacent queries
- Plus semantic search with 32x binary quantization for "find me config parsing functions"
- All local, no cloud calls, no telemetry

**Install:**

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash
# Windows (PowerShell)
irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.ps1 | iex
```

Then `cd <your-project> && codescope init` and open Claude Code. That writes a `.mcp.json` and you get the 52 tools.

**What's intentionally not here:** this is not an IDE replacement, not a language server, not a cloud service. It sits next to your LSP and next to your notes (there's even an `export_obsidian` tool if you use Obsidian).

Repo: https://github.com/onur-gokyildiz-bhi/codescope

Happy to answer questions about the graph schema, the SurrealDB traversal syntax gotchas (I shipped a bogus "6.4M× speedup" once that turned out to be a parse error silently swallowed — fixed, documented, and that experience is why I now have an "escape hatch" label on the `raw_query` tool), tree-sitter language support, or anything else. Full disclosure: solo maintainer, so please be nice about issue response time.

---

## Response templates for common questions

**"Why not use Sourcegraph SCIP?"**
SCIP is excellent for structural navigation but it's a single-step lookup ("what does this symbol reference"), not a graph you can walk N hops. Codescope's impact_analysis does iterative BFS to configurable depth and runs in under a millisecond. The other difference is local-first: Sourcegraph wants you to run a cluster; codescope runs in your terminal with an embedded database.

**"Why not use Cursor's codebase indexing?"**
Cursor is embeddings-first through Turbopuffer (cloud). That's great for semantic similarity but it doesn't model call graphs, type hierarchies, or impact analysis. Also cloud-only. Codescope is the structural reasoning layer; use both if you want.

**"Why SurrealDB instead of a 'real' graph database like Neo4j?"**
Three reasons: (1) SurrealDB embeds — no separate process, no cluster, no Java runtime, single binary install; (2) SurrealQL supports native graph traversal syntax (`<-calls<-\`function\`<-calls<-\`function\`.name`) that the optimizer walks as a single indexed edge walk; (3) I can ship it as a Rust library dependency.

**"52 MCP tools feels like a lot. Which ones do you actually use?"**
The core five are: `search_functions`, `find_function`, `find_callers`, `impact_analysis`, `context_bundle`. Those cover about 80% of agent queries in practice. The other 47 are specialized (ADR management, conversation memory, hotspot detection, change coupling, dead code, semantic search, etc.). One of the post-launch follow-ups is a tool usage audit to see what to trim.

**"How does this compare to Obsidian for code?"**
Different layer. Obsidian is brilliant for notes but it doesn't parse code — there's no AST, no call graph, no type hierarchy. Codescope is the code graph layer; pair it with Obsidian (or your notes tool of choice) via the `export_obsidian` MCP tool. Not rivals, adjacent.

**"What about language X?"**
Currently 59 formats (47 tree-sitter languages + 12 content/config parsers). If your language has a tree-sitter grammar, adding it is usually a thin extractor module. PRs welcome.

**"Is this a product or a hobby project?"**
Fully open source (MIT), no commercial plans today, no telemetry. Built because I needed it for my own work. Post-launch I'll decide based on usage whether there's a natural hosted offering, but nothing is behind a paywall and nothing will be.

---

## Sparring list — objections to prep for

1. "Graph-based code search has been tried before (Sourcegraph, Semantic, Glean) and hasn't beaten grep for most queries." → True for exact text search; codescope's pitch is *structural* queries that grep cannot answer at all, not a faster grep.
2. "LLMs will eventually do this reasoning themselves from raw source." → Maybe, but context windows aren't there yet and the cost is 1000x what a graph query costs.
3. "SurrealDB is immature / unstable." → I pin to 3.0.5 and have a test suite; the SurrealKV backend is single-file embedded, no network surface, no cluster state to corrupt.
4. "Why Rust? Could have built this in Python in a weekend." → Single-binary install, no Python runtime on user's machine, sub-millisecond query latency, MCP server stays hot across sessions.
5. "I don't use MCP / Claude Code." → CLI works standalone: `codescope search`, `codescope query`, `codescope-web` for the 3D UI. MCP is the main interface but not the only one.

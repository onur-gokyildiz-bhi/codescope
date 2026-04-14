# Shannon's Token-Budget Audit — 2026-04-14

> "Every byte in the context is a byte you paid for."

## Mechanisms

| Mechanism | Status | Notes |
|---|---|---|
| Delta-mode context_bundle | OK | `context_cache: Arc<RwLock<HashMap<String,String>>>` wired in `server.rs:38,72,85,131-134`; `tools/exploration.rs:29,197-253` compares byte-for-byte, returns `UNCHANGED` or line-diff. Telemetry (`tracing::info!` at `codescope.cache` target) logs `cold`/`unchanged`/`changed` outcomes — good. Cache is ONLY populated on the `file_context` success branch, so repeated failed lookups don't poison it. |
| Result archive (impact_analysis) | OK | `tools/callgraph.rs:8,209` — `maybe_archive(self.result_archive(), "impact_analysis", output)` is the final expression. |
| Result archive (search neighborhood) | OK | `tools/search.rs:412` — `crate::helpers::maybe_archive(server.result_archive(), "explore", output)`. Note: archiving is applied ONLY to the `neighborhood` mode; `fuzzy`, `exact`, `file`, `cross_type`, `backlinks` are unwrapped. |
| `retrieve_archived` round-trip | OK | `tools/search.rs:120-133` reads from `self.result_archive()` by ID; matches the ID format `"{tool_name}_{len}"` emitted by `helpers.rs:43`. |
| Tool description budget (target ≤100) | avg 92 / max 291 / p95 ≈189 | 10 of 32 tools exceed the stated 100-char budget. See table below. |
| Archive eviction (100-entry LRU cap per persona doc) | NOT IMPLEMENTED | `helpers.rs:33-56` just inserts; no cap, no LRU. Persona's "Known gotcha" is still real — daemon sessions leak. |

## Description-length audit (32 tools)

Over-budget (>100 char):

| Tool | Chars | Notes |
|---|---|---|
| `search` | 291 | Mode enumeration inlined; could be trimmed to mode list + "see params". |
| `code_health` | 189 | Per-mode descriptions inline. |
| `contributors` | 147 | Per-mode descriptions inline. |
| `http` | 142 | Per-mode descriptions inline. |
| `skills` | 139 | Per-mode descriptions inline. |
| `conversations` | 131 | Per-mode descriptions inline. |
| `knowledge` | 128 | Describes scope options. |
| `lint` | 119 | Per-mode descriptions inline. |
| `memory` | 109 | Per-mode descriptions inline. |
| `retrieve_archived` | 100 | Exactly at cap. |

Shortest: `supported_languages` (35), `scaffold` (48), `export_obsidian` (49). Pattern: every multi-mode tool blew the budget by enumerating modes in the description. Savings if all trimmed to ≤100: ~600 chars off the 2,955-char total (~20% reduction in tool-description surface area in the MCP system prompt).

## Potential savings — tools returning >2KB without archiving

Spot-checked by inspecting loop accumulators and format! density:

- **`community_detection`** (`tools/analytics.rs:95-184`) — emits THREE markdown tables (clusters, bridges, central) with `limit` default 20 rows each + headers. With `limit=20`, easily ~3-5 KB. NOT archived. **Archive candidate.**
- **`code_health` mode=hotspots / churn / coupling** (`tools/temporal.rs:47-272`) — table-format with `limit` default 20-200. Hotspots with default 200 can push >5 KB. NOT archived. **Archive candidate.**
- **`code_health` mode=review_diff** (`temporal.rs:133-266`) — iterates every changed file × every entity in that file. On a large PR: easily >10 KB. NOT archived. **Archive candidate.**
- **`lint` mode=smells** (`tools/quality.rs:246-383`) — FOUR tables (god functions, fan-in, fan-out, dense files) + cycles + duplicates. With `limit=10`, still ~2-3 KB; with higher limits balloons. **Archive candidate.**
- **`lint` mode=dead_code** (`quality.rs:166-244`) — single table, `limit` default 50. ~2-3 KB typical. Borderline.
- **`export_obsidian`** (`analytics.rs:187+`) — writes to filesystem, returns a short summary (verified at line ~260). OK.
- **`search` mode=backlinks** (`search.rs:418-487`) — four list sections; can exceed 2 KB on hub entities. NOT archived. Consider archiving alongside `neighborhood`.
- **`search` mode=cross_type** (`search.rs:281-326`) — eight sections × `limit` default 10 + headers. ~2-3 KB. Borderline archive candidate.
- **`find_callers` / `find_callees`** (`search.rs:40-78`) — return `serde_json::to_string_pretty(results)`; on hot functions with dozens of callers that's >2 KB of JSON. Not archived. **Archive candidate** (and consider a markdown formatter instead — JSON pretty-print is token-heavy for humans+LLMs).
- **`raw_query`** — user-controlled; `to_string_pretty`-style output is token-heavy. Skip (user escape hatch).

## Delta-mode subtleties

- Line-diff uses `HashSet<&str>` over `lines()` (`exploration.rs:216-219`). For a file with many duplicate lines (e.g., blank lines between sections), added/removed counts under-count — dedup by line-content. Cheap to live with.
- `unchanged` branch re-formats a short summary (`exploration.rs:208-213`) — ~100 chars. Good.
- `output.lines().count()` is called twice in `helpers.rs:44,45` inside `maybe_archive`. O(n) scan × 2. Micro-optimization; not a token issue.

## MCP system-prompt overhead — further reduction ideas

1. **Trim per-mode enumerations out of descriptions.** Move the mode list into the tool's parameter JSON-schema `description` field (one schema fetch, not every prompt). Saves ~600 chars.
2. **Collapse `find_callers` + `find_callees`.** Both are 1-hop; a single `find_neighbors(direction: "in"|"out")` tool would halve description cost (currently 77+92=169 → ~95).
3. **Remove `supported_languages` from the tool surface.** It's a one-shot introspection rarely called; can be exposed as a URI resource or a static README line. Saves 35 chars + schema.
4. **`retrieve_archived` only useful when an archive exists.** Consider hiding it from the tool list unless `result_archive.len() > 0` at server start (MCP doesn't support dynamic tool lists well today — track as future rmcp feature). Alternatively, move it into `search` as a mode.
5. **`raw_query` description could add "(escape hatch)"** so Claude defers to structured tools. Currently says "Prefer dedicated tools first" — good.
6. **Server `instructions` (injected project context).** `helpers.rs:175-306` (`build_context_summary`) + `build_post_index_insights` (`helpers.rs:608-748`) generate potentially multi-KB content that lands in the MCP `ServerInfo.instructions`. This is per-session fixed cost; worth measuring. Cap it (e.g., top-5 decisions, top-3 problems) and push overflow to `CONTEXT.md` which the agent reads on-demand.
7. **Archive key is predictable.** `"{tool}_{archive.len()}"` — two concurrent archives from the same tool could collide if the map is cleared. Use an atomic counter or a short hash. Tokens saved: none; correctness win.

## Action items

1. **Trim 10 over-budget descriptions** by moving mode lists into JSON-schema param docs. Target: every tool ≤100 chars. Estimated saving: ~600 chars per session.
2. **Wrap `maybe_archive` around**: `community_detection`, `code_health` (all modes), `lint` (smells, dead_code), `search` modes `backlinks` / `cross_type`, `find_callers`, `find_callees`.
3. **Implement the 100-entry LRU cap** in `helpers.rs` `maybe_archive` — the persona doc promises it; code doesn't deliver.
4. **Switch `find_callers`/`find_callees` output from pretty-JSON to markdown list** (matches the other search outputs). Typical saving: 40-60% on those calls.
5. **Cap `build_context_summary` / `build_post_index_insights` injection** (server-info `instructions`) to a hard char budget (e.g., 2 KB). Overflow lives in `CONTEXT.md`.
6. **(Optional) Consolidate `find_callers` + `find_callees` into `find_neighbors(direction)`** — halves tool surface for the 1-hop pair.
7. **(Optional) Atomic counter for archive IDs** — avoid collisions on manual archive clears.

## Sanity checks confirmed

- `context_cache` and `result_archive` are both cloned `Arc<RwLock<_>>` on `new` and `new_daemon` (`server.rs:72-73, 85-86`). Delta and archive survive across tool calls within a session.
- `maybe_archive` threshold is 4096 chars (`helpers.rs:38`) — matches persona doc.
- Archive ID format and retrieval are consistent.

Files inspected:
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\helpers.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\server.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\tools\exploration.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\tools\callgraph.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\tools\search.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\tools\temporal.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\tools\quality.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\tools\analytics.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\tools\contributors.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\mcp-server\src\tools\conversations.rs`

# Dewey's Tool Surface Audit — 2026-04-14

Auditor: Dewey (mcp-tool-curator)
Layer: `crates/mcp-server/src/tools/`
Budgets: tools ≤ 40, description ≤ 100 chars, imperative + disambiguating keywords.

## Tool inventory

- **Total: 32** (well inside the 40 budget; matches v0.7.5 consolidation target)

By file (`grep -c '#\[tool(' crates/mcp-server/src/tools/*.rs`):

| File | Tools | Names |
|---|---:|---|
| `admin.rs` | 2 | `project`, `index_codebase` |
| `adr.rs` | 1 | `manage_adr` |
| `analytics.rs` | 5 | `api_changelog`, `community_detection`, `export_obsidian`, `capture_insight`, `suggest_structure` |
| `ask.rs` | 1 | `ask` |
| `callgraph.rs` | 2 | `impact_analysis`, `type_hierarchy` |
| `contributors.rs` | 1 | `contributors` |
| `conversations.rs` | 1 | `conversations` |
| `embeddings.rs` | 2 | `embed_functions`, `semantic_search` |
| `exploration.rs` | 1 | `context_bundle` |
| `http.rs` | 1 | `http_analysis` |
| `knowledge.rs` | 1 | `knowledge` |
| `memory.rs` | 1 | `memory` |
| `quality.rs` | 2 | `lint`, `edit_preflight` |
| `refactor.rs` | 1 | `refactor` |
| `search.rs` | 7 | `search`, `find_callers`, `find_callees`, `graph_stats`, `raw_query`, `supported_languages`, `retrieve_archived` |
| `skills.rs` | 1 | `skills` |
| `temporal.rs` | 2 | `sync_git_history`, `code_health` |

Full sorted tool list (32): `api_changelog, ask, capture_insight, code_health, community_detection, context_bundle, contributors, conversations, edit_preflight, embed_functions, export_obsidian, find_callees, find_callers, graph_stats, http_analysis, impact_analysis, index_codebase, knowledge, lint, manage_adr, memory, project, raw_query, refactor, retrieve_archived, search, semantic_search, skills, suggest_structure, supported_languages, sync_git_history, type_hierarchy`.

## Description quality

### Violations of the 100-char budget (12 tools)

Descriptions that bust the per-tool budget — trim the action/mode enumerations or push them into params docs:

| Tool | ~len | Description |
|---|---:|---|
| `search` | ~270 | "Unified search: mode=fuzzy\|exact\|file\|cross_type\|neighborhood\|backlinks. …" — six modes enumerated inline |
| `code_health` | ~200 | "Code health analysis: mode=hotspots\|churn\|coupling\|review_diff. …" |
| `skills` | ~170 | "Skills/knowledge graph: action=index\|traverse\|generate. …" |
| `contributors` | ~157 | "Contributors: mode=map\|reviewers\|patterns. …" |
| `conversations` | ~140 | "Conversations: action=index\|search\|timeline. …" |
| `http_analysis` | ~138 | "HTTP analysis: mode=calls\|endpoint_callers. …" |
| `knowledge` | ~137 | "Knowledge graph: action=save\|search\|link\|lint. …" |
| `lint` | ~128 | "Lint: mode=dead_code\|smells\|custom. …" |
| `memory` | ~113 | "Memory: action=save\|search\|pin. …" |
| `refactor` | ~107 | "Refactor ops: action=rename\|find_unused\|safe_delete. …" |
| `retrieve_archived` | ~103 | "Retrieve full output of an archived large result. …" |
| `project` | ~101 | "Project management: action=init\|list. …" |

Pattern: every consolidated tool is over-budget because mode/action enumerations live in the description instead of in the `ParamsV` schema's `#[schemars(description = …)]` on the `mode`/`action` field.

### Weak / vague descriptions (missing domain keywords)

These pass the char budget but under-sell the tool — the model won't pick them based on keyword match alone:

- `index_codebase` — "Index source files into the knowledge graph." No mention of what it enables (everything else depends on it). Add "run first" or "prerequisite for all queries".
- `embed_functions` — "Generate vector embeddings for unembedded functions." Missing the hook: no mention that this is the prerequisite for `semantic_search`.
- `suggest_structure` — "Suggest directory structure for a new project." Zero signal about *how* (from what data?) — is this LLM-guessed or graph-derived? Looks like slop next to `impact_analysis`.
- `export_obsidian` — "Export code graph as Obsidian vault with wikilinks." Fine but consider adding "for human browsing" to disambiguate from `raw_query`-for-export.
- `edit_preflight` — "Check if edit aligns with team coding patterns." The word "patterns" is doing too much work — could be style, architecture, security. Concretize: "naming, module layout, imports".
- `supported_languages` — "List supported programming languages." Trivial, fine.
- `graph_stats` — "Code graph statistics: files, functions, classes, relationships count." Fine.

No tool has a placeholder "Helper"/"Utility" description — the author(s) avoided that anti-pattern.

## Consolidation candidates

Pairs where descriptions differ in fewer than 3 unique keywords — strong signal the tool can collapse into a mode-param.

### 1. `find_callers` + `find_callees` → single tool or mode on `search`

- `find_callers`: "Find direct callers of a function (1-hop). For transitive use impact_analysis."
- `find_callees`: "Find direct callees of a function (1-hop). For full neighborhood use search mode=neighborhood."

Difference: the word `callers` vs `callees` and the cross-reference tail. Only 2 unique keywords differ (`callers`/`impact_analysis` vs `callees`/`neighborhood`).

**Proposal:** fold into `search(mode="callers")` / `search(mode="callees")`. `search` already has `mode=neighborhood` and `mode=backlinks` which are conceptually adjacent. Saves 2 tool slots, unifies graph-edge lookups into one dispatcher.

Counter-argument: these are the two highest-frequency tools in the catalog. Keeping them as one-word top-level verbs may be worth the duplication. Flag for discussion, do not auto-consolidate.

### 2. `semantic_search` + `search(mode=fuzzy)` — keyword overlap

Both find functions by loose matching. `semantic_search` is vector-based, `search mode=fuzzy` is substring-based. The user-facing intent ("find something like X") is identical; only the backend differs.

**Proposal:** add `mode="semantic"` to `search`, deprecate `semantic_search` tool. The `search` description already lists 6 modes — 7 is not meaningfully worse, and it's already over-budget. Merge the budget fix with the consolidation.

### 3. `ask` vs `search` — fuzzy boundary

- `ask`: "Natural language question about the codebase. Auto-extracts search terms."
- `search(mode=fuzzy)`: substring search

Overlap: both take a free-form string. `ask` wraps LLM term-extraction; `search` is direct. Keep separate — different semantics — but tighten `ask`'s description to say "LLM-planned multi-tool query" so the model knows when to reach for it vs `search`.

### 4. `sync_git_history` + `code_health` — adjacent git-aware tools

`sync_git_history` is a prerequisite for `code_health`'s `hotspots` and `coupling` modes. Not a consolidation candidate (different verbs — "sync" vs "analyze") but the description of `sync_git_history` should cross-reference `code_health` explicitly.

### 5. `capture_insight` vs `memory(action=save)` vs `knowledge(action=save)` — three write paths

- `capture_insight`: "Record insight: decision, problem, solution, correction, learning."
- `memory action=save`: "persist note"
- `knowledge action=save`: part of the `knowledge` consolidated tool

Three ways to save a thought. From the CLAUDE.md the project is already signalling confusion ("Memory (lightweight — don't overthink it)"). Fewer than 3 unique keywords differ between `capture_insight` and `memory save`.

**Proposal:** collapse `capture_insight` into `memory(action=save, kind=insight)` or `knowledge(action=save, kind=insight)`. Kill one of the three. Current top candidate: fold `capture_insight` into `memory`, since `memory`'s description already lists decision/problem/solution.

### 6. `manage_adr` vs `knowledge` — ADRs are decision-kind knowledge

`manage_adr` is a first-class top-level tool for ADRs. `knowledge` already stores decisions. Difference: ADRs have structured fields (status, consequences, context). If those fields can live as `knowledge` metadata, `manage_adr` becomes `knowledge(action=save, kind=adr)` + `knowledge(action=search, kind=adr)`.

**Proposal:** evaluate merging `manage_adr` into `knowledge` with `kind=adr`. Saves a slot. Keep as-is if ADR lifecycle (status transitions) is actually distinct.

## Missing tools (expected but absent)

Cross-checked the caller's catalog list against the actual 32-tool surface. **All expected tools are present** — nothing missing. Confirming presence:

- ask — present
- capture_insight — present
- knowledge — present
- memory — present
- search — present
- context_bundle — present
- impact_analysis — present
- find_callers — present
- find_callees — present
- type_hierarchy — present
- semantic_search — present
- code_health — present
- lint — present
- refactor — present
- project — present
- conversations — present
- skills — present
- http_analysis — present
- raw_query — present
- graph_stats — present
- supported_languages — present
- export_obsidian — present
- manage_adr — present
- retrieve_archived — present
- embed_functions — present
- suggest_reviewers — **absent as a standalone tool** but folded into `contributors(mode=reviewers)` — correct consolidation, not missing
- contributors — present
- suggest_structure — present
- community_detection — present
- api_changelog — present
- edit_preflight — present
- sync_git_history — present

The only "missing" entry (`suggest_reviewers`) was intentionally folded into `contributors` in v0.7.5. Catalog and surface match.

## Consolidated-tool mode documentation audit

Caller asked whether mode/action params are well-documented *in the description*. They are documented inline but at the cost of the 100-char budget (see Violations section). Per-tool verdict:

| Tool | Modes enumerated? | All modes explained? | Budget? |
|---|---|---|---|
| `search` | yes (6) | yes, one-line each | over (~270) |
| `knowledge` | yes (4) | partial — `scope` explained, actions not individually described | over (~137) |
| `memory` | yes (3) | yes | over (~113) |
| `refactor` | yes (3) | lumped into one tail ("Shows references, dead code, or delete safety check.") — actions not mapped 1:1 to outputs | over (~107) |
| `http_analysis` | yes (2) | yes | over (~138) |
| `code_health` | yes (4) | yes | over (~200) |
| `conversations` | yes (3) | yes | over (~140) |
| `skills` | yes (3) | yes | over (~170) |
| `project` | yes (2) | yes | ~at limit (~101) |
| `lint` | yes (3) | yes | over (~128) |
| `contributors` | yes (3) | yes | over (~157) |

Only `refactor` has a **quality** issue on top of the budget issue: "Shows references, dead code, or delete safety check." does not map to `rename/find_unused/safe_delete` in order. A reader has to guess which action produces which output. Rewrite to `rename: shows references. find_unused: dead code. safe_delete: safety check.` (same length, fixes the mapping).

All other consolidated tools map action → outcome unambiguously.

## Action items

1. **Push mode/action docs into param schemas.** Move the enumeration from `#[tool(description)]` to `#[schemars(description)]` on the `mode`/`action` field. Drops 12 tool descriptions under the 100-char budget without losing information. The MCP client still surfaces param descriptions in the schema. Highest ROI.
2. **Fix `refactor` action→output mapping.** Rewrite description so each action maps to its output in order.
3. **Decide on `semantic_search` → `search(mode=semantic)` consolidation.** Saves a tool slot and aligns with v0.7.5 direction.
4. **Decide on `capture_insight` → `memory(action=save, kind=insight)` consolidation.** Three write paths for "save a thought" is a known confusion point per `CLAUDE.md`.
5. **Decide on `manage_adr` → `knowledge(kind=adr)` consolidation.** Only if ADR status-transition logic can live in metadata.
6. **Re-evaluate `find_callers`/`find_callees` merge into `search`.** They are high-frequency — keeping as top-level verbs is defensible. Document the call, do not auto-consolidate.
7. **Strengthen weak descriptions:**
   - `index_codebase`: add "run first — prerequisite for all queries".
   - `embed_functions`: add "required before semantic_search".
   - `suggest_structure`: clarify the data source ("graph-derived" vs "LLM-guessed").
   - `edit_preflight`: replace "patterns" with concrete dimensions ("naming, imports, module layout").
8. **Cross-reference `sync_git_history` → `code_health`** in the description so the model knows the prerequisite order.
9. **After all of the above:** target count is 32 → 29 (drop `semantic_search`, `capture_insight`, `manage_adr`). All under 100-char budget. Ship as v0.7.6.

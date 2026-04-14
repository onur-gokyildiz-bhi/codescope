# Ranganathan's Knowledge Graph Hygiene — 2026-04-14

> "If future-us can't find what past-us saved, we saved nothing."

## Counts

- **Total entries:** 43
- **status:done:** 12
- **status:planned:** 14
- **status:in-progress:** 0
- **status:blocked:** 0
- **Untagged-by-status** (concepts/sources/entities without a status facet): **17**

Kinds: concept (heavy majority), decision (shipped features), claim (roadmap narratives), entity (Dean, SurrealDB), source (3 external refs).

Ships dated 2026-04-14: 12 entries — huge burst today, consistent with the refactor/ship cadence in recent git log.

## Red flags

### 1. Orphan epidemic (the catastrophic one)

**39 of 43 entries have zero edges.** Only 4 nodes have any graph connectivity at all:

| Title | Edges |
|---|---|
| Roadmap: Multi-Repo Knowledge Graph | 2 |
| Dean | 1 |
| DGX Spark Platform Support | 1 |
| Roadmap: Compound Knowledge Loop | 1 |

This is not a knowledge *graph*, it's a knowledge *list*. Ranganathan's Second Law ("Every reader his book") is violated — no one can traverse from a shipped feature to its planned successor, from Dean's feedback to the fix that shipped for him, or from a research source to the implementation that uses it.

Specifically missing the highest-value links:

- **Dean Active Usage Confirmed** (entity) → should link to **Dean** (entity), **Install Robustness Fixes From Dean Feedback** (decision), **CUDA Parser Support** (decision, tagged dean-feedback implicitly via CUDA/GPU concept). Currently 0 edges — orphaned at the exact node that proves product-market fit.
- **Graph of Skills — PPR Reranking** (source, tagged `implemented`) → should `implemented_by` **Graph-Ranked Search (Simplified PPR)** (decision). Currently no edge.
- **Token Optimizer — Delta-Mode and Context Budget Patterns** (source, tagged `implemented`) → should `implemented_by` **Delta-Mode context_bundle** and **Result Archiving for Large Tool Outputs**. No edges.
- **Karpathy LLM Wiki Pattern** (source) → cited by **Roadmap: Compound Knowledge Loop** via tag `karpathy`, but no explicit edge.
- **Claude Code MCP Tool Drift — Root Cause Analysis** (concept) → should `supports` **MCP Tool Drift Fix** (decision). Classic problem→fix pair, unlinked.

### 2. Conflicting status tags

- Query `tags CONTAINS 'status:planned' AND tags CONTAINS 'status:done'` returned `[]`. **Clean on this axis.** No leftover planned tags from shipped work.

### 3. Tag vocabulary inconsistencies

- **"Install Robustness Fixes From Dean Feedback"** carries both `shipped:2026-04-14` AND `v0.7.3-pending`. It either shipped today (in which case `v0.7.3-pending` is a lie and should be `v0.7.3`) or it didn't ship (in which case `shipped:2026-04-14` is a lie). Pick one.
- **Missing version tags on shipped entries.** These 4 have `status:done` + `shipped:2026-04-14` but no `vX.Y.Z`:
  - Query Decomposition for Semantic Search
  - Result Archiving for Large Tool Outputs
  - Web UI Design System Upgrade
  - Dean Active Usage Confirmed (not a ship per se, but has the shipped date tag without version — the shipped tag may itself be wrong here; this is an *observation*, not a release)
- **Status-tag coverage gap.** 17 entries have neither `status:done`, `status:planned`, `status:in-progress`, nor `status:blocked`. These are concept/source/entity/claim types where status may not apply — but `codescope-roadmap` entries (Roadmap: GraphRAG as Differentiator, Roadmap: Multi-Repo, Roadmap: Remote MCP, Roadmap: SurrealDB Partnership, Roadmap: Compound Knowledge Loop) clearly should have `status:planned`. They read as roadmap items but aren't tagged as such, so they won't show up in planned-work queries.
- **`shipped:2026-04-14` on an entity.** "Dean Active Usage Confirmed" is kind:entity but carries a ship date. Entities aren't shipped — observations are. Either recast as kind:claim / kind:decision, or drop the shipped tag.
- **`implemented` tag on sources** (Graph of Skills, Token Optimizer) duplicates what an `implemented_by` edge would express. Fine as a breadcrumb, but only if the edge also exists (it doesn't — see orphan problem).
- **Case/spelling consistency:** no drift detected — everything is lowercase-kebab, `priority:high/medium/low` is consistent, `status:*` is consistent. That one-bright-spot is genuinely good.

### 4. Planned work not linked to its prerequisite shipped work

Every planned item is an island. Specifically:

| Planned | Should link to shipped |
|---|---|
| Scalable 3D Graph Viz for Large Repos | Web UI Knowledge Graph Visualization (`extends`) |
| Further Tool Consolidation Round 3 | MCP Tool Drift Fix (`extends`) |
| LSP Bridge for Editor-Agnostic Integration | — (greenfield, ok) |
| VSCode Extension (after LSP) | LSP Bridge (`depends_on`) — this dependency lives only in the parenthetical title |
| CUDA/GPU Code Semantic Support | CUDA Parser Support (already shipped! — this planned item may now be stale or needs re-scoping) |
| Diff-Aware PR Review Command | Multi-Edge Impact Analysis (`uses`) |

Note the CUDA item is especially suspicious: "CUDA Parser Support" shipped today (v0.7.5), yet "CUDA/GPU Code Semantic Support" is still tagged `status:planned priority:high`. Either the planned item is broader scope (then needs a note explaining what's left) or it's stale (should be `status:done`). **The Rule: Search Before You Build** is at risk here — a future session searching "CUDA" will find both and be confused.

## Green flags

- **Zero conflicting status tags.** No `status:planned` + `status:done` cross-contamination. Someone is updating in-place rather than creating duplicates. Good discipline.
- **Consistent tag casing and naming.** `status:*`, `priority:*`, `shipped:YYYY-MM-DD`, `vX.Y.Z` all follow the convention in CLAUDE.md. No `inProgress` vs `in-progress` drift.
- **Ship dates are absolute.** Every shipped entry uses `shipped:2026-04-14`, never "today" or "last week".
- **Source attribution is present.** `dean-feedback`, `gemini-review`, `autoresearch` tags correctly attribute origin. Ranganathan approves.
- **Kind distribution is healthy.** Mix of concept / decision / source / entity / claim — not just a pile of decisions.
- **12 entries shipped today** with matching version + date tags (mostly) — the release tracking discipline works when it runs.

## Action items

1. **FIX THE ORPHAN PROBLEM — this is priority one.** Run a cross-link pass. Minimum viable set:
   - `implemented_by` edges from sources → decisions (Graph of Skills → Graph-Ranked Search; Token Optimizer → Delta-Mode context_bundle).
   - `supports` from "Claude Code MCP Tool Drift — Root Cause Analysis" → "MCP Tool Drift Fix".
   - `related_to` edges from "Dean" → "Dean Active Usage Confirmed" → "Install Robustness Fixes From Dean Feedback" → "CUDA Parser Support".
   - `extends` / `related_to` from each planned item to its nearest shipped ancestor (table above).
   - `cites` from "Roadmap: Compound Knowledge Loop" → "Karpathy LLM Wiki Pattern".
2. **Resolve the CUDA overlap.** Decide whether "CUDA/GPU Code Semantic Support" (planned) is done, partially done, or broader than the shipped "CUDA Parser Support". Update tags and add a `related_to` edge either way.
3. **Fix the version-tag holes.** Add `vX.Y.Z` to the 4 shipped-but-unversioned entries (Query Decomposition, Result Archiving, Web UI Design System Upgrade, Dean Active Usage Confirmed — or drop `shipped:` from the last one if it's an observation).
4. **Disambiguate `v0.7.3-pending` on Install Robustness Fixes.** If shipped today, change to `v0.7.3` (or whatever the actual version is). If not, remove `shipped:2026-04-14`.
5. **Tag the roadmap claims with `status:planned`.** The five `Roadmap:` entries (GraphRAG, Multi-Repo, Remote MCP, SurrealDB Partnership, Compound Knowledge Loop) read as planned work — tag them so they surface in roadmap queries.
6. **Decide on `implemented` tag vs `implemented_by` edge.** If edges exist, drop the tag. If no edges, add them. Don't do both/neither.
7. **Add a `knowledge(action="lint")` run to the pre-ship checklist.** This report should be automatable — the orphan check caught 39 nodes in one query.

---

*Ranganathan's verdict: Good tag discipline, catastrophic link discipline. A library where every book is catalogued but none are shelved near each other. Fix the edges before the next ship, or the compound-knowledge-loop roadmap item becomes ironic.*

# Agent Grill Consolidated Summary — 2026-04-15

11 agents grilled codescope end-to-end. Reports under `.claude/reviews/`.

## P0 — launch blockers (truth-in-labeling, data correctness)

1. **CHANGELOG 7 releases behind** (Hopper) — v0.7.1–v0.7.7 not in CHANGELOG. Release protocol skipped.
2. **README ↔ BENCHMARKS.md drift** (Knuth) — tokio: 33s/12.6K vs 141.8s/13.6K (4× disagreement). "99.6%" hero number has no backing row.
3. **`is_codescope_running()` Windows-broken** (Nightingale) — calls pgrep only. On Windows returns false → `try_remove_stale_lock` removes live-process LOCK → **data corruption**.
4. **4 CONTAINS $bind silent-empty bugs** (Linnaeus) — `temporal/graph_sync.rs:174`, `crossrepo/linker.rs:67`, `adr.rs:91`, `conversations.rs:313` all return empty results silently.
5. **Multi-project leakage** (Linnaeus) — analytics.rs (clusters/bridges/central), quality.rs (hotspots/largest/per-file), query.rs:818 (find_unused_symbols) all missing `WHERE repo = $repo`. Cross-project data bleed.
6. **query.rs:829 schema bug** (Linnaeus) — filters `` `function` `` by `kind` field that doesn't exist on that table. Silently wrong filter.
7. **12 tool descriptions over 100 chars** (Ada, Dewey, Shannon) — all consolidated tools (search ~280, code_health ~200, skills ~170, ...). Root cause: mode/action enumerations in tool description instead of param schema.
8. **Schema version lies** (Linnaeus) — SCHEMA_VERSION=1 but ~12 new tables since. meta:schema now misreports DB capability.

## P1 — correctness / UX severity

9. **Knowledge graph is a list, not a graph** (Ranganathan) — 39/43 entries are orphans. Zero edges. The product's own "cross-link everything" narrative undermined.
10. **LSP `word_at_position` broken for non-ASCII** (Turing) — Rust/Python/JS identifiers with Unicode silently fail. `positionEncoding` never negotiated.
11. **Archive candidates not wrapped** (Shannon) — `community_detection`, `code_health` (all modes), `lint` (smells/dead_code), `search(backlinks/cross_type)`, `find_callers`/`find_callees` still return unarchived multi-KB outputs.
12. **No CI benchmarks** (Knuth) — `ci.yml` has no bench step. Regression detection mandate unmet.
13. **BENCHMARKS.md 5 releases stale** (Knuth) — tool consolidation, CUDA, LSP, migrations, OTLP unmeasured.
14. **LRU cap unimplemented** (Shannon) — persona spec says 100-entry cap, code just `insert`s unbounded.
15. **No dedicated C/C++ test suite** (Chomsky) — operator<=>, destructors, templates untested.
16. **Web UI empty state missing** (Victor) — first-time user sees black void.
17. **`window.THREE` race** (Victor) — octahedron geometry depends on global set after graph init; can silently fail.
18. **codescope-lsp not in release.yml** (Hopper) — protocol says 4 binaries, workflow builds 3.
19. **`setup-claude.ps1 --uninstall` unreachable** (Nightingale) — `$args` parsing lost through `irm | iex` one-liner.

## P2 — technical debt / polish

20. **Tag inconsistency on 5 knowledge entries** (Ranganathan) — "v0.7.3-pending" vs "shipped:2026-04-14" contradiction; 4 shipped entries missing version tag.
21. **CUDA duplicate knowledge entry** (Ranganathan) — "CUDA Parser Support" (status:done) vs "CUDA/GPU Code Semantic Support" (status:planned). Future sessions confused.
22. **7 silent catch {} blocks in frontend** (Victor) — errors swallowed everywhere except Graph3D.
23. **50+ hardcoded px in styles.css** (Victor) — typography scale broken by inline 10px/14px/16px. Token adoption ~60%.
24. **LOD not enforced client-side** (Victor) — agent mandate says <500 visible, UI doesn't cap.
25. **Cluster click only re-centers** (Victor) — expand action not implemented.
26. **Hot-path ORDER BY fields unindexed** (Linnaeus) — knowledge.updated_at, decision.timestamp, file.language.
27. **SymbolKind hardcoded to FUNCTION** (Turing) — classes should use STRUCT.
28. **hover picks `.next()`** on duplicate names (Turing) — should pick line-range match.
29. **N=1 benchmark methodology** (Knuth) — no warmup, no p50/p95, no stddev.
30. **Competitor numbers unsourced** (Knuth) — "Cursor 15-20s" no attribution.
31. **No pre-flight gate in release.yml** (Hopper) — broken tags can ship.
32. **`.h` defaults to C grammar** (Chomsky) — modern C++ uses .h, agent spec says cpp default.
33. **Intel Mac contradiction** (Hopper) — CHANGELOG says added, release.yml says removed.
34. **ServerInfo.instructions bloat risk** (Shannon) — build_context_summary can be multi-KB, no cap.

## Green flags across the audit

- Tool count inside budget: 32/40 ✅ (Ada, Dewey)
- Delta-mode context_bundle wired correctly ✅ (Shannon)
- `retrieve_archived` works end-to-end ✅ (Shannon)
- LSP implements all 6 target methods ✅ (Turing)
- Empty DB handled gracefully in LSP ✅ (Turing)
- CUDA kernel detection fully intact, 4 tests ✅ (Chomsky)
- C/C++ declarator chain 16-level recursive ✅ (Chomsky)
- Auto-lock-recovery (Unix) intact ✅ (Nightingale)
- 18/22 error messages have actionable fixes ✅ (Nightingale)
- v0.7.7 cross-platform binaries shipped cleanly ✅ (Hopper)
- No `any` types in schema ✅ (Linnaeus)
- Multi-hop traversal uses correct chain syntax ✅ (Linnaeus)
- cluster_mode=auto wired ✅ (Victor)
- Knowledge octahedron rendering ✅ (Victor)
- Tag casing consistent in knowledge graph ✅ (Ranganathan)
- Source attribution (dean-feedback, gemini-review) ✅ (Ranganathan)

## Recommended action order

**Block next release on:**
1. CHANGELOG backfill v0.7.1–v0.7.7 (Hopper)
2. Fix 4 CONTAINS $bind bugs (Linnaeus)
3. Fix multi-project leakage queries (Linnaeus)
4. Fix Windows `is_codescope_running` (Nightingale)
5. Truth-up README vs BENCHMARKS.md (Knuth)

**Block launch on:**
6. Knowledge graph cross-linking pass (Ranganathan)
7. Tool description budget (move mode docs to param schemars) (Dewey)
8. LSP Unicode identifier fix (Turing)

**Next session's sprint:**
9. Expand archive coverage to 5+ more tools (Shannon)
10. LRU cap on archive (Shannon)
11. CI benchmarks + historical series (Knuth)
12. Web UI empty state + error handling (Victor)

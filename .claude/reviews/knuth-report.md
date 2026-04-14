# Knuth's Measurement Audit — 2026-04-14

Scope: `crates/bench/src/main.rs` (the bench harness), `BENCHMARKS.md` (the published numbers), `README.md` (the claims), and `.github/workflows/*.yml` (what CI actually enforces).

Catchphrase holds: **numbers or didn't happen**. A lot of numbers exist. A lot of them don't happen in CI, and some don't match between files. Details below.

---

## What we measure

`crates/bench/src/main.rs` runs a 4-phase pipeline per repo:

1. **Source scan** — walks the tree with `ignore::WalkBuilder` (hidden+gitignore on), counts total / supported files and raw source bytes.
2. **Index benchmark** — wipes `$TMP/codescope-bench-<repo>`, opens a fresh SurrealKV DB, runs `GraphBuilder` one file at a time (`insert_entities` + `insert_relations` per file, **no batching**), measures wall-clock. Reports: files_indexed, entities, relations, time_ms, files/sec, entities/sec, DB size on disk.
3. **Query benchmarks** — 14 queries, each timed with `std::time::Instant`, single run, no warmup, no repetition, no statistics. Queries cover:
   - exact + substring function search
   - listing structs, largest functions, imports
   - graph_traversal_callers / callees (1-hop native)
   - `impact_d1_direct` (old WHERE-filter path — kept for regression comparison)
   - `impact_d2_native_traversal`, `impact_d3_native_traversal` (native `<-calls<-\`function\`` chain)
   - `type_hierarchy_traversal`, `fan_in_top10`
   - `impact_analysis_prod_shape` (the real MCP tool pattern)
4. **Token savings** — 4 scenarios. "Traditional" = sum of top-N source file sizes / 4 bytes-per-token. "Codescope" = `serde_json::to_string(response).len() / 4`. Ratio reported as a percentage.

**Dynamic impact target.** Bench discovers the highest-fan-in function per repo via `SELECT out.name, count() FROM calls GROUP BY out.name ORDER BY callers DESC LIMIT 1` and uses it in all impact queries. Prevents the old "hardcoded `main` returns 0 rows" bug. Good.

**Claimed in output:** index throughput, query p-of-1 latency, token-saving percent per scenario, highlight line for 3-hop traversal.

---

## Reproducibility

- Hardware documented: partial — only "Windows 11, Rust 1.91.1, SurrealDB embedded (SurrealKV)". **No CPU, no RAM, no disk type (SSD vs NVMe).** The agent card explicitly calls this out as a known gotcha; the doc doesn't honor it.
- Commit hash tracked: **no**. BENCHMARKS.md has no commit SHA, no date per row, no tree-sitter grammar versions. Numbers cannot be tied to the code that produced them.
- Cold/warm cache noted: **sort of, then contradicted**. Methodology says `Queries: Cold-start (no caching)` and the header says "Cold-start latencies (no caching)". But the harness:
  1. runs `discover_impact_target` before the timed block — which populates SurrealKV page cache for the `calls` table,
  2. then times queries sequentially, so query 14 is fully warm,
  3. performs no explicit throwaway warmup (contradicting the agent-card guidance).
  So the first query is the only "cold" one; the rest are warmed by prior queries. Label is misleading.
- Statistical rigor: **none**. N=1 per query per run. No p50/p95/p99, no stddev, no repeats. A single GC pause or fs flush in the middle of a query would silently corrupt the published number.

---

## Claim validation

| README claim | Evidence in BENCHMARKS.md | Status |
|---|---|---|
| "99.6%" token savings (`Find function + callers` 148K → 542) | BENCHMARKS.md shows **98.5–99.9%** for equivalent scenario across repos; exact "148K → 542" pair does not appear anywhere. | **Drift** — 99.6% is a prose round, not a measured row. |
| "sub-millisecond" / "sub-ms" graph traversal | BENCHMARKS.md: 2-hop native 0.66–1.49 ms, 3-hop native 0.63–1.37 ms. Most rows are sub-ms; tokio and gin exceed 1 ms. | **Mostly true**, but "sub-ms" is misleading for tokio (1.10 ms) and gin (1.49 ms). README line 7 should say "single-digit-millisecond" like BENCHMARKS.md does (line 31). |
| "7 projects across 5 languages" (README line 217, 278) | BENCHMARKS.md indexing table shows **4 projects** (ripgrep, axum, tokio, gin). Token-saving section has 7 (ripgrep, axum, tokio, FastAPI, Gin, Zod, Express). Methodology (line 235) admits: *"Express, Zod, FastAPI numbers in older sections of this doc are from a previous bench run."* | **Drift** — 7 projects are claimed uniformly, but only 4 were re-benchmarked 2026-04-10. Express/Zod/FastAPI rows are stale and not re-verified against current code. |
| README table shows `tokio 769 files, 12,628 entities, 33s, 2ms` | BENCHMARKS.md shows `tokio 812 files, 13,600 entities, 141.8s, various ms`. | **Drift — large.** README entity+file counts and index time disagree with BENCHMARKS.md by 4× on time. README is from an older run and was never re-synced after the 2026-04-10 bench. |
| README: `FastAPI 2713 files, 50,150 entities, 96s, 4ms` | BENCHMARKS.md indexing table has **no row for FastAPI** (acknowledged stale). | **Unverifiable** — no current evidence. |
| README: `Express 158 files, 450 entities, 2s, 0.3ms` | 450 entities for 158 files of JS looks suspicious (2.8 entities/file). Stale; not in current indexing table. | **Unverifiable**. |
| "Multi-hop traversal: 0.5–1.3 ms for 3-hop impact analysis on repos up to 50K entities" (README line 290) | BENCHMARKS.md has 3-hop native at 0.63–1.37 ms. Matches roughly. But the **production tool** (`impact_analysis_prod_shape`) is 1.06–3.26 ms and that's what users actually hit. | **Technically true but misleads.** README quotes the cheapest primitive (`.name`-only projection), not the production shape. BENCHMARKS.md itself flags this distinction (lines 17–20); README doesn't. |
| "~12 seconds / 0.8 ms" comparison (README line 33) | Not tied to any measured scenario in BENCHMARKS.md. | **Unverifiable** — looks like a made-up hero-banner comparison. |
| 32× memory reduction via Binary Quantization | BENCHMARKS.md: math table (1536 → 48 bytes per 384-dim vector). That's arithmetic, not a measured benchmark. No accuracy benchmark on a real retrieval task. | **Theoretical only** — "~97-99% accuracy" has no dataset or methodology cited. |

---

## Regression detection

- In CI: **no**. `.github/workflows/ci.yml` runs `cargo check`, `cargo clippy -D warnings`, `cargo test`, and a release build. **Zero references to `codescope-bench`.** `release.yml` builds artifacts only — no bench invocation before tagging.
- Historical numbers tracked: **no**. BENCHMARKS.md is a single-state document; every commit overwrites the previous numbers. No `benchmarks/results/YYYY-MM-DD-<sha>.json` directory, no CSV log, no time series.
- Release-gate enforcement: **no**. The agent card says *"Every tagged release updates BENCHMARKS.md with the current numbers"* — but BENCHMARKS.md was last touched 2026-04-12 (`e84e2a3`), while `v0.7.3`, `v0.7.4`, `v0.7.5`, `v0.7.6`, `v0.7.7` have all shipped since. **5 releases** with no bench refresh.
- Slow-PR flag: **no mechanism**. A 2× regression in `impact_analysis` latency would not be caught by CI, would not block a release, and would only surface if someone manually re-ran the bench and eyeballed the delta.
- Per-release number snapshots: **no**. The `--output results.json` flag exists but output is never committed.

Commits since last BENCHMARKS.md touch (2026-04-12): v0.7.3, v0.7.4, v0.7.5, v0.7.6, v0.7.7 — includes "round 2 tool consolidation (49 → 39 tools)", "CUDA support + LSP bridge", "schema migrations", "OpenTelemetry + scalable graph clustering". All of these touch hot paths; none were benched before ship.

---

## Red flags

1. **Token-savings methodology double-dips.** Same denominator (`total_bytes / 4`) is used for both "List all structs" and "Find largest functions" — implying the LLM reads the *entire* repo for either question. A realistic baseline is RAG top-K chunks, not full-source. This inflates savings to "~100%" which is cosmetically appealing but not defensible under scrutiny. BENCHMARKS.md methodology (line 230) is honest about it ("reading the top N source files"), but the scenarios that use `total_bytes / 4` do NOT follow that rule.
2. **"Traditional" baseline is our own fabrication.** Nobody has ever measured Cursor or Greptile producing a "List all structs" answer. The 148K / 99.6% hero number is an engineered comparison, not a head-to-head. Fine for marketing; dangerous if anyone asks "how did you measure that".
3. **N=1 timings published as authoritative.** Single-run latencies for sub-millisecond queries are dominated by OS scheduling noise. `find_function_exact` at 0.42 ms ± 0 stddev is a coin flip on a busy laptop.
4. **SurrealKV warming contradicts stated methodology.** "Cold-start (no caching)" is only true for query 1; the `discover_impact_target` pre-query plus serial execution means impact_d3 is warm. Either warm+cold should be documented separately, or the framing should change.
5. **README/BENCHMARKS.md drift is larger than BENCHMARKS.md/reality drift.** Launch copy (README) references a bench run that predates the current bench harness. The `impact_target` discovery machinery doesn't exist in the README's numbers at all — that feature was added in `6873af6` (2026-04-11), after the README indexing table was written.
6. **No grammar-version pinning.** Tree-sitter versions change parse throughput. Not recorded.
7. **DB size metric is taken AFTER inserts with no compaction.** SurrealKV uses LSM-style storage; the pre-compaction footprint is not the steady-state size. Publishing "63.8 MB" for tokio without noting this is dishonest.
8. **Worktree BENCHMARKS.md copies** exist at `.claude/worktrees/busy-lederberg/` and `.claude/worktrees/trusting-bohr/` — potential for stale numbers to re-enter main via a bad merge.
9. **`gin/BENCHMARKS.md` is a separate file** — unclear whether it's upstream-gin's doc or a stray codescope bench result. Ambiguity invites confusion.
10. **No competitor numbers are measured by us.** The "Competitive Comparison" section (lines 148–226) cites "blog posts and published benchmarks (April 2026)" but names no specific sources for any row. "Cursor: 15-20s end-to-end" has no citation; could be true, could be a Reddit anecdote.

---

## Action items

Priority ordered — cheapest to most valuable.

**P0 — truth-in-labeling (do this week, before HN launch):**

1. Fix the "7 projects 5 languages" claim. Either re-run bench on Express/Zod/FastAPI and add them to the current indexing table, or change README to "4 projects, 2 languages (extended corpus in token-savings section)".
2. Reconcile README indexing table (line 282–288) with BENCHMARKS.md. Delete the old numbers or regenerate. Currently they disagree by 4× on tokio.
3. Change README "99.6%" to a number that appears in BENCHMARKS.md, or add the 148K→542 scenario to the bench.
4. Change "sub-millisecond" → "single-digit-millisecond" in README line 7 to match the actual numbers (and what BENCHMARKS.md says in line 20).
5. Add `Hardware: <CPU>, <RAM>, <disk type>` and `Commit: <sha>` headers to BENCHMARKS.md. Bench run needs to emit these so they can't be faked.

**P1 — methodology hygiene (before v0.8):**

6. Add repetitions: `--repeat N` flag (default 10), report p50 / p95, drop the single-shot numbers.
7. Add explicit warmup: run each query once before timing. Delete the "cold-start" label or split into a separate `--cold` mode that restarts the DB handle between queries.
8. Fix token-savings double-dip: either cap "traditional" at top-K files (RAG-realistic) or clearly label the scenarios that assume full-source reads as "full-repo upper bound" (not "traditional").
9. Record grammar versions + Rust version in the bench output JSON.
10. Commit per-run `benchmarks/results/<date>-<sha>.json` so there's a historical series. Cheap, huge win for credibility.

**P2 — CI integration (before v0.9):**

11. Add a `bench` job in `.github/workflows/ci.yml` that runs on PR against a tiny fixture repo (e.g. the codescope repo itself). Fail the build if any query p50 regresses >20% vs main. This is literally the agent-card mandate and it does not exist.
12. On tag push, run full bench against the 7-repo corpus and commit the results JSON back to `benchmarks/results/`. Makes every release auditable.
13. Delete or consolidate the worktree copies of BENCHMARKS.md to prevent drift.

**P3 — honest comparisons (before any paid marketing):**

14. Either measure Cursor / Greptile / Bloop directly (scripted runs, published methodology) or move the competitive comparison section to "From published sources (not independently verified)". Current framing implies apples-to-apples measurement; it isn't.
15. Add a real BQ accuracy benchmark against a held-out retrieval task (e.g. code search queries with ground-truth answers). "~97-99%" with no dataset is not defensible.

---

## Summary

The bench harness itself (`crates/bench/src/main.rs`) is solid — dynamic impact target, realistic graph-traversal queries, JSON output plumbing. The **discipline around it is not.** No CI integration. No historical tracking. No repetitions. Hardware not documented. 5 releases since last BENCHMARKS.md refresh. README claims drift from BENCHMARKS.md which itself admits half its corpus is stale.

The fix is not expensive: (a) add the bench job to CI, (b) record hardware+commit in output, (c) commit per-run JSON, (d) sync README numbers with BENCHMARKS.md. A weekend's work, and then "numbers or didn't happen" actually holds.

**Relevant file paths:**
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\bench\src\main.rs`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\BENCHMARKS.md`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\README.md` (drift lines: 7, 33, 217, 221, 278, 282–290)
- `C:\Users\onurg\OneDrive\Documents\graph-rag\.github\workflows\ci.yml` (no bench step)
- `C:\Users\onurg\OneDrive\Documents\graph-rag\.github\workflows\release.yml` (no bench step)
- `C:\Users\onurg\OneDrive\Documents\graph-rag\.claude\worktrees\busy-lederberg\BENCHMARKS.md` (potential drift source)
- `C:\Users\onurg\OneDrive\Documents\graph-rag\.claude\worktrees\trusting-bohr\BENCHMARKS.md` (potential drift source)

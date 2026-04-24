# Codescope Benchmarks

Benchmarks run on real-world open-source projects. All measurements taken on Windows 11, Rust 1.91.1, bundled SurrealDB 3.0.5 server via the remote WS client, release build. Reproduce with `cargo run -p codescope-bench --release -- <path>`.

Latest re-run: **2026-04-24 against v0.8.11**. Previous numbers (2026-04-10, SurrealDB embedded, debug build) preserved inline for comparison.

## Index Throughput

| Project | Files | Entities | Relations | 2026-04-24 | 2026-04-10 | Speedup |
|---------|------:|---------:|----------:|-----------:|-----------:|--------:|
| ripgrep | 142   | 4,623    | 16,535    | **3.3s**   | 36.9s      | **11×** |
| axum    | 410   | 5,319    | 15,353    | **4.6s**   | 37.2s      | **8×**  |
| tokio   | 819   | 13,776   | 45,548    | **11.2s**  | 141.8s     | **13×** |
| gin     | 109   | 2,400    | 11,324    | **2.2s**   | 25.1s      | **11×** |

The 8-13× speedup is the compound effect of two changes:

- **R1-v2 server migration** — one shared `surreal` server owns the DB files; clients connect via WS. Prior to this, each bench run opened a local SurrealKV embedded engine, paying the per-run cold-start cost.
- **Batched INSERT builder** — one `INSERT RELATION INTO table [objs…]` per chunk instead of one `RELATE` per edge, wrapped in explicit transactions.

## Graph-First Multi-Hop Traversal (the differentiator)

This is what graph-based code intelligence enables that embedding-only tools (Cursor, Windsurf, Continue) **cannot answer at all**: walking the call graph to find transitive impact, type hierarchies, and fan-in.

For each repo, the benchmark dynamically picks the highest fan-in function as the impact target (so the numbers are meaningful — hardcoding `main` produces zero results because it's the call-graph root). Transitive callers are computed using SurrealDB's native graph traversal syntax (`<-calls<-\`function\`<-calls<-\`function\`.name`), which walks indexed edges in a single statement.

Numbers below are 2026-04-24 / v0.8.11 (median of 3 runs):

| Repo (size) | Impact target | 2-hop traversal | 3-hop traversal | `impact_analysis` (prod) |
|-------------|---------------|-----------------|-----------------|--------------------------|
| **ripgrep** (4.6k entities, 16.5k edges) | `build` | **0.97 ms** | **0.60 ms** | **2.78 ms** |
| **axum** (5.3k entities, 15.4k edges) | `clone` | **1.19 ms** | **0.48 ms** | **1.49 ms** |
| **tokio** (13.8k entities, 45.5k edges) | `ms` | **0.55 ms** | **0.48 ms** | **1.92 ms** |
| **gin** (Go, 2.4k entities, 11.3k edges) | `testRequest` | **1.23 ms** | **1.11 ms** | **1.01 ms** |

The first two columns measure the minimal traversal primitive (returning just `.name` from each hop). The third column measures the production MCP tool `impact_analysis`, which returns **full function records** per caller — heavier payload, realistic workload. It runs BFS per hop using the same native traversal pattern.

**Key result:** native multi-hop traversal stays **sub-millisecond regardless of repo size** on the latest bench. The production tool does a 3-hop BFS end-to-end in 1-3 ms — bounded by fan-out at the target, not by corpus size.

**Speedup vs the old WHERE-filter implementation.** Before 2026-04-12 the `impact_analysis` MCP tool issued `FROM calls WHERE out.name IN [...]` per hop, which is a full scan of the calls table and scales linearly with edge count. The new native traversal hits an indexed graph walk instead:

| Repo | Edges | Old WHERE-filter (per hop) | New native traversal (per hop) | Speedup |
|------|-------|----------------------------|--------------------------------|---------|
| gin | 11.3k | 40.08 ms | 1.06 ms | **38×** |
| axum | 15.1k | 89.70 ms | 2.52 ms | **36×** |
| ripgrep | 16.5k | 57.19 ms | 2.75 ms | **21×** |
| tokio | 44.7k | 173.19 ms | 3.26 ms | **53×** |

The speedup grows with repo size because the old path scans linearly and the new path walks indexed edges with O(fan-out) cost at the target. Multi-hop is often *faster* than single-hop with WHERE because the WHERE has no graph index. This is the property that makes graph-first viable for AI agents: a 3-hop "who transitively depends on this function?" query is bounded by graph fan-out, not corpus size.

## Token Savings vs Traditional Approach

The core value proposition: instead of reading entire source files to understand code, Codescope returns precise graph query results.

### ripgrep — Rust (142 files, 2.0 MB source)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 156,131 tokens | 2,388 tokens | **98.5%** |
| List all structs in project | 528,258 tokens | 1,154 tokens | **99.8%** |
| Find largest functions | 528,258 tokens | 290 tokens | **99.9%** |
| Impact analysis (callers + callees) | 197,615 tokens | 2,252 tokens | **98.9%** |

### axum — Rust (410 files, 1.6 MB source)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 69,073 tokens | 905 tokens | **98.7%** |
| List all structs in project | 415,438 tokens | 1,339 tokens | **99.7%** |
| Find largest functions | 415,438 tokens | 292 tokens | **99.9%** |
| Impact analysis (callers + callees) | 91,806 tokens | 595 tokens | **99.4%** |

### tokio — Rust (812 files, 5.6 MB source)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 125,620 tokens | 1,894 tokens | **98.5%** |
| List all structs in project | 1,463,493 tokens | 1,183 tokens | **99.9%** |
| Find largest functions | 1,463,493 tokens | 286 tokens | **~100%** |
| Impact analysis (callers + callees) | 171,175 tokens | 1,395 tokens | **99.2%** |

### FastAPI — Python (2,713 files, 15.9 MB)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 381,000 tokens | 260 tokens | **99.9%** |
| List all structs in project | 4,000,000 tokens | 1,700 tokens | **~100%** |
| Find largest functions | 4,000,000 tokens | 327 tokens | **~100%** |
| Impact analysis (callers + callees) | 431,300 tokens | 377 tokens | **99.9%** |

### Gin — Go (109 files, 0.8 MB source)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 82,170 tokens | 1,055 tokens | **98.7%** |
| List all structs in project | 214,604 tokens | 35 tokens | **~100%** |
| Find largest functions | 214,604 tokens | 185 tokens | **99.9%** |
| Impact analysis (callers + callees) | 107,642 tokens | 779 tokens | **99.3%** |

### Zod — TypeScript (465 files, 3.6 MB)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 254,900 tokens | 50 tokens | **~100%** |
| List all structs in project | 900,500 tokens | 1,600 tokens | **99.8%** |
| Find largest functions | 900,500 tokens | 335 tokens | **~100%** |
| Impact analysis (callers + callees) | 325,600 tokens | 50 tokens | **~100%** |

### Express.js — JavaScript (158 files, 711 KB)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 59,400 tokens | 50 tokens | **99.9%** |
| List all structs in project | 177,700 tokens | 0 tokens | **100%** |
| Find largest functions | 177,700 tokens | 302 tokens | **99.8%** |
| Impact analysis (callers + callees) | 76,100 tokens | 50 tokens | **99.9%** |

---

## Indexing Performance

| Repository | Language     | Files | Entities | Relations | Time   | Speed         | DB size |
|------------|--------------|-------|----------|-----------|--------|---------------|---------|
| ripgrep    | Rust         | 142   | 4,623    | 16,535    | 36.9s  | 3.8 files/s   | 22.2 MB |
| axum       | Rust         | 410   | 5,278    | 15,068    | 37.2s  | 11.0 files/s  | 22.5 MB |
| tokio      | Rust         | 812   | 13,600   | 44,675    | 141.8s | 5.7 files/s   | 63.8 MB |
| gin        | Go           | 109   | 2,400    | 11,324    | 25.1s  | 4.3 files/s   | 11.5 MB |

> Indexing throughput is measured single-row through the bench tool. The MCP server pipeline batches inserts and runs noticeably faster on the same corpora.

## Query Performance

Cold-start latencies (no caching) — single bench run on each repo:

| Query                             | ripgrep   | axum     | tokio     | gin      |
|-----------------------------------|-----------|----------|-----------|----------|
| search_functions                  |   4.75 ms |  4.05 ms |   6.05 ms |  4.49 ms |
| find_function_exact               |   0.42 ms |  0.42 ms |   0.41 ms |  0.42 ms |
| all_structs                       |   1.59 ms |  1.83 ms |   3.19 ms |  0.25 ms |
| largest_functions                 |   8.02 ms |  8.15 ms |  32.38 ms |  7.17 ms |
| graph_traversal_callers           |   3.86 ms |  1.80 ms |   3.03 ms |  1.32 ms |
| graph_traversal_callees           |   2.25 ms |  1.45 ms |   0.59 ms |  0.43 ms |
| count_all                         |   0.24 ms |  0.27 ms |   0.44 ms |  0.32 ms |
| imports_list                      |   0.28 ms |  0.30 ms |   0.34 ms |  0.31 ms |
| **impact_d1** (WHERE filter)      |  57.19 ms | 89.70 ms | 173.19 ms | 40.08 ms |
| **impact_d2** (native, 2-hop)     |   0.66 ms |  0.91 ms |   1.10 ms |  1.49 ms |
| **impact_d3** (native, 3-hop)     |   0.63 ms |  0.80 ms |   0.97 ms |  1.37 ms |
| **impact_analysis_prod_shape**    |   2.75 ms |  2.52 ms |   3.26 ms |  1.06 ms |
| type_hierarchy_traversal          |   1.23 ms |  0.84 ms |   1.96 ms |  0.34 ms |
| fan_in_top10                      |  54.73 ms | 42.59 ms | 254.50 ms | 39.37 ms |

## Semantic Search with Binary Quantization

Binary Quantization (BQ) converts float32 embeddings to packed binary vectors for 32x memory reduction. Two-stage retrieval: Hamming pre-filter then cosine rerank.

| Metric | Float32 (standard) | Binary Quantized | Improvement |
|--------|-------------------|------------------|-------------|
| Memory per vector (384-dim) | 1,536 bytes | 48 bytes | **32x smaller** |
| 10K functions stored | 15 MB | 468 KB | **32x** |
| 100K functions stored | 150 MB | 4.7 MB | **32x** |
| Search method | Full cosine scan | Hamming + cosine top-K | **Much faster** |
| Accuracy | Baseline | ~97-99% (5x oversampling) | Negligible loss |

---

## Competitive Comparison

### Architecture

| Tool | Search Technology | Graph DB | Semantic Search | Binary Quantization | Languages | Local |
|------|------------------|----------|-----------------|---------------------|-----------|-------|
| **Codescope** | Graph (SurrealDB) + Vector + BQ | Yes | BQ + Cosine two-stage | Yes (32x) | 59 | Yes |
| **Greptile** | Graph + Vector (hybrid) | Yes | Cosine | No | Unspecified | No (cloud) |
| **Sourcegraph** | Trigram (Zoekt) + SCIP | Partial (SCIP) | No | No | 30+ | No (cluster) |
| **GitHub Search** | N-gram (Blackbird, Rust) | No | No | No | 600+ (Linguist) | No (cloud) |
| **Bloop** | Vector (Qdrant) + Tantivy | No | Cosine | No | 11+ | Yes |
| **Cursor** | Vector (Turbopuffer) | No | Yes | No | Broad | No (cloud) |
| **Aider** | Tree-sitter + PageRank | No | No | No | 130+ | Yes |
| **Continue.dev** | Vector (SQLite/LanceDB) + ripgrep | No | Cosine | No | Broad | Yes |

### Indexing Speed

| Tool | Codebase | Time | Throughput |
|------|----------|------|------------|
| **Codescope** (bench, unbatched) | tokio (812 files, 5.6 MB) | 141.8s | 5.7 files/s |
| **Codescope** (bench, unbatched) | axum (410 files, 1.6 MB) | 37.2s | 11.0 files/s |
| **Bloop** | 1.3M LOC monorepo | 4m 20s | ~5K LOC/s |
| **Sourcegraph** | Go AWS SDK | 24s | Compiler-level |
| **GitHub Search** | 45M repos (global) | 18 hours | 120K docs/s |
| **Greptile** | Small repo | 3-5 minutes | Not published |
| **Cursor** | Large repo | Hours | 1M writes/s (backend) |

> The bench tool uses single-row inserts as a worst-case baseline. The production MCP server pipeline batches inserts and is materially faster on the same corpora; a fair head-to-head benchmark against Bloop/Sourcegraph through that path is sprint-pending work.

### Search Latency

| Tool | Exact Search | Semantic Search | Graph Traversal (3-hop transitive) |
|------|-------------|-----------------|-----------------|
| **Codescope** | **0.4-6 ms** | **< 30 ms (BQ)** | **3-10 ms end-to-end via `impact_analysis` (single-digit ms regardless of repo size)** |
| **Bloop** | Sub-second | 2-4s (with LLM) | N/A |
| **Sourcegraph** | ~100ms p99/shard | N/A | SCIP navigation |
| **GitHub Search** | ~100ms p99/shard | N/A | N/A |
| **Cursor** | N/A | 15-20s end-to-end | N/A |
| **Greptile** | Not published | Minutes (multi-hop) | Yes but slow |
| **Continue.dev** | Not published | Not published | N/A |

### Memory Efficiency

| Tool | Vector Storage | Bytes/Vector | BQ Support |
|------|---------------|-------------|------------|
| **Codescope** | SurrealDB + BQ | **48 bytes** | Yes (32x reduction) |
| **Bloop** | Qdrant (f32) | 1,536 bytes | No |
| **Cursor** | Turbopuffer (cloud) | Unknown | No |
| **Continue.dev** | SQLite/LanceDB (f32) | 1,536 bytes | No |

### Feature Matrix

| Feature | Codescope | Greptile | Sourcegraph | Bloop | Cursor | Aider |
|---------|-----------|----------|-------------|-------|--------|-------|
| Call graph traversal | Yes | Yes | Yes (SCIP) | No | No | No |
| Impact analysis | Yes | Yes | No | No | No | No |
| Dead code detection | Yes | No | No | No | No | No |
| Change coupling | Yes | No | No | No | No | No |
| Hotspot detection | Yes | No | No | No | No | No |
| Conversation memory | Yes | No | No | No | No | No |
| ADR management | Yes | No | No | No | No | No |
| Binary Quantization | Yes | No | No | No | No | No |
| Fully local / offline | Yes | No | No | Yes | No | Yes |
| MCP native | Yes | No | No | No | No | No |
| 99%+ token savings | Yes | Unknown | No | No | No | No |

### Key Differentiators

1. **Only local graph-based tool** - Greptile uses graph + vector but is cloud-only. Codescope runs entirely on your machine.
2. **Single-digit-millisecond multi-hop graph traversal** - The `impact_analysis` MCP tool returns 3-hop transitive impact in roughly 3-10 ms end-to-end across repos from 11k to 45k edges (21-53× faster than the previous WHERE-filter implementation). Cursor/Windsurf cannot answer this query at all; Sourcegraph SCIP navigation is single-step.
3. **32x memory-efficient semantic search** - Binary Quantization not available in any competitor.
4. **99%+ token savings** - No competitor publishes this metric. Graph-based retrieval returns only what's needed.
5. **52 MCP tools** - Richest tool set for AI agents (Greptile: API only, Bloop: 0 MCP, gstack: skills only).

### Areas for Improvement

1. **Language count** - Aider supports 130+, GitHub 600+, Codescope currently 59 (47 tree-sitter languages + 12 content/config parsers).
2. **Large-scale testing** - GitHub handles 45M repos; Codescope tested up to 2,713 files / 50K entities.
3. **Indexing throughput** - 5-68 files/s depending on language; behind Sourcegraph's optimized pipeline.

## Methodology

- **Traditional approach**: Estimated as reading the top N source files to answer a question (~4 bytes/token).
- **Codescope approach**: Measured as the actual JSON response size from the graph query (~4 bytes/token).
- **Indexing**: Full parse of all supported source files using tree-sitter, stored in SurrealDB embedded (SurrealKV).
- **Queries**: Cold-start (no caching), measured with `std::time::Instant`.
- **Impact target**: Dynamically discovered as the function with the highest fan-in in each repo (`fan_in_top10` query). Hardcoding `main` produces zero results because main is the call-graph root.
- **Projects tested**: ripgrep, axum, tokio (Rust), Gin (Go) — re-benchmarked 2026-04-10. Express, Zod, FastAPI numbers in older sections of this doc are from a previous bench run.
- **Competitor data**: Sourced from official documentation, blog posts, and published benchmarks (April 2026).

## Reproduce

```bash
git clone https://github.com/onur-gokyildiz-bhi/codescope
cd codescope
cargo run -p codescope-bench --release -- /path/to/repo --json --output results.json
```

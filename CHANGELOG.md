# Changelog

All notable changes to Codescope will be documented in this file.

## [Unreleased]

## [0.7.0] - 2026-04-13

Knowledge graph release. Codescope is no longer code-only — it now manages general knowledge (concepts, entities, sources, claims) alongside code entities in the same SurrealDB graph. Inspired by Karpathy's LLM Wiki pattern and claude-obsidian.

### Added
- **Knowledge graph schema:** `knowledge` table with title, content, kind (concept/entity/source/claim/contradiction/question), confidence, tags, embeddings. Edge tables: `supports`, `contradicts`, `related_to` for typed relationships between any entities (knowledge↔knowledge, knowledge↔code).
- **4 new MCP tools:** `knowledge_save` (create/update knowledge nodes), `knowledge_search` (full-text search with kind filter), `knowledge_link` (typed edges across knowledge and code entities), `knowledge_lint` (health check: orphans, contradictions, low-confidence clusters).
- **4 new skills:** `/wiki-ingest` (source ingestion pipeline: file/URL/image → extract entities/concepts/claims → file to graph → cross-reference with code), `/autoresearch` (autonomous research loop: web search → fetch → synthesize → file, based on Karpathy's pattern), `/wiki-query` (answer questions from knowledge graph, cite specific nodes), `/wiki-lint` (knowledge graph health report with severity levels).
- **Knowledge source watcher:** `.raw/` directory monitored for changes; creates a notification node when new/modified sources detected so the agent knows to run `/wiki-ingest`.
- `skills/autoresearch/references/program.md` — customizable research constraints (max rounds, source preferences, confidence scoring, stop conditions).

## [0.6.3] - 2026-04-13

### Fixed
- Graph entity duplication: MCP auto-index pipeline now canonicalizes + strips `\\?\` from base path (matching CLI init behavior). Same file no longer gets different qualified_names from different index paths.
- New `phase0_clean_stale()` wipes all code entities/edges before every re-index to prevent stale duplicates. Conversations, memory, and skills are preserved. Auto-index is now idempotent.

## [0.6.2] - 2026-04-13

### Added
- `codescope doctor` command — diagnoses 8 setup checks (binary, .mcp.json, rules, CLAUDE.md, database, stale processes, gitignore) with pass/fail + actionable fix instructions. `--fix` auto-repairs missing rules and gitignore entries.
- Claude Code Plugin packaging (`.claude-plugin/plugin.json` + `skills/` with references). Installable via `/plugin marketplace add onur-gokyildiz-bhi/codescope`.
- `skills/cs-query/references/SURREALQL.md` — full SurrealQL syntax guide for agents (tables, edges, traversal, anti-patterns, parameterized queries)
- `skills/codescope/references/TOOLS.md` — complete 52-tool reference with params

### Fixed
- `find_function` MCP tool: param renamed `query` → `name` (agents send `name: "X"`, not `query: "X"`)
- `install.sh`: kills running codescope processes (`pkill`) and removes old binaries (`rm -f`) before copy to avoid ETXTBSY ("text file busy") on Linux
- Clippy `useless_format` and `collapsible_else_if` in doctor.rs

## [0.6.1] - 2026-04-12

### Fixed
- Install scripts (`install.ps1`, `install.sh`) now detect existing install path and update in-place instead of installing to a different directory. Root cause of `/cs-update` appearing to do nothing.
- `install.ps1` stops running codescope processes before overwriting binaries (Windows file-lock issue)
- Added `.claude/rules/codescope-mandatory.md` (`alwaysApply: true`) so Claude Code is required to use codescope MCP tools instead of falling back to Read/Grep
- Added Intel macOS (`x86_64-apple-darwin`) build to release matrix. Uses `macos-13` runner (native x86_64). Previously Intel Mac users got a 404 on install.
- Tool count in install scripts updated 45 → 52

## [0.6.0] - 2026-04-12

Graph-first launch release. Headline change is a 21-53× speedup in the `impact_analysis` MCP tool from a rewrite to native SurrealDB inverse graph traversal, plus a complete refactor of the server and CLI into smaller modules, a sharpened graph-first positioning in README/BENCHMARKS.md, and the launch docs and asset drafts.

### Added
- Benchmark crate graph-first queries: `impact_d2`/`impact_d3` native multi-hop traversal, `type_hierarchy_traversal`, `fan_in_top10`, and `impact_analysis_prod_shape` (the exact query pattern the MCP tool uses)
- Benchmark tool dynamically discovers the highest fan-in function as the impact target (previously hardcoded `main`, which returns zero results because it is the call-graph root)
- `BenchmarkResults` JSON now exposes `impact_target`
- `[dev-dependencies]` section in `crates/mcp-server/Cargo.toml` with `surrealdb` `kv-mem` feature enabled so `graph_query_tests.rs` compiles standalone via `cargo test -p codescope-mcp` (previously only compiled under workspace-wide feature unification)
- `docs/quickstart.md` — 60-second walkthrough with expected output at every step
- `docs/troubleshooting.md` — top install, indexing, query, and MCP issues grouped and documented
- `docs/launch/` — HN post, tweet thread, and blog post drafts for the OSS launch
- CONTRIBUTING.md: new "Filing Issues", "Support Expectations", and "Scope Boundaries" sections for post-launch issue triage

### Changed
- **`impact_analysis` MCP tool rewritten to use SurrealDB native inverse graph traversal** (`SELECT <-calls<-\`function\` AS callers FROM \`function\` WHERE name IN [...]`) instead of the previous `FROM calls WHERE out.name IN [...]` WHERE-filter pattern. On real repos this is 21-53× faster per hop: 2.75 ms on ripgrep (was 57.19 ms), 2.52 ms on axum (was 89.70 ms), 3.26 ms on tokio (was 173.19 ms), 1.06 ms on gin (was 40.08 ms). End-to-end 3-hop impact drops from ~180-520 ms to under 10 ms across repos from 11k to 45k call edges. Per-hop latency is now bounded by graph fan-out at the target, not by corpus size. The BFS structure, deduplication, and "Direct Callers / Indirect Callers (N hops)" output format are preserved. A `MAX_CALLERS_PER_HOP` cap (100) replaces the old `LIMIT 100` in the query to guard against pathological fan-out.
- Sharpened 7 MCP tool descriptions with explicit disambiguation rules ("when to use X vs Y"): `search_functions`, `find_function`, `find_callers`, `find_callees`, `raw_query`, `impact_analysis`, `type_hierarchy`. Lifted structure from Leonie Monigatti's agentic search workshop (github.com/iamleonie/workshop-agentic-search).
- README rewritten with graph-first positioning, "Why graph-first?" section, and AI-native tool comparison table
- BENCHMARKS.md: new headline section "Graph-First Multi-Hop Traversal" with real sub-millisecond numbers across ripgrep, axum, tokio, and gin; refreshed indexing/query tables; speedup table showing old WHERE-filter vs new native traversal per repo; language count 35 → 59; MCP tool count 45 → 52
- Phase 1-4 refactor landed: `crates/mcp-server/src/server.rs` split 4537 → 166 lines; `crates/cli/src/main.rs` split 1293 → 131 lines; 52 MCP tools split into 16 sub-modules under `crates/mcp-server/src/tools/`; `IndexingPipeline` orchestrator extracted from lib.rs
- Daemon and stdio modes unified via shared `DaemonState`
- NLP `ask()` engine rewritten with intent + entity extraction (12 new unit tests)
- Embed pipeline now batches a single round-trip per 100 functions (was N+1 UPDATEs)
- `EmbedStats` regression test added (was returning hardcoded zeros)

### Fixed
- `GraphQuery::raw_query` no longer silently swallows parse errors from the first statement. Previously any `take(0)` error was treated as "no more statements", so a query with a SurrealQL syntax error returned an empty array instead of surfacing the parse error. This bug was what enabled the bogus "6.4 million× speedup" claim in a previous session's bench commit — a parse error reported as a 0.05 ms successful query.
- Benchmark chained graph-traversal syntax: hops must chain directly (`<-calls<-\`function\`<-calls<-\`function\`.name`), not with dots between hops. The dotted form was the parse error silently swallowed above.
- Clippy `needless_range_loop` warning in `crates/core/src/graph/builder.rs` (`for i in 0..chunk.len()` → `for (i, rel) in chunk.iter().enumerate()`) — the root cause of the CI `Check` job failing on every push for the last 30+ runs.
- Pre-existing `cargo fmt` violations across 25 files and 96 call sites — the root cause of the CI `Rustfmt` job failing on every push for the last 30+ runs.

## [0.5.0] - 2026-04-07

### Added
- Dart function/method extraction
- Protobuf parser
- .env file parser
- Gradle parser
- Circular dependency and duplicate code detection
- API changelog tool
- Export to Obsidian vault (`export_obsidian` tool with wikilinks)
- Tiered memory, decision rationale, and scoped memory
- Virtual dispatch heuristic for C#/Java
- Auto-embed after indexing
- Git history auto-sync
- Code smell detection tool
- Custom lint rules engine
- CI: `cargo audit` security scanning via `rustsec/audit-check`
- CI: Strict clippy and test enforcement (no more `continue-on-error`)
- `clippy.toml` and `deny.toml` for lint and supply chain security config
- `Dockerfile` with multi-stage build for containerized deployment
- Pre-commit hook config for local quality gates
- SHA256 checksums for release binaries

### Changed
- All dependencies upgraded (SurrealDB 3.0, rmcp 1.3, tree-sitter 0.25)
- Impact analysis BFS rewrite

### Fixed
- 6 C# evaluation bug fixes
- `.mcp.json` hardcoded user paths — now portable across machines
- All MCP config templates standardized to use `codescope-mcp` binary
- 3 `unwrap()` calls in production code replaced with safe alternatives
- All clippy warnings resolved (was 39, now 0)
- Hardcoded test paths replaced with `CODESCOPE_TEST_JSONL_DIR` env var

## [0.4.0] - 2025-03-15

### Added
- 3D interactive web UI with force-directed graph visualization
- Type hierarchy analysis (`type_hierarchy` tool)
- Skill/knowledge graph support with wikilink navigation
- Conversation history panel with date filter and search
- Auto project insights after indexing
- File tree, hotspots, skills, timeline, minimap in web UI
- 52 MCP tools (up from 36)

### Changed
- Unified 3 binaries into single `codescope` executable (kept separate binaries for backward compat)
- Faster indexing with parallel file collection

### Fixed
- Repo name derived from target path instead of CWD
- False-positive CLAUDE.md check in insights
- File tree nested entity array flattening

## [0.3.0] - 2025-02-20

### Added
- 35 language support (up from 10)
- HTTP endpoint linking and caller tracing
- Conversation memory: indexes Claude Code session transcripts
- Binary quantization for 32x memory-efficient semantic search
- Symbol rename and safe delete operations
- Dead code detection
- File watcher for live re-indexing
- Progressive disclosure in search results
- One-line install scripts (`install.ps1`, `install.sh`)
- `codescope init` command for zero-config setup
- 5 agent configs (Claude Code, Cursor, Zed, Codex CLI, Gemini CLI)

### Changed
- Switched to SurrealKV backend (from RocksDB)
- Optimized binary size with release profile tuning

### Fixed
- Call graph resolution for same-file callees
- DB lock limitation with SurrealKV migration
- 19 performance and memory issues across query engine

## [0.2.0] - 2025-01-30

### Added
- Team patterns and contributor mapping
- Edit preflight checks
- ADR (Architecture Decision Records) management
- Community detection in code graphs
- Memory and visualization tools
- Daemon mode (SSE server for multi-project)

## [0.1.0] - 2025-01-15

### Added
- Initial release
- Code parsing with tree-sitter (10 languages)
- SurrealDB knowledge graph storage
- MCP server for Claude Code integration
- Semantic search with FastEmbed
- `find_callers`, `find_callees`, `impact_analysis`
- `context_bundle`, `explore`, `search_functions`
- Git history sync and file churn analysis
- Benchmark suite

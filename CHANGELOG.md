# Changelog

All notable changes to Codescope will be documented in this file.

## [Unreleased]

## [0.8.8] - 2026-04-24

Testing + docs. No binary-behaviour changes.

### Added

- **GraphQuery smoke-test matrix** — 10 new integration tests
  covering `stats`, `raw_query` (happy + parse-error cases),
  `file_context`, `type_hierarchy` (known + unknown class),
  `find_all_references`, `safe_delete_check`,
  `find_unused_symbols`, and `find_duplicate_functions`. Brings
  the GraphQuery test count from 6 → 26 and catches schema /
  syntax regressions without retesting tool-handler correctness.
- **GSD v2 integration guide** (`docs/integrations/gsd.md`).
  The doc used to assume GSD v1 (prompt framework). v2
  (`gsd-pi` CLI built on the Pi SDK) is now the recommended
  path; v1 stays documented for users who still run it.
  Covers the separate-graphs model (GSD graph = plan, codescope
  graph = code) so users don't expect one to subsume the other.

## [0.8.7] - 2026-04-24

Parser coverage + dedup regression tests. Closes two gaps that
the recent status review surfaced.

### Added

- **JS / TS / TSX arrow-function extraction.** The walker now
  handles `variable_declarator` nodes whose value is an
  `arrow_function` or `function_expression`, pulling the
  identifier from the declarator's `name` field. Previously
  only `function_declaration` was caught, so React / SolidJS
  components and hooks written in the `const Foo = () => {}`
  style never surfaced in the graph. On this repo's
  frontend: `InsightPage.tsx` jumped from 7 → 19 functions
  as the local helpers and small inner closures became
  visible.
- **Dedup regression tests.** Four new integration tests
  against the in-memory SurrealDB fixture seed a duplicate
  `calls` edge and assert every traversal query
  (`find_callers`, `find_callees`, `backlinks`, `explore`)
  collapses it. Locks the v0.8.3 / v0.8.4 fixes against
  regression.

## [0.8.6] - 2026-04-22

CI hotfix — 0.8.3 / 0.8.4 / 0.8.5 all had their release
binaries blocked by a Linux-only clippy error. No user-visible
changes beyond the ones those tags already announced.

### Fixed

- **Linux clippy: unused `CommandExt` import in sandbox.rs.**
  `tokio::process::Command::pre_exec` is an inherent method on
  Unix; the extra `use std::os::unix::process::CommandExt;`
  imported a trait nothing called. Windows builds didn't see it
  because the `#[cfg(unix)]` block was dead code there. CI's
  `-D warnings` killed the release build on every tag since
  0.8.3. Removed the import; local Windows + CI Linux now
  agree.

## [0.8.5] - 2026-04-22

UI follow-up to 0.8.4 — the per-tool savings breakdown now
shows up in the web Insight page too, not just the CLI.

### Added

- **"Savings by tool" bar chart in the web UI.** `/api/insight`
  returns `gain.per_tool` (sorted by total saved) and the
  InsightPage renders a horizontal bar chart mirroring the
  `codescope gain` CLI table. Legacy unattributed calls get a
  muted row so the headline "Tokens saved" number always
  reconciles with the breakdown.

## [0.8.4] - 2026-04-22

Observability + bug-fix pass. `codescope gain` now tells you
where the savings are actually coming from.

### Added

- **Per-tool `codescope gain` breakdown.** Every MCP handler
  now attributes its call to the tool name. The CLI shows a
  sorted table — tool × calls × per-call estimate × total
  saved — instead of a flat "N calls × 2500". Each tool has
  its own token-savings estimate (impact_analysis ≈ 50 000,
  find_callers ≈ 6 000, graph_stats ≈ 500). Answers the
  "where is the value coming from" question directly.
- **Windows JobObject process-tree kill for `sandbox_run`.**
  Completes the cross-platform story started in 0.8.3. A
  job with `KILL_ON_JOB_CLOSE` automatically reaps every
  subprocess the snippet spawns — the Windows equivalent
  of setsid + `kill(-pgid)` on Unix.

### Fixed

- **GROUP BY dedup swept across all edge-traversal queries.**
  Same duplicate-row issue `find_callers` had in 0.8.3 also
  hit `explore` (callers + callees), `backlinks` (calls +
  wikilinks), and `impact_analysis` (implements). Each now
  groups on identifying fields so legacy DBs collapse
  duplicates without a required `codescope repair`.
- **Orphan call resolver uses edge `raw_callee` metadata.**
  The parser had been writing the exact callee identifier on
  every `calls` edge; the resolver was ignoring it and
  splitting the sanitized synthetic target ID on underscores.
  That silently mangled multi-word function names
  (`default_limit` → split to `limit`, never matches). Now
  reads `raw_callee` first and falls back to the old
  heuristic only for legacy edges missing the metadata.

## [0.8.3] - 2026-04-21

Bug-fix + hardening pass. `find_callers` duplicate rows were the
user-visible report; tracking them down surfaced a second bug
in the orphan resolver that had been silently eating ~97% of
cross-file call edges.

### Fixed

- **`find_callers` returned the same caller N times.** Two
  root causes:
  1. `delete_file_entities` and `clear_repo` dropped entity
     rows but left their edges. SurrealDB `TYPE RELATION`
     edges don't cascade on entity delete, so re-indexing
     a repo four times left four copies of every `calls`
     edge. Both functions now delete edges before entities,
     keyed on `in.file_path` / `out.file_path` (file scope)
     or `in.repo` / `out.repo` (repo scope). Verified
     empirically: `default_limit` edge count 60 → drops
     cleanly on `--clean` re-index.
  2. `resolve_call_targets` used `WHERE out.name IS NULL`
     to find dangling `calls.out` references. SurrealDB
     3.0.5 treats a missing record-link dereference as
     `NONE`, not `NULL` — so the predicate matched nothing
     and almost no cross-file calls got resolved. Same
     bug for the `meta::tb(out)` qualifier. Flipped to
     `IS NONE`. Verified: 0 orphans matched pre-fix → 21,023
     matched on this repo post-fix.
- **Defensive `GROUP BY` on `find_callers` / `find_callees`
  queries.** Collapses duplicate rows on legacy DBs indexed
  before this release, so the fix takes effect without a
  required `codescope repair`.

### Added / Hardened

- **`sandbox_run` kills the whole process tree on timeout.**
  Previous implementation moved the child into the timeout
  future and relied on drop reaping it. That only killed the
  direct child — anything the snippet spawned (pip shelling
  to compilers, node forking workers, bash pipelines) got
  reparented to `init` and kept running past timeout. Now
  the Unix path `setsid()`s the child and signals the group
  (SIGTERM → 200 ms grace → SIGKILL); Windows still hits only
  the direct child via `start_kill()` until JobObject
  support lands. `kill_on_drop(true)` as the safety net on
  both platforms.
- **`fetch_and_index` prefers `<article>` / `<main>` /
  `.content` before handing the body to `html2text`.** Docs
  and blogs almost always wrap real content in one of those;
  feeding the whole document to the extractor pulled in
  nav / footer / cookie-banner prose that diluted BM25
  scoring. Falls back to the full body when none match
  (SPAs, pages without semantic tags). String-based scan —
  a DOM parse would pull `html5ever` onto the hot path for
  a best-effort heuristic.

## [0.8.2] - 2026-04-21

Absorb context-mode + RTK into the codescope binary. You no longer
need two extra daemons to cover generic tool output and shell
output — codescope handles both.

### Added

- **CMX absorb — indexed content store.**
  New per-repo `indexed_content` table with BM25 FULLTEXT indexes.
  CLI: `codescope ingest <url|path>`, `codescope search-content <q>`,
  `codescope purge-indexed`. MCP: `fetch_and_index`, `index_content`,
  `search_indexed`. HTML is rendered to plain text via `html2text`.
  Distinct from the `knowledge` table on purpose — this one is dumb
  text (web fetches, log dumps, doc snapshots) so `knowledge_search`
  doesn't get polluted.
- **CMX absorb — sandbox_run MCP tool.**
  Runs a short python / node / bash snippet in a subprocess and
  returns `{stdout, stderr, exit_code, timed_out}`. 10 s default
  timeout (60 s cap), 16 KB per-stream output cap, credential env
  vars stripped before spawn.
- **RTK absorb — `codescope exec <cmd>` output compressors.**
  Wraps `cargo`, `pytest`, `npm` / `pnpm` / `yarn`, `tsc`,
  `docker`, plus `git` / `ls` / `cat` / `head` / `tail` / `grep`.
  Keeps warnings + errors + summary; drops the noise. ~87 %
  reduction measured on a cold `cargo check`. `--full` opts out.
- **Hook enhancement — suggest `codescope exec` wrapping.**
  `codescope-bash-suggest` (PreToolUse for Claude Code) now
  recognises compressor-eligible commands and nudges the model to
  prefix with `codescope exec` instead of only pointing at MCP
  tools.
- **`codescope init` auto-starts the surreal server** if it isn't
  running. Removes one step from the first-time setup — no more
  "I ran init and it errored" reports.
- **Unit-test coverage** for the new compressors (split_full_flag,
  keep_head_tail, pytest dot-progress detector, tsc dedup) and
  sandbox language-dispatch.

### Fixed

- **BM25 score always returned 0.0.** SurrealDB 3.0.5 quirk —
  `search::score()` returns zero even with a valid FULLTEXT BM25
  index and matching WHERE predicate. `search_content` now keeps
  the FULLTEXT predicate for fast filtering and computes a
  deterministic "title match (2.0) + body match (1.0)" score
  manually. Substring fallback (`string::contains`) catches
  analyzer-rejected tokens (URL fragments, punctuation).

### CI

- **E2E smoke timeout bumped 5 → 15 min.** Cold `cargo build -p
  codescope` exceeded 5 min after the html2text + tokio::process
  additions landed, so the job was timing out before the tests
  ran.
- **E2E smoke builds the frontend before the binary.** `codescope-
  web` now embeds `frontend/dist/*` at compile time via
  `include_dir!` + `include_str!` (the 0.8.1 hotfix); the e2e job
  still built the binary without the prior `npm run build`, so
  compilation failed on "No such file or directory".

## [0.8.1] - 2026-04-20

Hotfix — web UI rendered as a blank white page on every
released binary.

### Fixed

- **Frontend assets embedded at compile time via `include_dir`.**
  `serve_asset` used to read JS/CSS files from
  `$CARGO_MANIFEST_DIR/frontend/dist/assets/` at runtime. That
  path is resolved when the binary is built, so any installed
  archive (release tarball, Homebrew, `codescope install`,
  `codescope upgrade`) pointed at a directory on the CI runner
  that didn't exist on the user's machine. Result: every asset
  404'd and the root route served an index.html referencing
  JS that never loaded — white screen. Now the whole `dist/
  assets/` tree is baked into the binary via
  `include_dir::include_dir!(…)`; the legacy on-disk path
  stays as a dev fallback so `cargo run` still picks up live
  vite rebuilds without a recompile.
- **Asset `Content-Type` mapping** expands to fonts
  (`woff` / `woff2`) and SVGs, which previously came back as
  `application/octet-stream`. Most browsers tolerated it;
  older Safari did not.

## [0.8.0] - 2026-04-20

Substantial refactor + feature release across three branches.
Rewrites the storage layer onto a bundled SurrealDB server,
adds a narrated arc-tour UI, multi-agent distribution, and an
observability layer shared with RTK + context-mode.

### Architecture (R1–R8)

- **R1-v2 remote SurrealDB client.** Every crate talks to a
  bundled `surreal` server on `127.0.0.1:8077` via a single
  `DbHandle = Surreal<Any>`. Eliminates the exclusive file lock
  SurrealKV imposed across CLI + MCP stdio + web daemon + LSP;
  the server handles concurrency now, not us. Configurable via
  `CODESCOPE_DB_URL` / `CODESCOPE_DB_NS` / `CODESCOPE_DB_USER` /
  `CODESCOPE_DB_PASS`.
- **R2 structured error contract.** Every non-2xx body and tool
  error string now serialises to
  `{ok:false, error:{code, message, hint}}` with a narrow code
  taxonomy (`db_unreachable`, `db_version_drift`, `db_corrupt`,
  `repo_not_found`, `invalid_input`, `timeout`, `internal`,
  `index_not_ready`, `no_project`). Applies across web, MCP, and
  CLI stderr.
- **R3 end-to-end smoke crate.** `crates/e2e/` ships a
  `TestServer` fixture that spawns an ephemeral in-memory
  surreal per test. `smoke_server`, `smoke_multiproject`, and
  `smoke_cli` suites verify the invariants the refactor exists
  to guarantee. Dedicated `e2e` CI job, 5-min timeout.
- **R4 supervisor.** `codescope start` / `stop` / `status`
  manage the surreal binary idempotently via
  `~/.codescope/surreal.json`. `codescope doctor` grows a
  supervisor-state check. Windows uses `DETACHED_PROCESS` +
  `CREATE_NEW_PROCESS_GROUP`; Unix orphans the child.
- **R5 `/mcp/{repo}` per-repo MCP routing.** HTTP daemon pre-
  discovers every DB on the server at startup and mounts one
  `StreamableHttpService` per repo. Tool calls against
  `/mcp/<repo>` bypass `init_project` — the session resolves
  the repo lazily on first `ctx()`.
- **R6 `codescope repair`.** `--repo X [--reindex PATH]`
  drops the repo's SurrealDB database and optionally re-runs
  the indexer, without bouncing the server or touching other
  repos. Interactive confirmation unless `--yes`.
- **R7 `codescope migrate-to-server`.** Legacy per-repo
  SurrealKV dirs migrate via a spawned
  `surreal export` → `surreal import` pipeline per repo. Temp
  server on a free port backs the source; a small backtick-
  reserved-word pass fixes an upstream surreal 3.0.5 bug where
  exported `function:id` didn't parse back on import. Verified
  lossless on alice-project (2737 entities + 93146 relations,
  zero drift).
- **R8 release archives bundle the surreal binary.** Each
  target triple in `release.yml` now ships a pinned `surreal`
  next to the four codescope binaries. Install scripts drop it
  at `~/.codescope/bin/` where the R4 supervisor looks first.

### Phase 3 Dream — narrated tours through the knowledge graph

- **`/dream` view** (sixth top-bar tab, shortcut `6`). Arcs
  are tag-based clusters of knowledge entries: decisions,
  problems, solutions, concepts, claims.
- **3D tour graph.** Scenes become octahedron nodes connected
  by a glowing "storyline" path. Camera flies between scenes on
  autoplay (6 s/scene) or manual skip. Click any node to jump.
- **Template narration** of each scene, first-person memoir
  voice. Typographic quotes around titles; first-sentence
  extraction after stripping markdown scaffolding.
- **Markdown export** — downloads the active arc as a
  standalone `dream-<tag>.md` with H2 per scene + collapsed
  content blocks.

### Dream refinement (A / B / C / D / E)

- **Dream-A auto-tag suggestion.** Scans for entries without
  topical tags and proposes the top-3 arcs they could belong to
  via Jaccard overlap + tag-name-in-title bonus. One-click
  accept writes the tag.
- **Dream-B duplicate flag.** Scenes with ≥70% content
  similarity inside an arc get a magenta outline on the rail
  and a badge on the card. Click jumps to the anchor scene.
- **Dream-C cross-repo patterns.** Walks every repo on the
  server; tags that appear in ≥2 projects bubble up as "same
  pattern in N repos" cards.
- **Dream-D LLM narration (Ollama).** Opt-in via
  `CODESCOPE_LLM_URL` + `CODESCOPE_LLM_MODEL`. One batched
  completion per arc; cached by `hash(arc_id + scene_ids)`.
  First fetch returns template narration and kicks off a
  background generation; next fetch uses the LLM output.
- **Dream-E rule-based edge proposals.** Offers `solves_for` /
  `decided_about` / `related_to` RELATEs between scenes based
  on kind pair + Jaccard score. Accepted edges write to
  SurrealDB.

### Distribution + multi-agent (CMX / RTK)

- **Multi-agent `codescope init --agent <name>`.** Nine
  platforms: claude-code, cursor, gemini-cli, vscode-copilot,
  codex, windsurf, kiro, cline, antigravity. Each gets its
  config at the upstream-documented path, in the upstream-
  documented format (JSON for most, TOML for Codex).
- **Homebrew tap.** `brew install onur-gokyildiz-bhi/codescope/codescope`
  via `Formula/codescope.rb`.
- **Claude Code plugin marketplace.**
  `/plugin marketplace add onur-gokyildiz-bhi/codescope`.
- **`codescope upgrade`.** In-place self-update from the
  latest GitHub release for the host target triple.
- **Bash-suggest hook (RTK-03).** `codescope hook install`
  drops a PreToolUse script nudging the model toward codescope
  MCP tools when it's about to `cat` / `grep` / `find` into a
  codebase. `CODESCOPE_HOOK_BLOCK=1` makes matches hard-fail.

### Observability (CMX-01 / 01b / 02)

- **`codescope gain`.** Cumulative token-savings counter —
  atomic increment in `ctx()`, 30-s flush to
  `~/.codescope/gain.json`. Prints total calls + estimated
  tokens saved.
- **`codescope insight`.** Per-call histogram by repo + hour,
  unicode bar + sparkline. Seventh top-bar view with a live-
  refreshing web dashboard.
- **`codescope session` + CMX-02.** Every event in the log now
  carries `kind` (`tool_call` / `file_edit` / `error`),
  `session_id` (PID + boot-ns, stable per MCP process), and
  optional `detail`. The watcher emits `file_edit` events, so
  a session recap shows both what you asked codescope and what
  you changed on disk. `/api/session/recent` + web timeline.

### Response compaction (RTK-06)

- **`CODESCOPE_COMPACT=1`** strips embedding arrays, content
  hashes, and model metadata from every `raw_query` result.
  `CODESCOPE_COMPACT=ultra` additionally drops timestamps,
  `qualified_name`, and collapses absolute paths to their last
  three segments. 30–50% reduction on top of the structured
  graph queries.

### Routing hygiene (CMX-04, CMX-06, CMX-08)

- **Stop writing `.claude/rules/codescope-mandatory.md` to
  user repos.** Rules are now injected at MCP initialize via
  `ServerInfo.instructions` — no file surprises in a user's
  git.
- **`docs/llms.txt` + `docs/llms-full.txt`.** LLM-facing
  concise index + full MCP tool reference with parameter
  shapes and the R2 error-code taxonomy.
- **README "Think in Code" section.** Positions codescope
  tools as structured queries the LLM programs, not data it
  processes. Three-layer stack callout with RTK + context-mode.

### Internal

- **New crates/core modules:** `compact`, `gain`, `insight`,
  `llm` — each self-contained, env-gated where relevant.
- **`crates/web/src/dream.rs`** — now ~1000 lines covering the
  arc / scene / suggestion / pattern / edge-proposal / LLM
  narration-cache pipeline.
- **`workspace.metadata.surreal`** — single source of truth for
  the surreal binary version pin (matches `release.yml` env).

## [0.7.10] - 2026-04-17

4.5× indexing speedup and large-project MCP reliability fixes.

### Performance
- **INSERT RELATION bulk path:** replace multi-statement `RELATE` with SurrealDB's documented `INSERT RELATION INTO edge [array]` wrapped in an explicit `BEGIN TRANSACTION ... COMMIT TRANSACTION`. Measured 2.9× on the insert phase for edge-heavy workloads. Falls back to per-edge `RELATE` if the bulk path fails so nothing is silently dropped.
- **Explicit transactions on entity UPSERTs:** wrap each UPSERT batch in `BEGIN/COMMIT`. SurrealDB's default is per-statement transactions, so without an explicit txn boundary every UPSERT pays its own commit cost. Additional 1.8× on top of the relations fix. Combined total: **4.5× speedup** (28.9s → 6.4s on graph-rag, 237 files / 13k relations). 10k-file extrapolation: 16 min → 4.5 min.
- **Corpus-wide bulk insert:** collapse per-file `insert_entities` / `insert_relations` calls in both the CLI `index` path and the MCP server's `phase2_insert_results`. The flat-Vec collection is a prerequisite for the bulk query forms above and also unifies the conversation + memory auto-index paths.
- **In-memory call resolution:** replace the SurrealQL `FOR $o IN $orphans` loop in `resolve_call_targets` with a two-bulk-SELECT + Rust HashMap match + batched `DELETE + RELATE`. Scales linearly instead of O(N²) inside the embedded engine. Matters most on very large graphs.
- **Configurable query scale:** new env vars `CODESCOPE_QUERY_TIMEOUT_SECS` (default 60, was hardcoded 30) and `CODESCOPE_QUERY_DEFAULT_LIMIT` (default 500). Default `LIMIT` added to ~15 previously unbounded `SELECT` queries in `explore`, `file_context`, `type_hierarchy`, `find_all_references`, `find_http_calls`, `find_endpoint_callers`, `backlinks`, and friends. `count()` queries and `raw_query` intentionally untouched.
- **Configurable parser file size limit:** 512 KB → 2 MB default (covers generated CUDA kernels, LLVM IR dumps, kernel registries). Override with `CODESCOPE_MAX_FILE_SIZE_BYTES`. Skipped files now emit a `tracing::warn!` with path, size, and limit instead of disappearing silently.

### Fixed
- **MCP reconnect on large projects:** auto-index is now background by default with a readiness gate on every tool handler. Previously blocking-by-default auto-index could exceed the MCP client's handshake timeout on large repos, causing `Failed to reconnect to codescope` after install. Tools now return `{"status":"indexing","progress":"347/2100 files","elapsed_secs":12}` while the build is in flight instead of empty arrays. Opt into the old blocking behavior with `--auto-index-blocking` or `CODESCOPE_AUTO_INDEX_BLOCKING=1` for one-off CLI runs on small repos.
- **Transactional phase0:** parse-before-wipe staging — a mid-parse failure no longer leaves the DB silently empty. If phase1 fails the state transitions to `Failed` with the real error (surfaced via `index_status`), not to `Idle` with zero records.
- **Surfaced parse errors:** `IndexState` now collects read/parse errors into a capped `Vec<(PathBuf, String)>` (1000 entries) and exposes the count via `index_status`. Previous `.filter_map(.ok())` / `.unwrap_or_default()` calls silently swallowed every failure.
- **File-based logging in stdio MCP mode:** tracing output writes to `$XDG_STATE_HOME/codescope/logs/mcp-{pid}-{ts}.log` on Linux/macOS, `%LOCALAPPDATA%\codescope\logs\...` on Windows. stdout stays JSON-RPC-clean. A one-line stderr notice records the log path so the host forwards it if possible.
- **`index_status` MCP tool:** new tool returning `{state, files_total, files_indexed, files_skipped, errors_count, running_time_secs}`. Lets agents distinguish "indexing" from "no data" without guessing.

### Added
- **`codescope` agent + related skills:** new project-local agents (parser-specialist, graph-architect, release-captain, lsp-bridge-lead, mcp-tool-curator, knowledge-librarian, web-ui-designer, bench-champion, doctor-diagnostician, project-maintainer, context-optimizer) and skills (codescope, demo, lint-all, mcp-test, ship, status, tool-audit, cs-ask / cs-callers / cs-file / cs-impact / cs-search / cs-stats). Distributes ownership of the major subsystems to named agents so Ada doesn't become the bottleneck.
- **`/mcp-test` skill:** end-to-end MCP server verification — spawn stdio server, list tools, invoke each one, verify response shape.

### Internal
- CI clippy clean against Rust 1.95 (new lints: `manual_checked_ops`, `unnecessary_sort_by`, `collapsible_match`, etc.). RocksDB engine was evaluated and measured 1.5× SLOWER than SurrealKv on our edge-heavy workload — keeping SurrealKv. Entity `INSERT INTO [...] ON DUPLICATE KEY UPDATE` was attempted but tripped the unique `fn_qname` index when same-name fns in different impl blocks collapsed to the same sanitize_id; reverted to UPSERT + BEGIN/COMMIT. Both dead-ends filed in the knowledge graph so the experiment isn't re-run.

## [0.7.7] - 2026-04-14

OpenTelemetry observability and scalable 3D graph clustering for large repos.

### Features
- **OpenTelemetry observability:** new telemetry module (`crates/mcp-server/src/telemetry.rs`) with OTLP export. Activates only when `CODESCOPE_OTLP_ENDPOINT` env var is set (zero overhead otherwise). Tested with Jaeger, Grafana Tempo, and Honeycomb. `impact_analysis` instrumented with `#[tracing::instrument]`. Uses `opentelemetry` 0.27 / `opentelemetry-otlp` 0.27 `SpanExporter` API. README gains an Observability section.
- **Scalable 3D graph clustering:** backend `api_graph` gains `cluster_mode={none,folder,auto}` and `max_nodes` params. `apply_folder_clustering` groups nodes by top-2 path segments, replaces >10-member folders with a single super-node, and aggregates cross-folder edges. Frontend renders cluster nodes at 3x size in distinct purple. Default `cluster_mode=auto` triggers when the graph has more than 500 nodes — solves the hairball problem on 100K+ line repos.

### Internal
- Launch-grade README rewrite; positioning fix — codescope is a context layer, not an editor.

## [0.7.6] - 2026-04-14

Schema migrations, cross-project shared knowledge, and diff-aware PR review.

### Features
- **Schema migration system:** `SCHEMA_VERSION` constant + `meta` table tracking DB version. Idempotent `migrate_to_current()` runs on every DB connect (auto-upgrade). New `codescope migrate` CLI command for explicit migration. Future schema changes no longer require users to `rm -rf` and re-index. Infrastructure lives in `crates/core/src/graph/migrations.rs`, covered by 4 unit tests (fresh DB, upgrade, idempotency, roundtrip).
- **Cross-project shared knowledge:** global knowledge DB at `~/.codescope/db/_global/`. `knowledge` tool gains `scope` param (`project` default | `global` | `both`). `save` writes to the global DB when `scope=global`; `search` with `scope=both` unions and dedupes across project + global DBs; `link` edges live in the same DB as their nodes. Lazy global DB connection — no overhead if unused.
- **Diff-aware PR review:** new `codescope review <target>` where target is a git ref range, commit SHA, or `.diff` file. Parses unified diff, maps changed lines to graph entities, and runs impact analysis per changed function. `--max-callers` (default 10) and `--coverage` (flags functions with no test file references) flags. Markdown output on stdout (pipe to `gh pr comment`). No new dependencies — shells out to `git`, avoids `git2`.

## [0.7.5] - 2026-04-14

CUDA semantic support, LSP bridge, and tool consolidation round 3 (39 → 32).

### Features
- **CUDA semantic support:** detects `__global__`, `__device__`, `__host__` qualifiers on functions. Kernel launch sites (`kernel<<<grid, block>>>(args)`) emit a `calls` edge with `metadata.kind='kernel_launch'` and launch config captured as metadata. New `cuda_qualifier` field on the `Function` entity, surfaced in search results. File extensions: `.cu`, `.cuh`, `.cu.inc`, `.cuh.inc`.
- **LSP bridge:** new `codescope-lsp` crate (`crates/lsp/`) built on `tower-lsp`. Exposes the graph via the Language Server Protocol: `initialize`, `goto_definition`, `references`, `hover`, `workspace_symbol`, `document_symbol`. Works with VS Code, Zed, Neovim, and Helix — no editor extension needed. Invoke via `codescope lsp` (or the `codescope-lsp` binary directly).

### Changed
- **Tool consolidation round 3 (39 → 32):** `search_functions`, `find_function`, `file_entities`, `related`, `explore`, `backlinks` collapsed into one `search` tool with `mode=fuzzy|exact|file|cross_type|neighborhood|backlinks`. `contributor_map`, `suggest_reviewers`, `team_patterns` collapsed into one `contributors` tool with `mode=map|reviewers|patterns`. 9 tools → 2 tools. Total reduction across all rounds: 57 → 32 (44%).

### Fixes
- C/C++ function extraction now works (was silently returning `None`).

## [0.7.4] - 2026-04-14

File watcher auto re-index, daemon-aware init, and tool consolidation rounds 1 + 2 (57 → 39).

### Features
- **File watcher wired into MCP stdio auto-index:** live re-indexing now triggers automatically from the stdio MCP server, not just the daemon.
- **`codescope init --daemon`:** daemon-aware MCP config generation for users running the shared multi-project daemon.

### Changed
- **Tool consolidation rounds 1 + 2 (57 → 49 → 39):** 10 tools merged into 4 unified tools in round 1, then a further tightening in round 2. Agents should migrate to the consolidated tool names.

## [0.7.3] - 2026-04-14

Urgent fix for `knowledge_search` parse error on v0.7.2, project-rules installer, and web UI network access.

### Fixes
- **`knowledge_search` parse error:** added `updated_at` to the `SELECT` projection (was `ORDER BY`'d but not selected — SurrealDB parse error). Reported by a DGX Spark user on v0.7.2.
- **`knowledge_search` tag search:** inline tag literal instead of `.bind()` (SurrealDB `.bind()` does not work with `CONTAINS`). Output now also shows tags.
- `install.sh`: robust error handling — ERR trap, Windows bash detection, nested binary search with clearer error messages.
- DB: auto-recover from stale `LOCK` file via `pgrep` check instead of failing; better error message suggests `pkill` + `rm LOCK`.
- Clippy `ptr_arg` — `try_remove_stale_lock` takes `&Path` instead of `&PathBuf`.

### Features
- **`codescope web --host` flag:** web UI bind address for network/LAN access; LAN hostname shown in startup output.
- `setup-claude.sh` / `setup-claude.ps1` now also install rules to project-level `.claude/rules/` when the directory exists.
- `.claude/rules/codescope-mandatory.md` gains knowledge-tracking auto-triggers (`knowledge_search` before tasks, `knowledge_save` after).

### Internal
- CI: auto-format on push instead of failing the job. Bumped to Node.js 24. `cargo fmt --all` rule codified in CLAUDE.md.

## [0.7.2] - 2026-04-14

MCP tool drift fix, uninstall wizard, query decomposition, and design system tokens.

### Features
- **Query decomposition:** multi-step question handling for the `ask` engine — breaks compound questions into sub-queries, runs each against the graph, and composes a single answer.
- **Result archiving:** persist query results for later recall.
- **Uninstall mode in the setup wizard:** interactive removal of binaries, MCP config, and project rules.
- **Design system tokens:** shared color, spacing, and typography tokens across the web UI.
- **Work tracking protocol:** status tags (`status:done`, `status:planned`, `shipped:YYYY-MM-DD`, `vX.Y.Z`) documented for use with `knowledge_save` so future sessions can detect already-shipped work.

### Fixes
- **MCP tool drift:** slimmed tool descriptions and tightened `.claude/rules/` guidance so agents stop drifting back to `Read`/`Grep` between sessions.
- **Double MCP registration:** setup wizard now detects marketplace installs and skips re-registering the MCP server.

## [0.7.1] - 2026-04-13

Knowledge graph UI, delta-mode `context_bundle`, graph-ranked search, and multi-edge impact analysis.

### Features
- **Knowledge graph in the web UI:** knowledge nodes render as octahedrons alongside code entities, with dashed edges for `supports` / `contradicts` / `related_to` relationships. New knowledge panel shows confidence, tags, content, and linked entities. Command palette searches both code and knowledge with `kind` badges. Loading bar and error toasts added.
- **Delta-mode `context_bundle`:** repeat calls within a session return a structural diff instead of the full bundle, saving ~80-97% tokens per session (token-optimizer pattern).
- **Graph-ranked search:** search results re-sorted by caller count as a simplified Personalized PageRank proxy (graph-of-skills pattern).
- **Multi-edge `impact_analysis`:** after call-chain BFS, also reports importing files and trait implementors — complete blast radius, not just the call graph (graph-of-skills pattern).
- **Knowledge hot cache** + schema edge fields + GraphRAG positioning groundwork.
- `docs/llm-usage-guide.md` — tool selection and usage patterns for agents.

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

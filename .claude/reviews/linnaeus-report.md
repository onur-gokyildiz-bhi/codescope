# Linnaeus' Schema & Query Audit — 2026-04-14

## Schema health

### Tables (31 total)

**Code entity tables (14):** `file`, `` `function` ``, `class`, `module`, `variable`, `import_decl`, `config`, `doc`, `api`, `db_entity`, `infra`, `package`, `skill`, `http_call`

**Knowledge / conversation tables (6):** `conversation`, `conv_topic`, `decision`, `problem`, `solution`, `knowledge`

**Meta (1):** `meta`

**Edge / RELATION tables (24):** `contains`, `calls`, `imports`, `inherits`, `implements`, `uses`, `modified_in`, `depends_on`, `configures`, `defines_endpoint`, `has_field`, `references`, `depends_on_package`, `runs_script`, `discussed_in`, `decided_about`, `solves_for`, `co_discusses`, `links_to`, `calls_endpoint`, `supports`, `contradicts`, `related_to`

### Indexes — proper

- Every non-edge table has a `qualified_name` UNIQUE index
- `` `function` ``, `class`, `config`, `doc`, `pkg`, `infra`, `skill` all have BM25 full-text indexes on `name`
- `knowledge` has BM25 full-text indexes on both `title` and `content`
- Composite `(file_path, repo)` indexes on all major code tables (fn, class, module, var, import, cfg, doc, api, db, infra, pkg, skill, http_call)
- `(repo)` indexes on `` `function` ``, `class`, `file`, `skill`, `conversation`, `decision`, `knowledge`

### Indexes — missing / weak

| Table | Missing index | Impact |
|---|---|---|
| `knowledge` | `updated_at` | `ORDER BY updated_at DESC` (hot path: server startup cache + every search) scans full table |
| `knowledge` | `repo` already exists but `tags` has no index | `tags CONTAINS 'foo'` is full scan (array field) |
| `decision`, `problem`, `solution`, `conv_topic`, `conversation` | `timestamp` | All five tables are sorted by `timestamp DESC` in hot queries (`helpers.rs`, `exploration.rs`, `adr.rs`, `conversations.rs`) with no backing index |
| `decision`, `problem`, `solution` | `scope` | `WHERE scope ~ $scope` uses fuzzy match in `exploration.rs` on a non-indexed column |
| `file` | `language` | Language-filtered queries (`contributors.rs`, `helpers.rs` tech-stack) are full scans |
| `` `function` `` | `cuda_qualifier` | CUDA-specific filtering (if used) is a scan — low cardinality, probably acceptable |
| `` `function` `` | `body_hash` | Duplicate-detection query (`query.rs:935`) groups by `body_hash` on a non-indexed column |

### `any` types

None used. Schema uses `option<string>`, `option<int>`, `option<array>`, `option<datetime>` — properly typed. Arrays (`tags`, `embedding`, `binary_embedding`) have no element type but this is consistent with agent doctrine ("never use `any` except for `tags` arrays").

### Schema hygiene issues

1. **`crates/core/src/graph/schema.rs:268-274`** — `agent` field is defined on four tables (`decision`, `problem`, `solution`, `conv_topic`), but the definitions are interleaved inside the `decision` block rather than placed with their owning tables. Functionally correct, visually misleading.
2. **`meta` table** has only a `version` field and no index — fine for a single-row keyed read (`meta:schema`), but worth a comment noting why.
3. **`conv_topic`** — `DEFINE FIELD ... scope` at line 252 sits after the index definition, making it easy to miss.
4. **`knowledge`** — `kind` is stored as a free-form string with a non-full-text index (`know_kind`). Consider enumerating allowed kinds (`concept | decision | problem | ...`) via an `ASSERT $value IN [...]` clause, or at minimum documenting the taxonomy in the schema file.

## SurrealQL bug scan

| File | Line | Issue |
|---|---|---|
| `crates/core/src/temporal/graph_sync.rs` | 174 | `WHERE path CONTAINS $name` with `.bind("name", ...)` — known broken pattern, will return `[]` on non-trivial inputs. Inline the literal (escape single quotes) |
| `crates/core/src/crossrepo/linker.rs` | 66-71 | `WHERE path CONTAINS $module` with `.bind("module", ...)` — same bug. Cross-repo linker will silently link nothing |
| `crates/mcp-server/src/tools/adr.rs` | 91 | `WHERE name CONTAINS $search` with `.bind("search", ...)` — `get` action on ADRs will return nothing for any non-trivial search string |
| `crates/mcp-server/src/tools/conversations.rs` | 313 | `WHERE body CONTAINS $name` with `.bind("name", ...)` — timeline search is broken |
| `crates/core/src/graph/query.rs` | 816-833 | `find_unused_symbols` filters with `kind NOT IN ['override', 'virtual']` — the `` `function` `` table has **no `kind` field** (see schema line 47-60). Predicate either errors or silently matches everything. Also missing `WHERE repo = $repo` → cross-project leakage |
| `crates/mcp-server/src/tools/analytics.rs` | 109-137, 160-161 | `code_communities` (clusters / bridges / central) — none include `WHERE repo = $repo`. Multi-project DB will mix files across repos |
| `crates/mcp-server/src/tools/quality.rs` | 177-201, 256, 326 | `code_review_helper` hotspots / largest / most-funcs-per-file — no `repo` scope. `find_unused_symbols` analog, will leak across projects |
| `crates/mcp-server/src/tools/quality.rs` | 84 | `WHERE file_path CONTAINS '{inline}'` — inline is fine, but the literal goes through `.replace('\'', "")` only; no validation of `%` or other SurrealQL wildcards if the API ever swaps CONTAINS for LIKE |
| `crates/web/src/lib.rs` | 275, 512, 711, 787, 999, 1039, 1076, 1097 | Multiple web-handler queries — none scope by `repo`. Web is intentionally single-project but worth a comment noting the assumption |
| `crates/core/src/graph/query.rs` | 953-959 | `backlinks` — `SELECT ... FROM import_decl WHERE string::contains(name, $name)` has no `repo` scope, returns cross-project importers |
| `crates/mcp-server/src/helpers.rs` | 577 | `GROUP BY out.name ORDER BY call_count DESC` — `call_count` is in SELECT, OK. But no `repo` filter on `calls` edge (edges inherit from endpoints; still worth verifying in practice) |

### Confirmed-clean patterns

- All `function` table references use backticks (checked via grep across all crates — zero unquoted usages in SurrealQL strings).
- Multi-hop traversals use direct chain syntax (`<-calls<-\`function\`<-calls<-\`function\`.name`) — no dots between hops observed in `bench/main.rs:233,241`, `core/graph/query.rs:285,581-587`, `analytics.rs:299,324`, `callgraph.rs:84`, `conversations.rs:252,329-331`.
- `ORDER BY` fields with a computed alias (`line_count`, `size`, `bridge_score`, `score`, `in_degree`, `total_edges`, `caller_count`, `callee_count`, `cnt`, `func_count`, `fn_count`, `callers`) are all present in their respective `SELECT` projections. No "Missing order idiom" bombs found.
- `knowledge_search` (`tools/knowledge.rs:77-85`) correctly inlines the tag literal instead of binding it — consistent with the documented CONTAINS-bind workaround.

## Migration state

- **`SCHEMA_VERSION`:** `1` (in `crates/core/src/graph/schema.rs:9`)
- **Registered migrations:** `1` (v0 → v1: "Initial schema_version tracking", no-op data transform)
- **Drift:** **Significant.** Since `SCHEMA_VERSION = 1` was set, the schema has added entire table families without a version bump:
  - `knowledge` + `supports` / `contradicts` / `related_to` edges
  - `conversation` / `conv_topic` / `decision` / `problem` / `solution`
  - `skill`, `http_call`, `api`, `db_entity`, `infra`, `package`, `doc`, `config`
  - `tier`, `scope`, `agent`, `rationale` fields on decision/problem/solution/conv_topic
  - BM25 full-text analyzer + indexes
  - `calls_endpoint` / `links_to` edges

  All of these are picked up by `IF NOT EXISTS` on reconnect, so existing DBs do get the new structures — but `SCHEMA_VERSION` is still `1`. A user inspecting `meta:schema.version` to reason about DB capabilities will be misled. The migration framework is functional but under-used.

- **Migration code correctness:** `migrate_to_current` (migrations.rs:50) correctly walks from recorded version up to target. The loop only advances `current` when `m.from_version == current`, so a gap in the chain would silently skip — today that can't happen (only one migration), but a future author could introduce a skipped step.

## Action items

1. **Bump `SCHEMA_VERSION` to `2`** and add a documentation-only migration entry that describes the schema additions since v1 shipped. Future-proofs any data transform that does need a real migration, and stops `meta:schema.version` from lying.
2. **Fix the four `CONTAINS $bind` bugs** in `graph_sync.rs:174`, `linker.rs:67`, `adr.rs:91`, `conversations.rs:313`. Inline the literal using the same `.replace('\'', "")` escape pattern already used in `knowledge.rs:76` and `quality.rs:84-86`.
3. **Remove the nonexistent `kind` field reference** in `crates/core/src/graph/query.rs:829` (`find_unused_symbols`). Either drop the predicate or — if CUDA / override semantics are wanted — add `kind` to the `` `function` `` schema and populate it during indexing.
4. **Add `WHERE repo = $repo`** to the analytics queries in `analytics.rs` (`code_communities`), `quality.rs` (hotspots / largest / per-file counts), and `query.rs:818` (`find_unused_symbols`). Multi-project DBs are a core feature — these are leaks.
5. **Add missing indexes:**
   - `DEFINE INDEX know_updated ON knowledge FIELDS updated_at` (hot path, every server startup)
   - `DEFINE INDEX dec_timestamp ON decision FIELDS timestamp` (+ analogous for `problem`, `solution`, `conv_topic`, `conversation`)
   - `DEFINE INDEX file_language ON file FIELDS language`
   - `DEFINE INDEX fn_body_hash ON \`function\` FIELDS body_hash` (duplicate detection)
6. **Cosmetic:** reorder `crates/core/src/graph/schema.rs:268-274` so each `agent` field definition sits in its owning table's block. Same for the stray `scope` field on `conv_topic` at line 252.
7. **Consider adding `ASSERT` constraints** on enumerable strings: `knowledge.kind` (concept|decision|problem|source|...), `decision.tier` (0..=3), `knowledge.confidence` (low|medium|high). Currently relying on convention — one typo silently persists.

## Notes (not action items)

- The global-vs-project DB separation (`helpers::connect_global_db`) is a clean way to sidestep the `repo = $repo` scoping question for `knowledge` search. Good pattern — keep it.
- No `any` types found. Discipline is holding.
- All `RELATION` edge tables are correctly declared `TYPE RELATION SCHEMAFULL` before `RELATE` statements — no silent edge-as-document-table regressions.

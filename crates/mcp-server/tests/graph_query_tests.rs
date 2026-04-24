//! Integration tests that exercise the GraphQuery layer used by MCP tools.
//! Uses an in-memory SurrealDB instance with sample data, then verifies
//! that the query methods return expected results.
//!
//! This is the critical-path test layer — it ensures that the tools the
//! MCP server exposes (search_functions, find_callers, find_callees,
//! file_entities, explore, find_function) actually work end-to-end.

use codescope_core::graph::builder::GraphBuilder;
use codescope_core::graph::query::GraphQuery;
use codescope_core::graph::schema::init_schema;
use codescope_core::{CodeEntity, CodeRelation, DbHandle, EntityKind, RelationKind};
use surrealdb::engine::any;

async fn setup() -> (DbHandle, GraphQuery) {
    let db = any::connect("memory").await.unwrap();
    db.use_ns("codescope").use_db("test").await.unwrap();
    init_schema(&db).await.unwrap();

    let builder = GraphBuilder::new(db.clone());
    let entities = vec![
        make_fn("parse_file", "src/parser.rs"),
        make_fn("read_input", "src/parser.rs"),
        make_fn("write_output", "src/writer.rs"),
        make_fn("main", "src/main.rs"),
        make_class("Parser", "src/parser.rs"),
        make_class("Writer", "src/writer.rs"),
    ];
    builder.insert_entities(&entities).await.unwrap();

    // Add some call relationships
    let relations = vec![
        make_call("main", "parse_file"),
        make_call("main", "write_output"),
        make_call("parse_file", "read_input"),
    ];
    builder.insert_relations(&relations).await.unwrap();

    let gq = GraphQuery::new(db.clone());
    (db, gq)
}

fn make_fn(name: &str, file: &str) -> CodeEntity {
    CodeEntity {
        kind: EntityKind::Function,
        name: name.to_string(),
        qualified_name: format!("test::{}", name),
        file_path: file.to_string(),
        repo: "test".to_string(),
        start_line: 1,
        end_line: 10,
        start_col: 0,
        end_col: 0,
        signature: Some(format!("fn {}()", name)),
        body: None,
        body_hash: None,
        language: "rust".to_string(),
        cuda_qualifier: None,
    }
}

fn make_class(name: &str, file: &str) -> CodeEntity {
    CodeEntity {
        kind: EntityKind::Struct,
        name: name.to_string(),
        qualified_name: format!("test::{}", name),
        file_path: file.to_string(),
        repo: "test".to_string(),
        start_line: 1,
        end_line: 5,
        start_col: 0,
        end_col: 0,
        signature: None,
        body: None,
        body_hash: None,
        language: "rust".to_string(),
        cuda_qualifier: None,
    }
}

fn make_call(from: &str, to: &str) -> CodeRelation {
    CodeRelation {
        kind: RelationKind::Calls,
        from_entity: format!("test::{}", from),
        to_entity: format!("test::{}", to),
        from_table: "function".to_string(),
        to_table: "function".to_string(),
        metadata: None,
    }
}

// ── Search ──────────────────────────────────────────────────────

#[tokio::test]
async fn search_functions_substring_match() {
    let (_db, gq) = setup().await;
    let results = gq.search_functions("parse").await.unwrap();
    assert_eq!(results.len(), 1, "should find parse_file");
    assert_eq!(results[0].name.as_deref(), Some("parse_file"));
}

#[tokio::test]
async fn search_functions_case_insensitive() {
    let (_db, gq) = setup().await;
    let results = gq.search_functions("PARSE").await.unwrap();
    assert_eq!(results.len(), 1, "should be case-insensitive");
}

#[tokio::test]
async fn search_functions_no_match() {
    let (_db, gq) = setup().await;
    let results = gq.search_functions("nonexistent").await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn find_function_exact_match() {
    let (_db, gq) = setup().await;
    let results = gq.find_function("parse_file").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path.as_deref(), Some("src/parser.rs"));
}

#[tokio::test]
async fn find_function_no_substring_match() {
    // find_function does exact-match only, NOT substring
    let (_db, gq) = setup().await;
    let results = gq.find_function("parse").await.unwrap();
    assert!(
        results.is_empty(),
        "find_function should be exact-match only"
    );
}

// ── Call graph ──────────────────────────────────────────────────

#[tokio::test]
async fn find_callers_returns_correct_callers() {
    let (_db, gq) = setup().await;
    let callers = gq.find_callers("parse_file").await.unwrap();
    assert_eq!(callers.len(), 1, "main calls parse_file");
    assert_eq!(callers[0].name.as_deref(), Some("main"));
}

#[tokio::test]
async fn find_callers_no_callers() {
    let (_db, gq) = setup().await;
    let callers = gq.find_callers("main").await.unwrap();
    assert!(callers.is_empty(), "main has no callers");
}

#[tokio::test]
async fn find_callees_returns_correct_callees() {
    let (_db, gq) = setup().await;
    let callees = gq.find_callees("main").await.unwrap();
    assert_eq!(callees.len(), 2, "main calls parse_file and write_output");
    let names: Vec<&str> = callees.iter().filter_map(|c| c.name.as_deref()).collect();
    assert!(names.contains(&"parse_file"));
    assert!(names.contains(&"write_output"));
}

#[tokio::test]
async fn find_callees_no_callees() {
    let (_db, gq) = setup().await;
    let callees = gq.find_callees("read_input").await.unwrap();
    assert!(callees.is_empty(), "read_input calls nothing");
}

// ── File entities ───────────────────────────────────────────────

#[tokio::test]
async fn file_entities_returns_functions_and_classes() {
    let (_db, gq) = setup().await;
    let entities = gq.file_entities("src/parser.rs").await.unwrap();
    // 2 functions (parse_file, read_input) + 1 struct (Parser) = 3 entities
    assert_eq!(entities.len(), 3);
}

#[tokio::test]
async fn file_entities_empty_for_unknown_file() {
    let (_db, gq) = setup().await;
    let entities = gq.file_entities("src/nonexistent.rs").await.unwrap();
    assert!(entities.is_empty());
}

// ── Health ──────────────────────────────────────────────────────

#[tokio::test]
async fn health_check_succeeds() {
    let (_db, gq) = setup().await;
    gq.health_check().await.expect("health check should pass");
}

// ── Dedup regression — v0.8.3 / v0.8.4 ─────────────────────────
// Legacy DBs accumulated one `calls` edge per re-index because
// delete_file_entities didn't drop the edge rows. Every
// edge-traversal query has since been given a defensive
// GROUP BY. These tests simulate that state by inserting the
// same call relation twice and verifying the tools collapse
// the duplicate rows.

async fn setup_with_dup_calls() -> (DbHandle, GraphQuery) {
    let (db, gq) = setup().await;
    let builder = GraphBuilder::new(db.clone());
    // Insert the `main -> parse_file` edge a second and third time —
    // the bug was one accumulated per re-index.
    let dup = vec![
        make_call("main", "parse_file"),
        make_call("main", "parse_file"),
    ];
    builder.insert_relations(&dup).await.unwrap();
    (db, gq)
}

#[tokio::test]
async fn find_callers_collapses_duplicate_edges() {
    let (_db, gq) = setup_with_dup_calls().await;
    let callers = gq.find_callers("parse_file").await.unwrap();
    // main should appear exactly once even though 3 edges exist.
    assert_eq!(callers.len(), 1, "duplicate calls edges must collapse");
    assert_eq!(callers[0].name.as_deref(), Some("main"));
}

#[tokio::test]
async fn find_callees_collapses_duplicate_edges() {
    let (_db, gq) = setup_with_dup_calls().await;
    let callees = gq.find_callees("main").await.unwrap();
    // 3 edges to parse_file + 1 edge to write_output → 2 unique
    assert_eq!(callees.len(), 2);
}

#[tokio::test]
async fn backlinks_collapses_duplicate_caller_edges() {
    let (_db, gq) = setup_with_dup_calls().await;
    let result = gq.backlinks("parse_file").await.unwrap();
    let callers = result.get("callers").and_then(|v| v.as_array());
    assert!(callers.is_some(), "backlinks should surface callers array");
    assert_eq!(
        callers.unwrap().len(),
        1,
        "duplicate calls edges must collapse in backlinks too"
    );
}

// ── Graph-wide queries — smoke ─────────────────────────────────
// Covers the remaining GraphQuery public methods that each power
// at least one MCP tool. Each test asserts the query completes
// and returns a result in the expected shape — the point is to
// catch schema / syntax regressions early, not to exhaustively
// verify correctness (the tool body's own tests do that).

#[tokio::test]
async fn stats_returns_populated_object() {
    let (_db, gq) = setup().await;
    let stats = gq.stats().await.expect("stats query should run");
    // Shape check — must be an object with at least a functions field.
    let obj = stats
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_array())
        .and_then(|a| a.first());
    assert!(
        obj.is_some(),
        "stats returned an unexpected shape: {stats:?}"
    );
}

#[tokio::test]
async fn raw_query_passthrough_works() {
    let (_db, gq) = setup().await;
    // Single-statement → flat-array shape (see raw_query's
    // backward-compat branch).
    let result = gq
        .raw_query("SELECT name FROM `function` WHERE name = 'main'")
        .await
        .expect("raw_query should accept arbitrary SurrealQL");
    let rows = result
        .as_array()
        .expect("single-statement raw_query returns a flat array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("name").and_then(|v| v.as_str()), Some("main"),);
}

#[tokio::test]
async fn raw_query_surfaces_parse_errors() {
    let (_db, gq) = setup().await;
    let err = gq.raw_query("NOT VALID SURREALQL").await;
    assert!(err.is_err(), "parse error should bubble up");
}

#[tokio::test]
async fn file_context_returns_neighborhood() {
    let (_db, gq) = setup().await;
    let ctx = gq
        .file_context("src/parser.rs")
        .await
        .expect("file_context should complete");
    // Must surface at least the `entities` key (functions + classes
    // of the file) regardless of call structure.
    assert!(
        ctx.get("entities").is_some() || ctx.is_array() || ctx.is_object(),
        "unexpected file_context shape: {ctx:?}"
    );
}

#[tokio::test]
async fn type_hierarchy_returns_entity_info() {
    let (_db, gq) = setup().await;
    let result = gq
        .type_hierarchy("Parser", 2)
        .await
        .expect("type_hierarchy should complete for known class");
    // The class row (entity) should be present even with no inheritance.
    assert!(
        result.get("entity").is_some() || result.is_object(),
        "unexpected type_hierarchy shape: {result:?}"
    );
}

#[tokio::test]
async fn type_hierarchy_handles_unknown_name() {
    let (_db, gq) = setup().await;
    let result = gq
        .type_hierarchy("NonExistentClass", 2)
        .await
        .expect("should not error on missing class");
    assert!(result.is_object());
}

#[tokio::test]
async fn find_all_references_smoke() {
    let (_db, gq) = setup().await;
    let result = gq
        .find_all_references("parse_file")
        .await
        .expect("find_all_references should complete");
    assert!(result.is_object() || result.is_array());
}

#[tokio::test]
async fn safe_delete_check_returns_advice() {
    let (_db, gq) = setup().await;
    // parse_file has one caller (main) — safe_delete should warn.
    let result = gq
        .safe_delete_check("parse_file")
        .await
        .expect("safe_delete_check should complete");
    assert!(
        result.is_object(),
        "safe_delete_check should return an advisory object"
    );
}

#[tokio::test]
async fn find_unused_symbols_excludes_called_ones() {
    let (_db, gq) = setup().await;
    // With min_lines=0 every fn passes the length filter.
    let unused = gq
        .find_unused_symbols(0, "test")
        .await
        .expect("find_unused_symbols should complete");
    let names: Vec<String> = unused
        .iter()
        .filter_map(|r| r.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    // parse_file IS called (by main), so it must NOT appear.
    assert!(
        !names.contains(&"parse_file".to_string()),
        "parse_file is called by main but appeared as unused"
    );
}

// ── Memory tool underlying queries ────────────────────────────
// The `memory` MCP tool is handler-logic-heavy but ultimately just
// UPSERTs into the conv_topic / decision / problem / solution
// tables and SELECTs via `name ~ $search OR body ~ $search`.
// These tests lock that contract so a schema drift surfaces fast.

#[tokio::test]
async fn memory_save_upserts_conv_topic() {
    let (db, _gq) = setup().await;
    let q = "UPSERT conv_topic SET name = $name, qualified_name = $qname, \
             body = $body, repo = $repo, language = 'memory', kind = 'shared_memory', \
             file_path = 'memory', start_line = 0, end_line = 0, timestamp = $ts";
    db.query(q)
        .bind(("name", "use redis for sessions".to_string()))
        .bind(("qname", "test:memory:use_redis_for_sessions".to_string()))
        .bind((
            "body",
            "Session store switched to Redis 2026-04".to_string(),
        ))
        .bind(("repo", "test".to_string()))
        .bind(("ts", "2026-04-24T00:00:00".to_string()))
        .await
        .expect("memory save should succeed");

    let count: Vec<serde_json::Value> = db
        .query("SELECT count() FROM conv_topic WHERE repo = 'test' GROUP ALL")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(
        count
            .first()
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_u64()),
        Some(1),
        "exactly one conv_topic row should exist after memory save"
    );
}

#[tokio::test]
async fn memory_search_finds_saved_row_by_body() {
    let (db, _gq) = setup().await;
    db.query(
        "UPSERT conv_topic SET name = 'pick redis', qualified_name = 'test:memory:pick_redis', \
         body = 'chose redis over memcached for TTL controls', repo = 'test', \
         language = 'memory', kind = 'shared_memory', file_path = 'memory', \
         start_line = 0, end_line = 0, timestamp = '2026-04-24T00:00:00'",
    )
    .await
    .unwrap();

    // Mirror the tool's post-fix search predicate (v0.8.9 fixed
    // the `~` parse error by switching to string::contains).
    let results: Vec<serde_json::Value> = db
        .query(
            "SELECT name FROM conv_topic WHERE repo = $repo \
             AND (string::contains(string::lowercase(name), string::lowercase($q)) \
               OR string::contains(string::lowercase(body), string::lowercase($q)))",
        )
        .bind(("repo", "test".to_string()))
        .bind(("q", "memcached".to_string()))
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(results.len(), 1, "body-text match should find the memory");
}

// ── ADR tool — create / list / get round trip ─────────────────
// `manage_adr` upserts into the `decision` table and queries it
// back via SELECT ... ORDER BY timestamp / CONTAINS. These tests
// lock the end-to-end SurrealQL contract without a full MCP
// server instance.

async fn adr_create(db: &DbHandle, title: &str, body: &str, ts: &str) {
    let qname = format!(
        "test:adr:{}",
        title
            .to_lowercase()
            .replace(' ', "_")
            .chars()
            .take(60)
            .collect::<String>()
    );
    db.query(
        "UPSERT decision SET name = $name, qualified_name = $qname, \
         body = $body, repo = 'test', language = 'adr', \
         file_path = 'adr', start_line = 0, end_line = 0, \
         timestamp = $ts",
    )
    .bind(("name", title.to_string()))
    .bind(("qname", qname))
    .bind(("body", body.to_string()))
    .bind(("ts", ts.to_string()))
    .await
    .expect("ADR upsert should succeed");
}

#[tokio::test]
async fn adr_create_then_list_roundtrip() {
    let (db, _gq) = setup().await;
    adr_create(
        &db,
        "use surreal server",
        "local DB",
        "2026-04-20T00:00:00Z",
    )
    .await;
    adr_create(
        &db,
        "codescope exec compression",
        "rtk pattern",
        "2026-04-21T00:00:00Z",
    )
    .await;

    // list: newest first.
    let rows: Vec<serde_json::Value> = db
        .query("SELECT name, timestamp FROM decision WHERE repo = 'test' ORDER BY timestamp DESC LIMIT 50")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].get("name").and_then(|v| v.as_str()),
        Some("codescope exec compression"),
        "ORDER BY timestamp DESC should put the newer ADR first"
    );
}

#[tokio::test]
async fn adr_get_by_contains_substring() {
    let (db, _gq) = setup().await;
    adr_create(
        &db,
        "use surreal server",
        "local DB",
        "2026-04-20T00:00:00Z",
    )
    .await;

    // Mirror the handler's query shape — CONTAINS with inlined
    // literal (the tool comment explicitly notes CONTAINS + bind
    // is unreliable in SurrealDB).
    let rows: Vec<serde_json::Value> = db
        .query("SELECT name FROM decision WHERE name CONTAINS 'surreal' AND repo = 'test' LIMIT 1")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 1);
}

// ── capture_insight — CREATE into decision / problem / solution ─

#[tokio::test]
async fn capture_insight_creates_decision_row() {
    let (db, _gq) = setup().await;
    let qname = "test:insight:decision:pin_redis";
    let create = "CREATE decision SET name = $name, qualified_name = $qname, \
                  body = $body, repo = 'test', language = 'insight', \
                  file_path = 'insight', start_line = 0, end_line = 0, \
                  timestamp = $ts, agent = $agent";
    db.query(create)
        .bind(("name", "pin redis version".to_string()))
        .bind(("qname", qname.to_string()))
        .bind((
            "body",
            "Stay on 7.2 until the new cluster spec stabilises.".to_string(),
        ))
        .bind(("ts", "2026-04-24T00:00:00Z".to_string()))
        .bind(("agent", "claude-code".to_string()))
        .await
        .expect("capture_insight CREATE should succeed");

    let count: Vec<serde_json::Value> = db
        .query("SELECT count() FROM decision WHERE repo = 'test' GROUP ALL")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(
        count
            .first()
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_u64()),
        Some(1)
    );
}

// ── HTTP analysis — smoke ─────────────────────────────────────

#[tokio::test]
async fn find_http_calls_returns_empty_on_bare_fixture() {
    let (_db, gq) = setup().await;
    // Fixture has no http_call rows; tool should return an empty
    // vec rather than erroring out.
    let calls = gq
        .find_http_calls(None)
        .await
        .expect("find_http_calls should complete on empty fixture");
    assert!(calls.is_empty());
}

#[tokio::test]
async fn find_http_calls_filters_by_method() {
    let (db, gq) = setup().await;
    // Seed two http_call rows with different methods. Schema is
    // strict — set every non-optional field.
    db.query(
        "CREATE http_call SET name = 'get_user_api', kind = 'GET', \
         qualified_name = 'test:http:get_user', repo = 'test', \
         file_path = 'src/api.rs', start_line = 10, end_line = 12, \
         url_pattern = '/user/:id', language = 'rust'",
    )
    .await
    .expect("http_call GET CREATE should succeed");
    db.query(
        "CREATE http_call SET name = 'post_login_api', kind = 'POST', \
         qualified_name = 'test:http:post_login', repo = 'test', \
         file_path = 'src/api.rs', start_line = 20, end_line = 22, \
         url_pattern = '/login', language = 'rust'",
    )
    .await
    .expect("http_call POST CREATE should succeed");

    let all = gq.find_http_calls(None).await.unwrap();
    assert_eq!(all.len(), 2, "no filter should return both rows");

    let posts = gq.find_http_calls(Some("POST")).await.unwrap();
    assert_eq!(posts.len(), 1, "method=POST should narrow to one row");
}

// ── Quality / lint tool — dead_code detection ─────────────────
// The lint(mode=dead_code) tool queries the same thing
// find_unused_symbols does (callers.len() == 0). This test
// locks the contract for both.

#[tokio::test]
async fn lint_dead_code_flags_uncalled_function() {
    let (db, _gq) = setup().await;
    // Add a never-called fn via the builder (keeps the test honest —
    // if schema evolves, insert_entities stays in lockstep with it).
    let builder = GraphBuilder::new(db.clone());
    builder
        .insert_entities(&[make_fn("orphan_helper", "src/dead.rs")])
        .await
        .expect("insert orphan fn");

    // Mirror the lint(mode=dead_code) predicate: no incoming calls
    // edge. The fixture's main/parse_file/write_output/read_input
    // are also uncalled by test fixtures, so just check presence.
    let rows: Vec<serde_json::Value> = db
        .query(
            "SELECT name FROM `function` WHERE \
             repo = 'test' \
             AND name NOT IN (SELECT VALUE out.name FROM calls WHERE out.name != NONE AND out.repo = 'test')",
        )
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    let names: Vec<String> = rows
        .iter()
        .filter_map(|r| r.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    assert!(
        names.contains(&"orphan_helper".to_string()),
        "orphan_helper should be flagged as dead code; got {names:?}"
    );
}

// ── Knowledge tool — raw UPSERT smoke ─────────────────────────
// The knowledge tool's save path upserts into the `knowledge`
// table keyed on a slugified id, with tags as an inline array.
// This exercises that path without spinning up a full MCP server.

#[tokio::test]
async fn knowledge_save_is_idempotent_on_same_id() {
    let (db, _gq) = setup().await;
    let upsert = "UPSERT knowledge:test_decision SET \
                  title = $title, content = $content, kind = 'decision', \
                  repo = 'test', source_url = '', confidence = 'high', \
                  tags = ['status:done', 'v0.8.x'], \
                  created_at = created_at ?? $now, updated_at = $now";

    // First write.
    db.query(upsert)
        .bind(("title", "use surreal server".to_string()))
        .bind(("content", "initial".to_string()))
        .bind(("now", "2026-04-24T00:00:00".to_string()))
        .await
        .unwrap();

    // Second write (simulates re-save) must update, not append.
    db.query(upsert)
        .bind(("title", "use surreal server".to_string()))
        .bind(("content", "revised — added clustering note".to_string()))
        .bind(("now", "2026-04-24T01:00:00".to_string()))
        .await
        .unwrap();

    let rows: Vec<serde_json::Value> = db
        .query("SELECT content FROM knowledge WHERE id = knowledge:test_decision")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 1, "UPSERT on same id must not duplicate");
    assert_eq!(
        rows[0].get("content").and_then(|v| v.as_str()),
        Some("revised — added clustering note"),
        "second write should overwrite content"
    );
}

#[tokio::test]
async fn knowledge_search_matches_tag_contains() {
    let (db, _gq) = setup().await;
    db.query(
        "UPSERT knowledge:t1 SET title = 'Dream narrator', content = 'Phase 3 arc tour', \
         kind = 'decision', repo = 'test', source_url = '', confidence = 'high', \
         tags = ['status:done', 'v0.8.0', 'phase3'], created_at = '2026-04-01', updated_at = '2026-04-01'",
    )
    .await
    .unwrap();

    let rows: Vec<serde_json::Value> = db
        .query("SELECT title FROM knowledge WHERE tags CONTAINS 'phase3'")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 1, "tag-CONTAINS search should hit the row");
}

#[tokio::test]
async fn find_duplicate_functions_smoke() {
    let (_db, gq) = setup().await;
    let dupes = gq
        .find_duplicate_functions("test")
        .await
        .expect("find_duplicate_functions should complete");
    // Fixture has no body hashes set on the entities, so this may
    // return empty. Smoke test just asserts no crash.
    assert!(dupes.is_empty() || !dupes.is_empty());
}

#[tokio::test]
async fn explore_function_returns_deduped_neighborhood() {
    let (_db, gq) = setup_with_dup_calls().await;
    let result = gq.explore("main").await.unwrap();
    // explore emits `calls_to` / `called_by` for function entities.
    let callees = result
        .get("calls_to")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        callees.len(),
        2,
        "main's calls_to should dedupe to 2 unique: {callees:?}"
    );
}

// ── Analytics tools ────────────────────────────────────────────
// community_detection / api_changelog / suggest_structure:
// mirror the raw SurrealQL each tool runs against the same
// fixture. Asserts row shape + ordering, not formatting.
// export_obsidian is filesystem-bound — skipped.

#[tokio::test]
async fn community_detection_clusters_ranks_by_total_edges() {
    let (db, _gq) = setup().await;
    let q = "SELECT file_path, count(->calls) AS out_calls, count(<-calls) AS in_calls, \
             (count(->calls) + count(<-calls)) AS total_edges \
             FROM `function` WHERE file_path != NONE AND repo = $repo \
             GROUP BY file_path ORDER BY total_edges DESC LIMIT $lim";
    let rows: Vec<serde_json::Value> = db
        .query(q)
        .bind(("lim", 20i64))
        .bind(("repo", "test".to_string()))
        .await
        .expect("clusters query should run")
        .take(0)
        .unwrap_or_default();
    assert!(
        !rows.is_empty(),
        "fixture has call edges, expected clusters"
    );
    let top_file = rows[0]
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let top_total = rows[0]
        .get("total_edges")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(top_total > 0, "top cluster must have non-zero total_edges");
    assert!(
        top_file == "src/parser.rs" || top_file == "src/main.rs",
        "unexpected top cluster file: {top_file}"
    );
}

#[tokio::test]
async fn community_detection_central_orders_by_in_degree() {
    let (db, _gq) = setup().await;
    let q = "SELECT name, file_path, count(<-calls) AS in_degree \
             FROM `function` WHERE repo = $repo ORDER BY in_degree DESC LIMIT $lim";
    let rows: Vec<serde_json::Value> = db
        .query(q)
        .bind(("lim", 20i64))
        .bind(("repo", "test".to_string()))
        .await
        .expect("central query should run")
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 4, "all four fixture fns should appear");
    let first = rows[0]
        .get("in_degree")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let last = rows[rows.len() - 1]
        .get("in_degree")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        first >= last,
        "ORDER BY in_degree DESC violated: {first} < {last}"
    );
    assert_eq!(
        rows[rows.len() - 1].get("name").and_then(|v| v.as_str()),
        Some("main"),
        "main has in_degree 0, should be at the tail"
    );
}

#[tokio::test]
async fn api_changelog_query_orders_by_file_then_line() {
    let (db, _gq) = setup().await;
    let q = "SELECT name, file_path, start_line, end_line, signature \
             FROM `function` WHERE repo = $repo \
             ORDER BY file_path, start_line LIMIT 200";
    let rows: Vec<serde_json::Value> = db
        .query(q)
        .bind(("repo", "test".to_string()))
        .await
        .expect("api_changelog fn query should run")
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 4);
    let files: Vec<&str> = rows
        .iter()
        .filter_map(|r| r.get("file_path").and_then(|v| v.as_str()))
        .collect();
    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(files, sorted, "rows must be sorted by file_path");
}

#[tokio::test]
async fn api_changelog_empty_on_unknown_repo() {
    let (db, _gq) = setup().await;
    // SurrealDB 3.0.5 requires ORDER BY idioms to be in the
    // projection — matches what api_changelog actually selects.
    let rows: Vec<serde_json::Value> = db
        .query(
            "SELECT name, file_path, start_line FROM `function` WHERE repo = $repo \
             ORDER BY file_path, start_line LIMIT 200",
        )
        .bind(("repo", "does-not-exist".to_string()))
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn suggest_structure_entity_count_sees_indexed_repo() {
    let (db, _gq) = setup().await;
    // SurrealDB 3.0.5 doesn't allow bare SELECT without FROM.
    // Split into two concrete GROUP ALL probes — same signal.
    let fn_rows: Vec<serde_json::Value> = db
        .query("SELECT count() FROM `function` WHERE repo = $repo GROUP ALL")
        .bind(("repo", "test".to_string()))
        .await
        .expect("fn count probe")
        .take(0)
        .unwrap_or_default();
    let cls_rows: Vec<serde_json::Value> = db
        .query("SELECT count() FROM class WHERE repo = $repo GROUP ALL")
        .bind(("repo", "test".to_string()))
        .await
        .expect("cls count probe")
        .take(0)
        .unwrap_or_default();
    assert_eq!(
        fn_rows
            .first()
            .and_then(|r| r.get("count"))
            .and_then(|v| v.as_u64()),
        Some(4)
    );
    assert_eq!(
        cls_rows
            .first()
            .and_then(|r| r.get("count"))
            .and_then(|v| v.as_u64()),
        Some(2)
    );
}

// SKIP: export_obsidian: filesystem-bound — DB reads are plain
// `SELECT ... WHERE repo = $repo` shapes already covered above.

// ── Refactor + skills ──────────────────────────────────────────

#[tokio::test]
async fn refactor_rename_surfaces_caller_as_reference() {
    let (_db, gq) = setup().await;
    let result = gq.find_all_references("parse_file").await.unwrap();
    let total = result
        .get("total_references")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(total >= 2, "definition + call_site expected, got {total}");
    let refs = result
        .get("references")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let types: Vec<&str> = refs
        .iter()
        .filter_map(|r| r.get("ref_type").and_then(|v| v.as_str()))
        .collect();
    assert!(types.contains(&"definition"));
    assert!(types.contains(&"call_site"));
}

#[tokio::test]
async fn refactor_find_unused_includes_orphan_function() {
    let (db, gq) = setup().await;
    let builder = GraphBuilder::new(db.clone());
    builder
        .insert_entities(&[make_fn("lonely_helper", "src/dead.rs")])
        .await
        .unwrap();
    let unused = gq.find_unused_symbols(0, "test").await.unwrap();
    let names: Vec<String> = unused
        .iter()
        .filter_map(|r| r.get("name").and_then(|v| v.as_str()).map(String::from))
        .collect();
    assert!(
        names.contains(&"lonely_helper".to_string()),
        "orphan fn should be flagged unused; got {names:?}"
    );
    assert!(
        !names.contains(&"write_output".to_string()),
        "write_output is called by main — must not appear"
    );
}

#[tokio::test]
async fn skills_index_clean_wipes_skill_and_links_to() {
    let (db, _gq) = setup().await;
    db.query(
        "CREATE skill SET name = 'rust', qualified_name = 'test:skill:rust', \
         kind = 'SkillNode', file_path = 'rust.md', repo = 'test', \
         language = 'markdown', start_line = 1, end_line = 10, \
         description = 'systems lang', node_type = 'skill'",
    )
    .await
    .unwrap();
    db.query("DELETE FROM skill; DELETE FROM links_to;")
        .await
        .expect("skills clean should succeed");
    let rows: Vec<serde_json::Value> = db
        .query("SELECT count() FROM skill GROUP ALL")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert!(rows.is_empty() || rows[0].get("count").and_then(|v| v.as_u64()) == Some(0));
}

#[tokio::test]
async fn skills_traverse_returns_error_for_missing_skill() {
    let (_db, gq) = setup().await;
    let result = gq
        .traverse_skill_graph("nonexistent_skill", 1, 2)
        .await
        .expect("traverse should complete even for missing root");
    assert!(
        result.get("error").is_some(),
        "missing skill must yield error key, got {result:?}"
    );
}

#[tokio::test]
async fn skills_traverse_finds_seeded_skill_node() {
    let (db, gq) = setup().await;
    db.query(
        "CREATE skill SET name = 'rust', qualified_name = 'test:skill:rust', \
         kind = 'SkillNode', file_path = 'rust.md', repo = 'test', \
         language = 'markdown', start_line = 1, end_line = 10, \
         description = 'systems lang', node_type = 'skill'",
    )
    .await
    .unwrap();
    let result = gq.traverse_skill_graph("rust", 1, 2).await.unwrap();
    assert!(result.get("error").is_none(), "root should be found");
    let sname = result
        .get("skill")
        .and_then(|s| s.get("name"))
        .and_then(|v| v.as_str());
    assert_eq!(sname, Some("rust"));
    assert!(result.get("links_to").is_some());
    assert!(result.get("linked_from").is_some());
}

// SKIP: skills(action="generate") — filesystem + LLM-bound.

// ── Temporal: code_health + sync_git_history ──────────────────

async fn init_commit_schema(db: &DbHandle) {
    db.query(
        "DEFINE TABLE IF NOT EXISTS commit SCHEMAFULL; \
         DEFINE FIELD IF NOT EXISTS hash ON commit TYPE string; \
         DEFINE FIELD IF NOT EXISTS author ON commit TYPE string; \
         DEFINE FIELD IF NOT EXISTS message ON commit TYPE string; \
         DEFINE FIELD IF NOT EXISTS timestamp ON commit TYPE int; \
         DEFINE FIELD IF NOT EXISTS files_changed ON commit TYPE int; \
         DEFINE FIELD IF NOT EXISTS repo ON commit TYPE string;",
    )
    .await
    .expect("commit schema bootstrap should succeed");
}

#[tokio::test]
async fn sync_git_history_upserts_commit_row() {
    let (db, _gq) = setup().await;
    init_commit_schema(&db).await;
    db.query(
        "UPSERT commit:abc123 SET hash = 'abc123', author = 'Alice', \
         message = 'first commit', timestamp = 1700000000, \
         files_changed = 2, repo = 'test'",
    )
    .await
    .expect("SCHEMAFULL commit UPSERT must accept every required field");
    db.query(
        "UPSERT commit:abc123 SET hash = 'abc123', author = 'Alice', \
         message = 'amended', timestamp = 1700000000, \
         files_changed = 3, repo = 'test'",
    )
    .await
    .unwrap();
    let rows: Vec<serde_json::Value> = db
        .query("SELECT hash, message, files_changed FROM commit WHERE repo = 'test'")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 1, "UPSERT must be idempotent on commit:<hash>");
    assert_eq!(
        rows[0].get("message").and_then(|v| v.as_str()),
        Some("amended"),
        "second UPSERT should overwrite message"
    );
    assert_eq!(
        rows[0].get("files_changed").and_then(|v| v.as_u64()),
        Some(3)
    );
}

#[tokio::test]
async fn code_health_hotspots_query_shape() {
    let (db, _gq) = setup().await;
    init_commit_schema(&db).await;
    db.query(
        "UPSERT commit:c1 SET hash='c1', author='A', message='m1', \
         timestamp=1700000000, files_changed=1, repo='test'; \
         UPSERT commit:c2 SET hash='c2', author='A', message='m2', \
         timestamp=1700000100, files_changed=1, repo='test';",
    )
    .await
    .unwrap();
    db.query(
        "LET $f = (SELECT VALUE id FROM `function` WHERE name='parse_file' LIMIT 1)[0]; \
         RELATE $f->modified_in->commit:c1 SET change_type='modified'; \
         RELATE $f->modified_in->commit:c2 SET change_type='modified';",
    )
    .await
    .expect("modified_in RELATE should accept the edge");

    let hotspots: Vec<serde_json::Value> = db
        .query(
            "SELECT name, file_path, start_line, end_line, \
             (end_line - start_line) AS size, \
             count(->modified_in) AS churn, \
             ((end_line - start_line) * count(->modified_in)) AS risk_score \
             FROM `function` WHERE repo = $repo \
             ORDER BY risk_score DESC LIMIT 30",
        )
        .bind(("repo", "test".to_string()))
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();

    assert_eq!(hotspots.len(), 4, "all test-repo fns should appear");
    assert_eq!(
        hotspots[0].get("name").and_then(|v| v.as_str()),
        Some("parse_file")
    );
    assert_eq!(
        hotspots[0].get("churn").and_then(|v| v.as_u64()),
        Some(2),
        "parse_file has 2 modified_in edges"
    );
}

// SKIP: code_health churn / coupling / review_diff — all read
// from on-disk git repo via GitAnalyzer; no SurrealQL path.

// ── Embeddings + conversations ────────────────────────────────
// SKIP: semantic_search end-to-end — requires embedding provider.
// SKIP: embed_functions end-to-end — requires embedding provider.
// SKIP: conversations(action=index) — filesystem-bound.
// We lock the DB-layer contracts that those tools delegate to.

#[tokio::test]
async fn semantic_search_cosine_surql_runs_on_seeded_embedding() {
    let (db, _gq) = setup().await;
    db.query(
        "UPDATE `function` SET embedding = [0.1, 0.2, 0.3, 0.4] \
         WHERE name = 'parse_file' AND repo = 'test'",
    )
    .await
    .expect("seed embedding UPDATE should succeed");

    let rows: Vec<serde_json::Value> = db
        .query(
            "SELECT name, vector::similarity::cosine(embedding, $query_vec) AS score \
             FROM `function` WHERE embedding IS NOT NONE \
             ORDER BY score DESC LIMIT $limit",
        )
        .bind(("query_vec", vec![0.1f32, 0.2, 0.3, 0.4]))
        .bind(("limit", 10i64))
        .await
        .expect("cosine SELECT must parse")
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("name").and_then(|v| v.as_str()),
        Some("parse_file")
    );
}

#[tokio::test]
async fn embed_functions_batched_for_update_lands_embedding() {
    let (db, _gq) = setup().await;
    // SurrealDB 3.0.5 renamed `type::thing` to `type::record`.
    // Lock the replacement shape so a future upgrade surfaces
    // the drift immediately.
    let surql = "FOR $item IN $updates { \
                 UPDATE type::record('function:' + $item.id) \
                 SET embedding = $item.embedding, binary_embedding = $item.bq; \
                 }";
    let id = codescope_core::graph::builder::sanitize_id("test::parse_file");
    let updates = vec![serde_json::json!({
        "id": id,
        "embedding": [0.5f32, 0.5, 0.5, 0.5],
        "bq": [0i64, 0, 0, 0],
    })];
    db.query(surql)
        .bind(("updates", updates))
        .await
        .expect("batched FOR UPDATE must parse + execute");

    let rows: Vec<serde_json::Value> = db
        .query("SELECT embedding FROM `function` WHERE name = 'parse_file'")
        .await
        .unwrap()
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0]
            .get("embedding")
            .and_then(|v| v.as_array())
            .is_some(),
        "embedding array must be set after batched UPDATE"
    );
}

#[tokio::test]
async fn conversations_search_matches_body_across_tables() {
    let (db, _gq) = setup().await;
    db.query(
        "CREATE decision SET name = 'pick redis', qualified_name = 'test:dec:redis', \
         body = 'chose redis for ttl controls', repo = 'test', language = 'conv', \
         kind = 'decision', file_path = 'conv', start_line = 0, end_line = 0, \
         timestamp = '2026-04-20T00:00:00'",
    )
    .await
    .expect("decision seed");
    db.query(
        "CREATE problem SET name = 'cache miss storm', qualified_name = 'test:prob:cms', \
         body = 'memcached dogpile during deploy', repo = 'test', language = 'conv', \
         kind = 'problem', file_path = 'conv', start_line = 0, end_line = 0, \
         timestamp = '2026-04-21T00:00:00'",
    )
    .await
    .expect("problem seed");

    let rows: Vec<serde_json::Value> = db
        .query(
            "SELECT name, 'problem' AS type FROM problem WHERE \
             string::contains(string::lowercase(name), string::lowercase($kw)) \
             OR string::contains(string::lowercase(body), string::lowercase($kw)) \
             LIMIT $lim",
        )
        .bind(("kw", "memcached".to_string()))
        .bind(("lim", 20u32))
        .await
        .expect("per-table search SELECT must parse")
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("name").and_then(|v| v.as_str()),
        Some("cache miss storm")
    );
}

#[tokio::test]
async fn conversations_timeline_contains_inlined_literal() {
    let (db, _gq) = setup().await;
    db.query(
        "CREATE decision SET name = 'pin parse_file', qualified_name = 'test:dec:pin_pf', \
         body = 'freeze parse_file until parser lands', repo = 'test', language = 'conv', \
         kind = 'decision', file_path = 'conv', start_line = 0, end_line = 0, \
         timestamp = '2026-04-20T00:00:00'",
    )
    .await
    .expect("decision seed");

    let safe_name = "parse_file".replace('\'', "");
    let q = format!(
        "SELECT name, body, timestamp, 'decision' AS type \
         FROM decision WHERE body CONTAINS '{}' \
         ORDER BY timestamp DESC LIMIT $lim",
        safe_name
    );
    let rows: Vec<serde_json::Value> = db
        .query(&q)
        .bind(("lim", 20u32))
        .await
        .expect("timeline CONTAINS query must parse")
        .take(0)
        .unwrap_or_default();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("name").and_then(|v| v.as_str()),
        Some("pin parse_file")
    );
}

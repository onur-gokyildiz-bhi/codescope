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
use codescope_core::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;

async fn setup() -> (Surreal<surrealdb::engine::local::Db>, GraphQuery) {
    let db = Surreal::new::<Mem>(()).await.unwrap();
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
    assert_eq!(
        results[0].file_path.as_deref(),
        Some("src/parser.rs")
    );
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
    let names: Vec<&str> = callees
        .iter()
        .filter_map(|c| c.name.as_deref())
        .collect();
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

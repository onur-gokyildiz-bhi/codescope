use codescope_core::graph::builder::GraphBuilder;
use codescope_core::graph::schema::init_schema;
/// Database integration tests for SurrealDB operations.
/// Post R1-v2 (v0.8.0) the graph helpers consume
/// `DbHandle = Surreal<Any>`, so tests use
/// `engine::any::connect("memory")` — same wire shape as the
/// bundled server, zero-persistence.
use codescope_core::{CodeEntity, CodeRelation, DbHandle, EntityKind, RelationKind};
use serde::Deserialize;
use surrealdb::engine::any;
use surrealdb::types::SurrealValue;

#[derive(Debug, Deserialize, SurrealValue)]
struct NameRow {
    name: String,
}

async fn setup_db() -> DbHandle {
    let db = any::connect("memory")
        .await
        .expect("Failed to create in-memory DB");
    db.use_ns("codescope")
        .use_db("test")
        .await
        .expect("Failed to set namespace");
    init_schema(&db).await.expect("Failed to init schema");
    db
}

fn make_entity(kind: EntityKind, name: &str, file: &str) -> CodeEntity {
    CodeEntity {
        kind,
        name: name.to_string(),
        qualified_name: format!("test::{}::{}", file, name),
        file_path: file.to_string(),
        repo: "test-repo".to_string(),
        start_line: 1,
        end_line: 10,
        start_col: 0,
        end_col: 0,
        signature: Some(format!("fn {}()", name)),
        body: Some("{ /* test */ }".to_string()),
        body_hash: Some("abc123".to_string()),
        language: "rust".to_string(),
        cuda_qualifier: None,
    }
}

fn make_relation(from: &str, to: &str, kind: RelationKind) -> CodeRelation {
    CodeRelation {
        from_entity: from.to_string(),
        to_entity: to.to_string(),
        kind,
        from_table: "function".to_string(),
        to_table: "function".to_string(),
        metadata: None,
    }
}

#[tokio::test]
async fn schema_init_is_idempotent() {
    let db = setup_db().await;
    init_schema(&db)
        .await
        .expect("Second schema init should be idempotent");
}

#[tokio::test]
async fn insert_and_query_entities() {
    let db = setup_db().await;
    let builder = GraphBuilder::new(db.clone());

    let entities = vec![
        make_entity(EntityKind::Function, "hello", "src/main.rs"),
        make_entity(EntityKind::Function, "world", "src/main.rs"),
        make_entity(EntityKind::Struct, "Config", "src/config.rs"),
    ];

    let count = builder
        .insert_entities(&entities)
        .await
        .expect("Insert should succeed");
    assert_eq!(count, 3);

    let result: Vec<NameRow> = db
        .query("SELECT name FROM `function`")
        .await
        .expect("Query should succeed")
        .take(0)
        .expect("Should get results");
    assert_eq!(result.len(), 2, "Should have 2 functions");
}

#[tokio::test]
async fn insert_and_query_relations() {
    let db = setup_db().await;
    let builder = GraphBuilder::new(db.clone());

    let entities = vec![
        make_entity(EntityKind::Function, "caller", "src/a.rs"),
        make_entity(EntityKind::Function, "callee", "src/b.rs"),
    ];
    builder.insert_entities(&entities).await.unwrap();

    let relations = vec![make_relation(
        "test::src/a.rs::caller",
        "test::src/b.rs::callee",
        RelationKind::Calls,
    )];
    let count = builder
        .insert_relations(&relations)
        .await
        .expect("Insert relations should succeed");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn upsert_preserves_existing_data() {
    let db = setup_db().await;
    let builder = GraphBuilder::new(db.clone());

    let entity = make_entity(EntityKind::Function, "evolve", "src/lib.rs");
    builder.insert_entities(&[entity.clone()]).await.unwrap();

    let mut updated = entity;
    updated.body = Some("{ /* updated */ }".to_string());
    updated.body_hash = Some("def456".to_string());
    let count = builder.insert_entities(&[updated]).await.unwrap();
    assert_eq!(count, 1);

    let result: Vec<NameRow> = db
        .query("SELECT name FROM `function` WHERE name = 'evolve'")
        .await
        .unwrap()
        .take(0)
        .unwrap();
    assert_eq!(result.len(), 1, "Upsert should not create duplicates");
}

#[tokio::test]
async fn empty_insert_is_noop() {
    let db = setup_db().await;
    let builder = GraphBuilder::new(db.clone());

    let count = builder.insert_entities(&[]).await.unwrap();
    assert_eq!(count, 0);

    let count = builder.insert_relations(&[]).await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn concurrent_inserts_dont_corrupt() {
    let db = setup_db().await;

    let mut handles = vec![];
    for i in 0..5 {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let builder = GraphBuilder::new(db);
            let entities = (0..10)
                .map(|j| {
                    make_entity(
                        EntityKind::Function,
                        &format!("fn_{}_{}", i, j),
                        &format!("src/batch_{}.rs", i),
                    )
                })
                .collect::<Vec<_>>();
            builder.insert_entities(&entities).await.unwrap()
        }));
    }

    let mut total = 0;
    for h in handles {
        total += h.await.unwrap();
    }
    assert_eq!(total, 50, "All 50 entities should be inserted");
}

#[tokio::test]
async fn test_all_entity_tables_exist() {
    let db = setup_db().await;
    let tables = [
        "file",
        "`function`",
        "class",
        "module",
        "variable",
        "import_decl",
        "config",
        "doc",
        "api",
        "db_entity",
        "infra",
        "package",
        "skill",
        "http_call",
        "conversation",
        "conv_topic",
        "decision",
        "problem",
        "solution",
    ];
    for table in tables {
        let query = format!("SELECT count() AS cnt FROM {} GROUP ALL", table);
        let result: Vec<serde_json::Value> = db
            .query(&query)
            .await
            .unwrap_or_else(|e| panic!("Table {} should exist: {}", table, e))
            .take(0)
            .unwrap_or_default();
        // Table exists if query doesn't error (count may be 0)
        assert!(
            result.len() <= 1,
            "Table {} query should return 0 or 1 row",
            table
        );
    }
}

#[tokio::test]
async fn test_all_edge_tables_exist() {
    let db = setup_db().await;
    let edges = [
        "contains",
        "calls",
        "imports",
        "inherits",
        "implements",
        "uses",
        "modified_in",
        "depends_on",
        "configures",
        "defines_endpoint",
        "has_field",
        "references",
        "depends_on_package",
        "runs_script",
        "discussed_in",
        "decided_about",
        "solves_for",
        "co_discusses",
        "links_to",
        "calls_endpoint",
    ];
    for edge in edges {
        let query = format!("SELECT count() AS cnt FROM {} GROUP ALL", edge);
        db.query(&query)
            .await
            .unwrap_or_else(|e| panic!("Edge table {} should exist: {}", edge, e));
    }
}

#[tokio::test]
async fn test_schema_idempotent_with_data() {
    let db = setup_db().await;
    let builder = GraphBuilder::new(db.clone());

    // Insert some data
    let entities = vec![make_entity(EntityKind::Function, "test_fn", "src/main.rs")];
    builder.insert_entities(&entities).await.unwrap();

    // Re-run schema init — should NOT destroy data
    init_schema(&db).await.unwrap();

    // Data should still be there
    let result: Vec<NameRow> = db
        .query("SELECT name FROM `function` WHERE name = 'test_fn'")
        .await
        .unwrap()
        .take(0)
        .unwrap();
    assert_eq!(result.len(), 1, "Data should survive schema re-init");
}

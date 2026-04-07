use codescope_core::graph::builder::GraphBuilder;
use codescope_core::graph::schema::init_schema;
/// Database integration tests for SurrealDB operations.
/// Tests schema init, entity insertion, querying, and concurrent access.
use codescope_core::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use serde::Deserialize;
use surrealdb::engine::local::Mem;
use surrealdb::types::SurrealValue;
use surrealdb::Surreal;

#[derive(Debug, Deserialize, SurrealValue)]
struct NameRow {
    name: String,
}

async fn setup_db() -> Surreal<surrealdb::engine::local::Db> {
    let db = Surreal::new::<Mem>(())
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

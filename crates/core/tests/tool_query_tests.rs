//! Integration tests for MCP tool query functions.
//! Tests the GraphQuery methods that power the MCP tools.

use codescope_core::graph::builder::GraphBuilder;
use codescope_core::graph::query::GraphQuery;
use codescope_core::graph::schema::init_schema;
use codescope_core::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;

async fn setup() -> (
    Surreal<surrealdb::engine::local::Db>,
    GraphBuilder,
    GraphQuery,
) {
    let db = Surreal::new::<Mem>(()).await.unwrap();
    db.use_ns("codescope").use_db("test").await.unwrap();
    init_schema(&db).await.unwrap();
    let builder = GraphBuilder::new(db.clone());
    let query = GraphQuery::new(db.clone());
    (db, builder, query)
}

fn make_fn(name: &str, file: &str, sig: &str) -> CodeEntity {
    CodeEntity {
        kind: EntityKind::Function,
        name: name.to_string(),
        qualified_name: format!("test::{}::{}", file, name),
        file_path: file.to_string(),
        repo: "test-repo".to_string(),
        start_line: 1,
        end_line: 20,
        start_col: 0,
        end_col: 0,
        signature: Some(sig.to_string()),
        body: Some("{ /* body */ }".to_string()),
        body_hash: Some("hash123".to_string()),
        language: "rust".to_string(),
    }
}

fn make_class(name: &str, file: &str) -> CodeEntity {
    CodeEntity {
        kind: EntityKind::Class,
        name: name.to_string(),
        qualified_name: format!("test::{}::{}", file, name),
        file_path: file.to_string(),
        repo: "test-repo".to_string(),
        start_line: 1,
        end_line: 50,
        start_col: 0,
        end_col: 0,
        signature: Some(format!("struct {}", name)),
        body: None,
        body_hash: None,
        language: "rust".to_string(),
    }
}

fn make_call(from: &str, to: &str) -> CodeRelation {
    CodeRelation {
        kind: RelationKind::Calls,
        from_entity: from.to_string(),
        to_entity: to.to_string(),
        from_table: "function".to_string(),
        to_table: "function".to_string(),
        metadata: None,
    }
}

async fn seed_data(builder: &GraphBuilder) {
    let entities = vec![
        make_fn(
            "parse_source",
            "src/parser.rs",
            "pub fn parse_source(path: &Path) -> Result<Vec<Entity>>",
        ),
        make_fn(
            "extract_entities",
            "src/parser.rs",
            "fn extract_entities(tree: &Tree) -> Vec<Entity>",
        ),
        make_fn(
            "build_graph",
            "src/graph.rs",
            "pub async fn build_graph(entities: &[Entity]) -> Result<()>",
        ),
        make_fn(
            "find_callers",
            "src/query.rs",
            "pub async fn find_callers(name: &str) -> Result<Vec<SearchResult>>",
        ),
        make_fn("main", "src/main.rs", "fn main()"),
        make_class("GraphBuilder", "src/graph.rs"),
        make_class("CodeParser", "src/parser.rs"),
    ];
    builder.insert_entities(&entities).await.unwrap();

    let relations = vec![
        make_call(
            "test::src/main.rs::main",
            "test::src/parser.rs::parse_source",
        ),
        make_call("test::src/main.rs::main", "test::src/graph.rs::build_graph"),
        make_call(
            "test::src/parser.rs::parse_source",
            "test::src/parser.rs::extract_entities",
        ),
        make_call(
            "test::src/graph.rs::build_graph",
            "test::src/query.rs::find_callers",
        ),
    ];
    builder.insert_relations(&relations).await.unwrap();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// search_functions("parse") should find parse_source
#[tokio::test]
async fn test_search_functions() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    let results = query.search_functions("parse").await.unwrap();
    assert!(!results.is_empty(), "Should find at least one match");
    let names: Vec<&str> = results.iter().filter_map(|r| r.name.as_deref()).collect();
    assert!(
        names.contains(&"parse_source"),
        "Should find parse_source, got: {:?}",
        names
    );
}

/// find_callers("parse_source") should return main
#[tokio::test]
async fn test_find_callers() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    let results = query.find_callers("parse_source").await.unwrap();
    assert!(!results.is_empty(), "parse_source should have callers");
    let names: Vec<&str> = results.iter().filter_map(|r| r.name.as_deref()).collect();
    assert!(
        names.contains(&"main"),
        "main should call parse_source, got callers: {:?}",
        names
    );
}

/// find_callees("main") should return parse_source and build_graph
#[tokio::test]
async fn test_find_callees() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    let results = query.find_callees("main").await.unwrap();
    assert!(
        results.len() >= 2,
        "main should have at least 2 callees, got {}",
        results.len()
    );
    let names: Vec<&str> = results.iter().filter_map(|r| r.name.as_deref()).collect();
    assert!(
        names.contains(&"parse_source"),
        "main should call parse_source, got: {:?}",
        names
    );
    assert!(
        names.contains(&"build_graph"),
        "main should call build_graph, got: {:?}",
        names
    );
}

/// explore("build_graph") should return entity info + callers + callees
#[tokio::test]
async fn test_explore_function() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    let result = query.explore("build_graph").await.unwrap();
    let obj = result.as_object().expect("explore should return an object");

    assert_eq!(
        obj.get("entity_type").and_then(|v| v.as_str()),
        Some("function"),
        "build_graph should be found as a function"
    );
    assert!(
        obj.contains_key("matches"),
        "Should contain matches: {:?}",
        obj.keys().collect::<Vec<_>>()
    );

    // Check callers
    let called_by = obj
        .get("called_by")
        .and_then(|v| v.as_array())
        .expect("Should have called_by");
    let caller_names: Vec<&str> = called_by
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(
        caller_names.contains(&"main"),
        "build_graph should be called by main, got: {:?}",
        caller_names
    );

    // Check callees
    let calls_to = obj
        .get("calls_to")
        .and_then(|v| v.as_array())
        .expect("Should have calls_to");
    let callee_names: Vec<&str> = calls_to
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(
        callee_names.contains(&"find_callers"),
        "build_graph should call find_callers, got: {:?}",
        callee_names
    );
}

/// explore("GraphBuilder") should return class info
#[tokio::test]
async fn test_explore_class() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    let result = query.explore("GraphBuilder").await.unwrap();
    let obj = result.as_object().expect("explore should return an object");

    assert_eq!(
        obj.get("entity_type").and_then(|v| v.as_str()),
        Some("class"),
        "GraphBuilder should be found as a class"
    );
    assert!(obj.contains_key("matches"), "Should contain matches");

    let matches = obj.get("matches").and_then(|v| v.as_array()).unwrap();
    assert!(
        !matches.is_empty(),
        "Should have at least one match for GraphBuilder"
    );
    let first = &matches[0];
    assert_eq!(
        first.get("name").and_then(|v| v.as_str()),
        Some("GraphBuilder")
    );
}

/// find_unused_symbols(1) should find functions with 0 callers.
/// extract_entities and find_callers are never called by any other seeded function
/// (except parse_source calls extract_entities and build_graph calls find_callers).
/// "main" is excluded by the filter. So unused = functions not in the target of any calls edge.
/// Actually: parse_source is called by main, extract_entities is called by parse_source,
/// build_graph is called by main, find_callers is called by build_graph. So all have callers
/// except main (which is excluded by name filter). None should be unused.
/// But the query uses `name NOT IN (SELECT VALUE out.name FROM calls WHERE out.name != NONE)`.
/// The calls edge out.name gives us: parse_source, build_graph, extract_entities, find_callers.
/// So unused = functions whose name is NOT in that set and not excluded = none (main is excluded).
/// Let's add a truly unused function.
#[tokio::test]
async fn test_find_unused_symbols() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    // Add a truly unused function (not called by anything, not excluded by name filters)
    let unused_fn = make_fn(
        "orphan_helper",
        "src/utils.rs",
        "fn orphan_helper() -> bool",
    );
    builder.insert_entities(&[unused_fn]).await.unwrap();

    let results = query.find_unused_symbols(1).await.unwrap();
    let names: Vec<&str> = results
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(
        names.contains(&"orphan_helper"),
        "orphan_helper should appear as unused, got: {:?}",
        names
    );
    // "main" should NOT appear (excluded by name filter)
    assert!(
        !names.contains(&"main"),
        "main should be excluded from unused symbols"
    );
}

/// type_hierarchy("GraphBuilder", 2) should return at minimum the entity itself
#[tokio::test]
async fn test_type_hierarchy() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    let result = query.type_hierarchy("GraphBuilder", 2).await.unwrap();
    let obj = result
        .as_object()
        .expect("type_hierarchy should return an object");

    assert_eq!(
        obj.get("name").and_then(|v| v.as_str()),
        Some("GraphBuilder"),
        "Should have the queried name"
    );
    assert_eq!(
        obj.get("depth").and_then(|v| v.as_u64()),
        Some(0),
        "Root depth should be 0"
    );
    // Entity should be present
    assert!(
        obj.contains_key("entity"),
        "Should contain entity info, got keys: {:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

/// detect_circular_deps("test-repo") should return empty (no cycles in test data)
#[tokio::test]
async fn test_detect_circular_deps() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    let results = query.detect_circular_deps("test-repo").await.unwrap();
    assert!(
        results.is_empty(),
        "Test data has no circular deps, got: {:?}",
        results
    );
}

/// find_duplicate_functions("test-repo") — all seeded functions share body_hash "hash123"
/// so they should be reported as duplicates
#[tokio::test]
async fn test_find_duplicate_functions() {
    let (_db, builder, query) = setup().await;
    seed_data(&builder).await;

    let results = query.find_duplicate_functions("test-repo").await.unwrap();
    assert!(
        !results.is_empty(),
        "Functions with same body_hash should be flagged as duplicates"
    );

    let first = &results[0];
    let cnt = first.get("cnt").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(
        cnt > 1,
        "Duplicate group should have count > 1, got {}",
        cnt
    );

    let hash = first
        .get("body_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(hash, "hash123", "Duplicate hash should be hash123");
}

/// health_check() should succeed on a valid in-memory DB
#[tokio::test]
async fn test_health_check() {
    let (_db, _builder, query) = setup().await;

    query
        .health_check()
        .await
        .expect("Health check should succeed on a valid DB");
}

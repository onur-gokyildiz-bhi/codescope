/// Test conversation indexing against a real Claude Code JSONL transcript.
/// This test is ignored by default — run with:
///   CODESCOPE_TEST_JSONL_DIR=/path/to/jsonl/dir cargo test real_conv -- --ignored --nocapture

fn get_test_jsonl_dir() -> Option<std::path::PathBuf> {
    std::env::var("CODESCOPE_TEST_JSONL_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
}

#[test]
#[ignore]
fn real_conv_large_jsonl() {
    use codescope_core::conversation::parse_conversation;
    use std::time::Instant;

    let dir = match get_test_jsonl_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: Set CODESCOPE_TEST_JSONL_DIR to a directory containing .jsonl files");
            return;
        }
    };

    // Find the largest .jsonl file in the directory
    let jsonl_path = match std::fs::read_dir(&dir).ok().and_then(|entries| {
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
            .max_by_key(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
            .map(|e| e.path())
    }) {
        Some(p) => p,
        None => {
            println!("SKIP: No .jsonl files found in {:?}", dir);
            return;
        }
    };

    let file_size = std::fs::metadata(&jsonl_path).unwrap().len();
    println!(
        "Testing with real JSONL: {:.1}MB ({} bytes)",
        file_size as f64 / 1_000_000.0,
        file_size
    );

    let known_entities = vec![
        "function:insert_entities:codescope_core::graph::builder::insert_entities".to_string(),
        "function:insert_relations:codescope_core::graph::builder::insert_relations".to_string(),
        "function:parse_source:codescope_core::parser::parse_source".to_string(),
        "class:GraphBuilder:codescope_core::graph::builder::GraphBuilder".to_string(),
        "class:GraphQuery:codescope_core::graph::query::GraphQuery".to_string(),
        "class:CodeParser:codescope_core::parser::CodeParser".to_string(),
        "file:builder.rs:crates/core/src/graph/builder.rs".to_string(),
        "file:schema.rs:crates/core/src/graph/schema.rs".to_string(),
        "file:query.rs:crates/core/src/graph/query.rs".to_string(),
        "file:server.rs:crates/mcp-server/src/server.rs".to_string(),
        "file:mod.rs:crates/core/src/parser/mod.rs".to_string(),
    ];

    let start = Instant::now();
    let (entities, relations, result) =
        parse_conversation(&jsonl_path, "graph-rag", &known_entities).unwrap();
    let elapsed = start.elapsed();

    println!("\n=== Conversation Indexing Results ===");
    println!("Parse time: {:.1}ms", elapsed.as_millis());
    println!("Sessions: {}", result.sessions_indexed);
    println!("Total entities: {}", entities.len());
    println!("Total relations: {}", relations.len());
    println!("  Decisions: {}", result.decisions);
    println!("  Problems: {}", result.problems);
    println!("  Solutions: {}", result.solutions);
    println!("  Topics: {}", result.topics);
    println!("  Code links: {}", result.code_links);

    println!("\n--- First 10 Classified Segments ---");
    for entity in entities
        .iter()
        .filter(|e| e.kind != codescope_core::EntityKind::ConversationSession)
        .take(10)
    {
        println!(
            "[{:?}] {} (line {})",
            entity.kind, entity.name, entity.start_line
        );
        if let Some(body) = &entity.body {
            let preview = if body.len() > 150 {
                &body[..150]
            } else {
                body.as_str()
            };
            println!("  body: {}", preview);
        }
    }

    println!("\n--- Code Links ---");
    for rel in relations
        .iter()
        .filter(|r| {
            r.kind == codescope_core::RelationKind::DiscussedIn
                || r.kind == codescope_core::RelationKind::DecidedAbout
        })
        .take(10)
    {
        println!("[{:?}] {} -> {}", rel.kind, rel.from_entity, rel.to_entity);
    }

    println!("\n--- Solution->Problem Links ---");
    for rel in relations
        .iter()
        .filter(|r| r.kind == codescope_core::RelationKind::SolvesFor)
        .take(10)
    {
        println!("{} -> {}", rel.from_entity, rel.to_entity);
    }

    assert!(
        result.sessions_indexed == 1,
        "Should index exactly 1 session"
    );
    assert!(
        entities.len() >= 5,
        "Real conversation should produce at least 5 entities, got {}",
        entities.len()
    );
    assert!(
        result.problems + result.decisions + result.solutions >= 3,
        "Real conversation should find multiple classified segments"
    );
    assert!(
        elapsed.as_secs() < 10,
        "Should parse 10MB JSONL in under 10 seconds"
    );
}

#[test]
#[ignore]
fn real_conv_small_jsonl() {
    use codescope_core::conversation::parse_conversation;

    let dir = match get_test_jsonl_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: Set CODESCOPE_TEST_JSONL_DIR to a directory containing .jsonl files");
            return;
        }
    };

    // Find the smallest .jsonl file
    let jsonl_path = match std::fs::read_dir(&dir).ok().and_then(|entries| {
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
            .min_by_key(|e| e.metadata().map(|m| m.len()).unwrap_or(u64::MAX))
            .map(|e| e.path())
    }) {
        Some(p) => p,
        None => {
            println!("SKIP: No .jsonl files found in {:?}", dir);
            return;
        }
    };

    let (entities, relations, result) = parse_conversation(&jsonl_path, "graph-rag", &[]).unwrap();

    println!("\n=== Small Session Results ===");
    println!(
        "Entities: {}, Relations: {}",
        entities.len(),
        relations.len()
    );
    println!(
        "Decisions: {}, Problems: {}, Solutions: {}, Topics: {}",
        result.decisions, result.problems, result.solutions, result.topics
    );

    for entity in entities
        .iter()
        .filter(|e| e.kind != codescope_core::EntityKind::ConversationSession)
    {
        println!("[{:?}] {}", entity.kind, entity.name);
    }

    assert!(result.sessions_indexed == 1);
}

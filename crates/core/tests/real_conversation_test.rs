/// Test conversation indexing against a real Claude Code JSONL transcript.
/// This test is ignored by default — run with: cargo test real_conv -- --ignored --nocapture

#[test]
#[ignore]
fn real_conv_large_jsonl() {
    use codescope_core::conversation::parse_conversation;
    use std::path::Path;
    use std::time::Instant;

    let jsonl_path = Path::new("C:/Users/onurg/.claude/projects/C--Users-onurg-OneDrive-Documents-graph-rag/f3c19537-a153-4e6a-9647-784912d5ceb2.jsonl");

    if !jsonl_path.exists() {
        println!("SKIP: Large JSONL file not found at {:?}", jsonl_path);
        return;
    }

    let file_size = std::fs::metadata(jsonl_path).unwrap().len();
    println!("Testing with real JSONL: {:.1}MB ({} bytes)", file_size as f64 / 1_000_000.0, file_size);

    // Simulate known entities from an indexed codebase
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
    let (entities, relations, result) = parse_conversation(jsonl_path, "graph-rag", &known_entities).unwrap();
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

    // Print first 10 entities (not session)
    println!("\n--- First 10 Classified Segments ---");
    for entity in entities.iter().filter(|e| e.kind != codescope_core::EntityKind::ConversationSession).take(10) {
        println!("[{:?}] {} (line {})", entity.kind, entity.name, entity.start_line);
        if let Some(body) = &entity.body {
            let preview = if body.len() > 150 { &body[..150] } else { body.as_str() };
            println!("  body: {}", preview);
        }
    }

    // Print code link relations
    println!("\n--- Code Links ---");
    for rel in relations.iter().filter(|r| {
        r.kind == codescope_core::RelationKind::DiscussedIn || r.kind == codescope_core::RelationKind::DecidedAbout
    }).take(10) {
        println!("[{:?}] {} -> {}", rel.kind, rel.from_entity, rel.to_entity);
    }

    // Print solution-to-problem links
    println!("\n--- Solution→Problem Links ---");
    for rel in relations.iter().filter(|r| r.kind == codescope_core::RelationKind::SolvesFor).take(10) {
        println!("{} -> {}", rel.from_entity, rel.to_entity);
    }

    // Sanity checks
    assert!(result.sessions_indexed == 1, "Should index exactly 1 session");
    assert!(entities.len() >= 5, "Real conversation should produce at least 5 entities, got {}", entities.len());
    assert!(result.problems + result.decisions + result.solutions >= 3,
        "Real conversation should find multiple classified segments");
    assert!(elapsed.as_secs() < 10, "Should parse 10MB JSONL in under 10 seconds");
}

#[test]
#[ignore]
fn real_conv_small_jsonl() {
    use codescope_core::conversation::parse_conversation;
    use std::path::Path;

    let jsonl_path = Path::new("C:/Users/onurg/.claude/projects/C--Users-onurg-OneDrive-Documents-graph-rag/593845ec-7e80-4f35-9411-ccd16be2c0ea.jsonl");

    if !jsonl_path.exists() {
        println!("SKIP: Small JSONL file not found");
        return;
    }

    let (entities, relations, result) = parse_conversation(jsonl_path, "graph-rag", &[]).unwrap();

    println!("\n=== Small Session Results ===");
    println!("Entities: {}, Relations: {}", entities.len(), relations.len());
    println!("Decisions: {}, Problems: {}, Solutions: {}, Topics: {}",
        result.decisions, result.problems, result.solutions, result.topics);

    for entity in entities.iter().filter(|e| e.kind != codescope_core::EntityKind::ConversationSession) {
        println!("[{:?}] {}", entity.kind, entity.name);
    }

    assert!(result.sessions_indexed == 1);
}

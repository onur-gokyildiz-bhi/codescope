use codescope_core::parser::CodeParser;
use codescope_core::{EntityKind, RelationKind};
use std::io::Write;
use tempfile::NamedTempFile;

// =====================================================
// Parser Tests
// =====================================================

#[test]
fn test_language_registry_has_19_languages() {
    let parser = CodeParser::new();
    let langs = parser.supported_languages();
    // 19 tree-sitter languages + 9 content parsers
    assert!(langs.len() >= 19, "Expected at least 19 languages, got {}", langs.len());
}

#[test]
fn test_supports_rust_extension() {
    let parser = CodeParser::new();
    assert!(parser.supports_extension("rs"));
    assert!(parser.supports_extension("py"));
    assert!(parser.supports_extension("ts"));
    assert!(parser.supports_extension("go"));
    assert!(parser.supports_extension("java"));
    assert!(parser.supports_extension("swift"));
    assert!(parser.supports_extension("dart"));
    assert!(parser.supports_extension("zig"));
    assert!(parser.supports_extension("hs"));
    assert!(parser.supports_extension("lua"));
    assert!(parser.supports_extension("scala"));
    assert!(parser.supports_extension("ex"));
}

#[test]
fn test_supports_content_extensions() {
    let parser = CodeParser::new();
    assert!(parser.supports_extension("json"));
    assert!(parser.supports_extension("yaml"));
    assert!(parser.supports_extension("yml"));
    assert!(parser.supports_extension("toml"));
    assert!(parser.supports_extension("md"));
    assert!(parser.supports_extension("sql"));
    assert!(parser.supports_extension("tf"));
}

#[test]
fn test_supports_filenames() {
    let parser = CodeParser::new();
    assert!(parser.supports_filename("Dockerfile"));
    assert!(parser.supports_filename("package.json"));
    assert!(parser.supports_filename("Cargo.toml"));
}

#[test]
fn test_unsupported_extension() {
    let parser = CodeParser::new();
    assert!(!parser.supports_extension("xyz"));
    assert!(!parser.supports_extension("png"));
    assert!(!parser.supports_extension("exe"));
}

// =====================================================
// Rust Parsing Tests
// =====================================================

#[test]
fn test_parse_rust_function() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".rs").unwrap();
    write!(file, "fn hello() {{\n    println!(\"hi\");\n}}\n").unwrap();

    let (entities, _relations) = parser.parse_file(file.path(), "test-repo").unwrap();

    assert!(entities.iter().any(|e| e.kind == EntityKind::File), "Should have File entity");
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::Function && e.name == "hello"),
        "Should have function 'hello'"
    );
}

#[test]
fn test_parse_rust_struct() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".rs").unwrap();
    write!(file, "pub struct MyStruct {{\n    pub name: String,\n}}\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::Struct && e.name == "MyStruct"),
        "Should have struct 'MyStruct'"
    );
}

#[test]
fn test_parse_rust_enum() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".rs").unwrap();
    write!(file, "enum Color {{\n    Red,\n    Green,\n    Blue,\n}}\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::Enum && e.name == "Color"),
        "Should have enum 'Color'"
    );
}

#[test]
fn test_parse_rust_imports() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".rs").unwrap();
    write!(file, "use std::collections::HashMap;\nfn main() {{}}\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::Import),
        "Should have Import entity"
    );
}

#[test]
fn test_parse_rust_contains_relations() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".rs").unwrap();
    write!(file, "fn foo() {{}}\nfn bar() {{}}\n").unwrap();

    let (_entities, relations) = parser.parse_file(file.path(), "test-repo").unwrap();
    let contains = relations.iter().filter(|r| r.kind == RelationKind::Contains).count();
    assert!(contains >= 2, "Should have at least 2 Contains relations, got {}", contains);
}

// =====================================================
// TypeScript Parsing Tests
// =====================================================

#[test]
fn test_parse_typescript_function() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".ts").unwrap();
    write!(file, "function greet(name: string): string {{\n  return `Hello ${{name}}`;\n}}\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::Function && e.name == "greet"),
        "Should have function 'greet'"
    );
}

#[test]
fn test_parse_typescript_class() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".ts").unwrap();
    write!(file, "class UserService {{\n  getUser() {{ return null; }}\n}}\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::Class && e.name == "UserService"),
        "Should have class 'UserService'"
    );
}

// =====================================================
// Python Parsing Tests
// =====================================================

#[test]
fn test_parse_python_function() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".py").unwrap();
    write!(file, "def calculate(x, y):\n    return x + y\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::Function && e.name == "calculate"),
        "Should have function 'calculate'"
    );
}

#[test]
fn test_parse_python_class() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".py").unwrap();
    write!(file, "class Animal:\n    def speak(self):\n        pass\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::Class && e.name == "Animal"),
        "Should have class 'Animal'"
    );
}

// =====================================================
// Content Parser Tests
// =====================================================

#[test]
fn test_parse_json_config() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".json").unwrap();
    write!(file, r#"{{"database": {{"host": "localhost", "port": 5432}}}}"#).unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(entities.iter().any(|e| e.kind == EntityKind::File), "Should have File entity");
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::ConfigKey || e.kind == EntityKind::ConfigSection),
        "Should have config entities"
    );
}

#[test]
fn test_parse_markdown() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".md").unwrap();
    write!(file, "# Title\n\nSome text.\n\n## Section\n\n```rust\nfn main() {{}}\n```\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::DocSection),
        "Should have DocSection entities"
    );
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::DocCodeBlock),
        "Should have DocCodeBlock entity"
    );
}

#[test]
fn test_parse_dockerfile() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::new().unwrap();
    // Rename to Dockerfile won't work with NamedTempFile, so use .tf extension trick
    // Instead, we write a Dockerfile-like content to a temp dir
    let dir = tempfile::tempdir().unwrap();
    let dockerfile_path = dir.path().join("Dockerfile");
    std::fs::write(&dockerfile_path, "FROM rust:1.75 AS builder\nRUN cargo build\nFROM debian:bookworm-slim\nCOPY --from=builder /app /app\n").unwrap();

    let (entities, _) = parser.parse_file(&dockerfile_path, "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::DockerStage),
        "Should have DockerStage entity"
    );
    let _ = file; // suppress unused
}

#[test]
fn test_parse_sql() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".sql").unwrap();
    write!(file, "CREATE TABLE users (\n  id INT PRIMARY KEY,\n  name VARCHAR(100)\n);\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::DbTable && e.name == "users"),
        "Should have DbTable 'users'"
    );
}

#[test]
fn test_parse_yaml() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".yaml").unwrap();
    write!(file, "server:\n  host: localhost\n  port: 8080\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::ConfigKey || e.kind == EntityKind::ConfigSection),
        "Should have config entities"
    );
}

#[test]
fn test_parse_toml() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".toml").unwrap();
    write!(file, "[package]\nname = \"myapp\"\nversion = \"1.0.0\"\n").unwrap();

    let (entities, _) = parser.parse_file(file.path(), "test-repo").unwrap();
    assert!(
        entities.iter().any(|e| e.kind == EntityKind::ConfigKey || e.kind == EntityKind::ConfigSection),
        "Should have config entities"
    );
}

// =====================================================
// Relation Table Tests (Bug #3 fix verification)
// =====================================================

#[test]
fn test_relations_have_correct_tables() {
    let parser = CodeParser::new();
    let mut file = NamedTempFile::with_suffix(".rs").unwrap();
    write!(file, "fn caller() {{\n    callee();\n}}\nfn callee() {{}}\n").unwrap();

    let (_entities, relations) = parser.parse_file(file.path(), "test-repo").unwrap();

    for rel in &relations {
        assert!(!rel.from_table.is_empty(), "from_table should not be empty for {:?}", rel.kind);
        assert!(!rel.to_table.is_empty(), "to_table should not be empty for {:?}", rel.kind);
    }

    // Contains relations should have "file" as from_table
    let contains = relations.iter().filter(|r| r.kind == RelationKind::Contains).collect::<Vec<_>>();
    for rel in &contains {
        assert_eq!(rel.from_table, "file", "Contains should have from_table='file'");
    }
}

// =====================================================
// EntityKind Table Name Tests
// =====================================================

#[test]
fn test_entity_kind_table_names() {
    assert_eq!(EntityKind::File.table_name(), "file");
    assert_eq!(EntityKind::Function.table_name(), "function");
    assert_eq!(EntityKind::Method.table_name(), "function");
    assert_eq!(EntityKind::Class.table_name(), "class");
    assert_eq!(EntityKind::Struct.table_name(), "class");
    assert_eq!(EntityKind::Import.table_name(), "import_decl");
    assert_eq!(EntityKind::ConfigKey.table_name(), "config");
    assert_eq!(EntityKind::DocSection.table_name(), "doc");
    assert_eq!(EntityKind::ApiEndpoint.table_name(), "api");
    assert_eq!(EntityKind::DbTable.table_name(), "db_entity");
    assert_eq!(EntityKind::InfraResource.table_name(), "infra");
    assert_eq!(EntityKind::Package.table_name(), "package");
    assert_eq!(EntityKind::Dependency.table_name(), "package");
    assert_eq!(EntityKind::Script.table_name(), "package");
}

// =====================================================
// Incremental Indexing Hash Tests
// =====================================================

#[test]
fn test_content_hash_consistency() {
    use codescope_core::graph::incremental::hash_content;
    let hash1 = hash_content("fn main() {}");
    let hash2 = hash_content("fn main() {}");
    let hash3 = hash_content("fn main() { changed }");

    assert_eq!(hash1, hash2, "Same content should produce same hash");
    assert_ne!(hash1, hash3, "Different content should produce different hash");
}

// =====================================================
// Conversation Indexing Tests
// =====================================================

#[test]
fn test_conversation_parser_basic() {
    use codescope_core::conversation::parse_conversation;
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let jsonl_path = dir.path().join("test-session.jsonl");

    // Write a minimal JSONL conversation
    let mut f = std::fs::File::create(&jsonl_path).unwrap();
    // User message asking about a problem
    writeln!(f, r#"{{"type":"user","message":{{"role":"user","content":"The compile error in builder.rs — doesn't compile with the new changes"}},"timestamp":"2026-04-01T10:00:00Z","sessionId":"test-abc-123"}}"#).unwrap();
    // Assistant finds the problem
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"The problem is that the hash field on the file table requires TYPE string but content parsers create File entities with body_hash: None."}},"timestamp":"2026-04-01T10:01:00Z","sessionId":"test-abc-123"}}"#).unwrap();
    // Assistant proposes a solution
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"The fix is to change the schema from hash TYPE string to hash TYPE option<string>. I've updated schema.rs to use option<string> for the hash field."}},"timestamp":"2026-04-01T10:02:00Z","sessionId":"test-abc-123"}}"#).unwrap();
    // Assistant makes a decision
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"I decided to use UPSERT SET syntax instead of content JSON for all entity inserts. This avoids SurrealDB JSON parsing issues with nested quotes."}},"timestamp":"2026-04-01T10:03:00Z","sessionId":"test-abc-123"}}"#).unwrap();

    let known_entities = vec![
        "function:insert_entities:graph_rag::builder::insert_entities".to_string(),
        "file:schema.rs:graph_rag::schema".to_string(),
    ];

    let (entities, relations, result) = parse_conversation(&jsonl_path, "test-repo", &known_entities).unwrap();

    // Should have session + classified segments
    assert!(result.sessions_indexed == 1, "Should index 1 session");
    assert!(!entities.is_empty(), "Should produce entities, got {}", entities.len());
    assert!(result.problems >= 1, "Should find at least 1 problem, got {}", result.problems);
    assert!(result.solutions >= 1, "Should find at least 1 solution, got {}", result.solutions);
    assert!(result.decisions >= 1, "Should find at least 1 decision, got {}", result.decisions);

    // Session entity should exist
    let session = entities.iter().find(|e| e.kind == EntityKind::ConversationSession);
    assert!(session.is_some(), "Should have a ConversationSession entity");

    // Should have contains relations (session -> segments)
    let contains_rels: Vec<_> = relations.iter().filter(|r| r.kind == RelationKind::Contains).collect();
    assert!(!contains_rels.is_empty(), "Should have Contains relations from session to segments");

    // Solution-to-problem linking
    let solves_rels: Vec<_> = relations.iter().filter(|r| r.kind == RelationKind::SolvesFor).collect();
    assert!(!solves_rels.is_empty(), "Should have SolvesFor relation linking solution to problem");

    println!("Conversation indexing test results:");
    println!("  Entities: {}", entities.len());
    println!("  Relations: {}", relations.len());
    println!("  Decisions: {}", result.decisions);
    println!("  Problems: {}", result.problems);
    println!("  Solutions: {}", result.solutions);
    println!("  Topics: {}", result.topics);
    println!("  Code links: {}", result.code_links);
}

#[test]
fn test_conversation_parser_tool_errors() {
    use codescope_core::conversation::parse_conversation;
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let jsonl_path = dir.path().join("test-errors.jsonl");

    let mut f = std::fs::File::create(&jsonl_path).unwrap();
    // Message with tool error
    writeln!(f, r#"{{"type":"user","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"tool1","content":"error: cannot find type `Db` in module `engine::local`\n  --> src/graph/builder.rs:3:31","is_error":true}}]}},"timestamp":"2026-04-01T10:00:00Z","sessionId":"test-err-456"}}"#).unwrap();
    // Assistant fixes it
    writeln!(f, r#"{{"type":"assistant","message":{{"role":"assistant","content":"Fixed by adding the missing import: use surrealdb::engine::local::Db. The fix is to update the imports in builder.rs."}},"timestamp":"2026-04-01T10:01:00Z","sessionId":"test-err-456"}}"#).unwrap();

    let (entities, _relations, result) = parse_conversation(&jsonl_path, "test-repo", &[]).unwrap();

    assert!(result.problems >= 1, "Tool errors should be detected as problems, got {}", result.problems);
    assert!(result.solutions >= 1, "Should find the fix as a solution, got {}", result.solutions);

    // Verify entity types
    let problems: Vec<_> = entities.iter().filter(|e| e.kind == EntityKind::Problem).collect();
    assert!(!problems.is_empty(), "Should have Problem entities");
}

#[test]
fn test_conversation_entity_table_names() {
    assert_eq!(EntityKind::ConversationSession.table_name(), "conversation");
    assert_eq!(EntityKind::ConversationTopic.table_name(), "conv_topic");
    assert_eq!(EntityKind::Decision.table_name(), "decision");
    assert_eq!(EntityKind::Problem.table_name(), "problem");
    assert_eq!(EntityKind::Solution.table_name(), "solution");
}

#[test]
fn test_conversation_relation_table_names() {
    assert_eq!(RelationKind::DiscussedIn.table_name(), "discussed_in");
    assert_eq!(RelationKind::DecidedAbout.table_name(), "decided_about");
    assert_eq!(RelationKind::SolvesFor.table_name(), "solves_for");
}

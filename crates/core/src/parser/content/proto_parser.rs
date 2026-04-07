use super::ContentParser;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use anyhow::Result;

pub struct ProtoParser;

impl ContentParser for ProtoParser {
    fn name(&self) -> &str {
        "proto"
    }
    fn extensions(&self) -> &[&str] {
        &["proto"]
    }

    fn parse(
        &self,
        file_path: &str,
        source: &str,
        repo: &str,
    ) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let file_qname = format!("{}:{}", repo, file_path);
        entities.push(CodeEntity {
            kind: EntityKind::File,
            name: file_path.to_string(),
            qualified_name: file_qname.clone(),
            file_path: file_path.to_string(),
            repo: repo.to_string(),
            start_line: 0,
            end_line: source.lines().count() as u32,
            start_col: 0,
            end_col: 0,
            signature: None,
            body: None,
            body_hash: None,
            language: "protobuf".to_string(),
        });

        let mut current_service: Option<String> = None;

        for (i, line) in source.lines().enumerate() {
            let line_num = (i + 1) as u32;
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            // import "path";
            if trimmed.starts_with("import ") {
                let import_path = trimmed
                    .trim_start_matches("import ")
                    .trim_end_matches(';')
                    .trim()
                    .trim_matches('"');
                if !import_path.is_empty() {
                    let qname = format!("{}:import:{}", file_qname, import_path);
                    entities.push(CodeEntity {
                        kind: EntityKind::Import,
                        name: import_path.to_string(),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(trimmed.to_string()),
                        body: None,
                        body_hash: None,
                        language: "protobuf".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "import_decl".to_string(),
                        metadata: None,
                    });
                }
                continue;
            }

            // service X {
            if trimmed.starts_with("service ") {
                if let Some(name) = extract_proto_name(trimmed, "service ") {
                    let qname = format!("{}:service:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::ApiEndpoint,
                        name: name.clone(),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(trimmed.to_string()),
                        body: None,
                        body_hash: None,
                        language: "protobuf".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "api".to_string(),
                        metadata: None,
                    });
                    current_service = Some(name);
                }
                continue;
            }

            // rpc MethodName(Request) returns (Response)
            if trimmed.starts_with("rpc ") {
                if let Some(name) = extract_proto_name(trimmed, "rpc ") {
                    let parent = current_service.as_deref().unwrap_or("unknown");
                    let qname = format!("{}:rpc:{}:{}", file_qname, parent, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::Function,
                        name: name.clone(),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(trimmed.to_string()),
                        body: None,
                        body_hash: None,
                        language: "protobuf".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "function".to_string(),
                        metadata: None,
                    });
                }
                continue;
            }

            // message X {
            if trimmed.starts_with("message ") {
                if let Some(name) = extract_proto_name(trimmed, "message ") {
                    let qname = format!("{}:message:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::Class,
                        name: name.clone(),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(trimmed.to_string()),
                        body: None,
                        body_hash: None,
                        language: "protobuf".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "class".to_string(),
                        metadata: None,
                    });
                }
                continue;
            }

            // enum X {
            if trimmed.starts_with("enum ") {
                if let Some(name) = extract_proto_name(trimmed, "enum ") {
                    let qname = format!("{}:enum:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::Enum,
                        name: name.clone(),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(trimmed.to_string()),
                        body: None,
                        body_hash: None,
                        language: "protobuf".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "class".to_string(),
                        metadata: None,
                    });
                }
                continue;
            }

            // Track closing braces for service scope
            if trimmed == "}" {
                current_service = None;
            }
        }

        Ok((entities, relations))
    }
}

/// Extract the name token after a keyword prefix (e.g., "service ", "message ").
fn extract_proto_name(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?.trim();
    let name = rest
        .split(|c: char| c.is_whitespace() || c == '{' || c == '(')
        .next()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

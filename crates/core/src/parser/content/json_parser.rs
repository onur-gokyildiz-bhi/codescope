use anyhow::Result;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use super::ContentParser;

pub struct JsonParser;

impl ContentParser for JsonParser {
    fn name(&self) -> &str { "json" }
    fn extensions(&self) -> &[&str] { &["json"] }

    fn parse(&self, file_path: &str, source: &str, repo: &str) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        // Delegate to PackageParser for package.json files
        let filename = file_path.rsplit(&['/', '\\']).next().unwrap_or(file_path);
        if filename.eq_ignore_ascii_case("package.json") {
            return super::package_parser::PackageParser.parse(file_path, source, repo);
        }

        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let file_qname = format!("{}:{}", repo, file_path);

        // File entity
        entities.push(CodeEntity {
            kind: EntityKind::File,
            name: file_path.to_string(),
            qualified_name: file_qname.clone(),
            file_path: file_path.to_string(),
            repo: repo.to_string(),
            start_line: 0,
            end_line: source.lines().count() as u32,
            start_col: 0, end_col: 0,
            signature: None, body: None,
            body_hash: None,
            language: "json".to_string(),
        });

        // Parse JSON
        let value: serde_json::Value = match serde_json::from_str(source) {
            Ok(v) => v,
            Err(_) => return Ok((entities, relations)),
        };

        if let serde_json::Value::Object(map) = &value {
            extract_json_keys(map, file_path, repo, &file_qname, "", &mut entities, &mut relations, 0);
        }

        Ok((entities, relations))
    }
}

fn extract_json_keys(
    map: &serde_json::Map<String, serde_json::Value>,
    file_path: &str,
    repo: &str,
    parent_qname: &str,
    prefix: &str,
    entities: &mut Vec<CodeEntity>,
    relations: &mut Vec<CodeRelation>,
    depth: usize,
) {
    if depth > 5 { return; } // Limit nesting depth

    for (key, value) in map {
        let full_key = if prefix.is_empty() { key.clone() } else { format!("{}.{}", prefix, key) };
        let qname = format!("{}:{}:{}", repo, file_path, full_key);

        let kind = if matches!(value, serde_json::Value::Object(_)) {
            EntityKind::ConfigSection
        } else {
            EntityKind::ConfigKey
        };

        let body_str = match value {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(b.to_string()),
            serde_json::Value::Array(arr) => Some(format!("[{} items]", arr.len())),
            _ => None,
        };

        entities.push(CodeEntity {
            kind,
            name: full_key.clone(),
            qualified_name: qname.clone(),
            file_path: file_path.to_string(),
            repo: repo.to_string(),
            start_line: 0, end_line: 0, start_col: 0, end_col: 0,
            signature: None,
            body: body_str,
            body_hash: None,
            language: "json".to_string(),
        });

        relations.push(CodeRelation {
            kind: RelationKind::Contains,
            from_entity: parent_qname.to_string(),
            to_entity: qname.clone(),
            from_table: if depth == 0 { "file".to_string() } else { "config".to_string() },
            to_table: "config".to_string(),
            metadata: None,
        });

        // Recurse into nested objects
        if let serde_json::Value::Object(nested) = value {
            extract_json_keys(nested, file_path, repo, &qname, &full_key, entities, relations, depth + 1);
        }
    }
}

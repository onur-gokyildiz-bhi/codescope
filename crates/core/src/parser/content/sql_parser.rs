use anyhow::Result;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use super::ContentParser;

pub struct SqlParser;

impl ContentParser for SqlParser {
    fn name(&self) -> &str { "sql" }
    fn extensions(&self) -> &[&str] { &["sql"] }

    fn parse(&self, file_path: &str, source: &str, repo: &str) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let file_qname = format!("{}:{}", repo, file_path);
        entities.push(CodeEntity {
            kind: EntityKind::File, name: file_path.to_string(),
            qualified_name: file_qname.clone(), file_path: file_path.to_string(),
            repo: repo.to_string(), start_line: 0, end_line: source.lines().count() as u32,
            start_col: 0, end_col: 0, signature: None, body: None, body_hash: None,
            language: "sql".to_string(),
        });

        let upper_source = source.to_uppercase();
        let lines: Vec<&str> = source.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let line_num = (i + 1) as u32;
            let upper = line.trim().to_uppercase();

            // CREATE TABLE
            if upper.starts_with("CREATE TABLE") || upper.starts_with("CREATE TEMPORARY TABLE") {
                if let Some(name) = extract_name_after(&upper, "TABLE") {
                    let qname = format!("{}:table:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::DbTable, name: name.clone(),
                        qualified_name: qname.clone(), file_path: file_path.to_string(),
                        repo: repo.to_string(), start_line: line_num, end_line: line_num,
                        start_col: 0, end_col: 0,
                        signature: Some(line.trim().to_string()),
                        body: None, body_hash: None, language: "sql".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains, from_entity: file_qname.clone(),
                        to_entity: qname, metadata: None,
                    });
                }
            }
            // CREATE INDEX
            else if upper.starts_with("CREATE INDEX") || upper.starts_with("CREATE UNIQUE INDEX") {
                if let Some(name) = extract_name_after(&upper, "INDEX") {
                    let qname = format!("{}:index:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::DbIndex, name: name.clone(),
                        qualified_name: qname.clone(), file_path: file_path.to_string(),
                        repo: repo.to_string(), start_line: line_num, end_line: line_num,
                        start_col: 0, end_col: 0,
                        signature: Some(line.trim().to_string()),
                        body: None, body_hash: None, language: "sql".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains, from_entity: file_qname.clone(),
                        to_entity: qname, metadata: None,
                    });
                }
            }
            // CREATE VIEW
            else if upper.starts_with("CREATE VIEW") || upper.starts_with("CREATE OR REPLACE VIEW") {
                if let Some(name) = extract_name_after(&upper, "VIEW") {
                    let qname = format!("{}:view:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::DbView, name: name.clone(),
                        qualified_name: qname.clone(), file_path: file_path.to_string(),
                        repo: repo.to_string(), start_line: line_num, end_line: line_num,
                        start_col: 0, end_col: 0,
                        signature: Some(line.trim().to_string()),
                        body: None, body_hash: None, language: "sql".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains, from_entity: file_qname.clone(),
                        to_entity: qname, metadata: None,
                    });
                }
            }
        }

        Ok((entities, relations))
    }
}

fn extract_name_after(upper: &str, keyword: &str) -> Option<String> {
    let idx = upper.find(keyword)? + keyword.len();
    let rest = upper[idx..].trim();
    // Skip "IF NOT EXISTS"
    let rest = if rest.starts_with("IF NOT EXISTS") {
        rest["IF NOT EXISTS".len()..].trim()
    } else {
        rest
    };
    let name = rest.split(|c: char| c.is_whitespace() || c == '(').next()?;
    let name = name.trim_matches(|c: char| c == '"' || c == '`' || c == '[' || c == ']');
    if name.is_empty() { None } else { Some(name.to_lowercase()) }
}

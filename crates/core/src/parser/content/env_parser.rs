use super::ContentParser;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use anyhow::Result;

pub struct EnvParser;

impl ContentParser for EnvParser {
    fn name(&self) -> &str {
        "env"
    }
    fn extensions(&self) -> &[&str] {
        // .env files are matched by filename, not extension
        &[]
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
            language: "env".to_string(),
        });

        for (i, line) in source.lines().enumerate() {
            let line_num = (i + 1) as u32;
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Parse KEY=value
            if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].trim();
                let value = trimmed[eq_pos + 1..].trim();

                if key.is_empty() {
                    continue;
                }

                let qname = format!("{}:env:{}", file_qname, key);
                entities.push(CodeEntity {
                    kind: EntityKind::ConfigKey,
                    name: key.to_string(),
                    qualified_name: qname.clone(),
                    file_path: file_path.to_string(),
                    repo: repo.to_string(),
                    start_line: line_num,
                    end_line: line_num,
                    start_col: 0,
                    end_col: 0,
                    signature: None,
                    body: Some(value.to_string()),
                    body_hash: None,
                    language: "env".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::Contains,
                    from_entity: file_qname.clone(),
                    to_entity: qname,
                    from_table: "file".to_string(),
                    to_table: "config".to_string(),
                    metadata: None,
                });
            }
        }

        Ok((entities, relations))
    }
}

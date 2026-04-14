use super::ContentParser;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use anyhow::Result;

pub struct YamlParser;

impl ContentParser for YamlParser {
    fn name(&self) -> &str {
        "yaml"
    }
    fn extensions(&self) -> &[&str] {
        &["yaml", "yml"]
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
            language: "yaml".to_string(),
            cuda_qualifier: None,
        });

        let value: serde_yaml::Value = match serde_yaml::from_str(source) {
            Ok(v) => v,
            Err(_) => return Ok((entities, relations)),
        };

        extract_yaml_value(
            &value,
            file_path,
            repo,
            &file_qname,
            "",
            &mut entities,
            &mut relations,
            0,
        );
        Ok((entities, relations))
    }
}

#[allow(clippy::too_many_arguments)]
fn extract_yaml_value(
    value: &serde_yaml::Value,
    file_path: &str,
    repo: &str,
    parent_qname: &str,
    prefix: &str,
    entities: &mut Vec<CodeEntity>,
    relations: &mut Vec<CodeRelation>,
    depth: usize,
) {
    if depth > 5 {
        return;
    }

    if let serde_yaml::Value::Mapping(map) = value {
        for (k, v) in map {
            let key = match k {
                serde_yaml::Value::String(s) => s.clone(),
                _ => format!("{:?}", k),
            };
            let full_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };
            let qname = format!("{}:{}:{}", repo, file_path, full_key);

            let kind = if matches!(v, serde_yaml::Value::Mapping(_)) {
                EntityKind::ConfigSection
            } else {
                EntityKind::ConfigKey
            };

            let body_str = match v {
                serde_yaml::Value::String(s) => Some(s.clone()),
                serde_yaml::Value::Number(n) => Some(format!("{}", n.as_f64().unwrap_or(0.0))),
                serde_yaml::Value::Bool(b) => Some(b.to_string()),
                serde_yaml::Value::Sequence(arr) => Some(format!("[{} items]", arr.len())),
                _ => None,
            };

            entities.push(CodeEntity {
                kind,
                name: full_key.clone(),
                qualified_name: qname.clone(),
                file_path: file_path.to_string(),
                repo: repo.to_string(),
                start_line: 0,
                end_line: 0,
                start_col: 0,
                end_col: 0,
                signature: None,
                body: body_str,
                body_hash: None,
                language: "yaml".to_string(),
                cuda_qualifier: None,
            });

            relations.push(CodeRelation {
                kind: RelationKind::Contains,
                from_entity: parent_qname.to_string(),
                to_entity: qname.clone(),
                from_table: if depth == 0 {
                    "file".to_string()
                } else {
                    "config".to_string()
                },
                to_table: "config".to_string(),
                metadata: None,
            });

            extract_yaml_value(
                v,
                file_path,
                repo,
                &qname,
                &full_key,
                entities,
                relations,
                depth + 1,
            );
        }
    }
}

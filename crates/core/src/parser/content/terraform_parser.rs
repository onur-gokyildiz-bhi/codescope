use super::ContentParser;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use anyhow::Result;

pub struct TerraformParser;

impl ContentParser for TerraformParser {
    fn name(&self) -> &str {
        "terraform"
    }
    fn extensions(&self) -> &[&str] {
        &["tf", "tfvars"]
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
            language: "terraform".to_string(),
            cuda_qualifier: None,
        });

        for (i, line) in source.lines().enumerate() {
            let line_num = (i + 1) as u32;
            let trimmed = line.trim();

            // resource "type" "name" {
            if trimmed.starts_with("resource ") {
                if let Some((rtype, rname)) = extract_tf_block(trimmed, "resource") {
                    let qname = format!("{}:resource:{}:{}", file_qname, rtype, rname);
                    entities.push(CodeEntity {
                        kind: EntityKind::InfraResource,
                        name: format!("{}.{}", rtype, rname),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(format!("resource \"{}\" \"{}\"", rtype, rname)),
                        body: None,
                        body_hash: None,
                        language: "terraform".to_string(),
                        cuda_qualifier: None,
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "infra".to_string(),
                        metadata: None,
                    });
                }
            }
            // variable "name" {
            else if trimmed.starts_with("variable ") {
                if let Some(name) = extract_tf_name(trimmed, "variable") {
                    let qname = format!("{}:var:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::InfraVariable,
                        name: name.clone(),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(format!("variable \"{}\"", name)),
                        body: None,
                        body_hash: None,
                        language: "terraform".to_string(),
                        cuda_qualifier: None,
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "infra".to_string(),
                        metadata: None,
                    });
                }
            }
            // provider "name" {
            else if trimmed.starts_with("provider ") {
                if let Some(name) = extract_tf_name(trimmed, "provider") {
                    let qname = format!("{}:provider:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::InfraProvider,
                        name: name.clone(),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(format!("provider \"{}\"", name)),
                        body: None,
                        body_hash: None,
                        language: "terraform".to_string(),
                        cuda_qualifier: None,
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "infra".to_string(),
                        metadata: None,
                    });
                }
            }
            // module "name" {
            else if trimmed.starts_with("module ") {
                if let Some(name) = extract_tf_name(trimmed, "module") {
                    let qname = format!("{}:module:{}", file_qname, name);
                    entities.push(CodeEntity {
                        kind: EntityKind::InfraResource,
                        name: format!("module.{}", name),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(format!("module \"{}\"", name)),
                        body: None,
                        body_hash: None,
                        language: "terraform".to_string(),
                        cuda_qualifier: None,
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname.clone(),
                        to_entity: qname,
                        from_table: "file".to_string(),
                        to_table: "infra".to_string(),
                        metadata: None,
                    });
                }
            }
        }

        Ok((entities, relations))
    }
}

fn extract_tf_block(line: &str, keyword: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix(keyword)?.trim();
    let parts: Vec<&str> = rest.split('"').filter(|s| !s.trim().is_empty()).collect();
    if parts.len() >= 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

fn extract_tf_name(line: &str, keyword: &str) -> Option<String> {
    let rest = line.strip_prefix(keyword)?.trim();
    let name = rest.split('"').nth(1)?;
    Some(name.to_string())
}

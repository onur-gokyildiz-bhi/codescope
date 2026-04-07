use super::ContentParser;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use anyhow::Result;

pub struct DockerfileParser;

impl ContentParser for DockerfileParser {
    fn name(&self) -> &str {
        "dockerfile"
    }
    fn extensions(&self) -> &[&str] {
        &[]
    } // Detected by filename, not extension

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
            language: "dockerfile".to_string(),
        });

        let mut current_stage = file_qname.clone();
        let mut stage_count = 0;

        for (i, line) in source.lines().enumerate() {
            let line_num = (i + 1) as u32;
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let upper = trimmed.to_uppercase();

            // FROM instruction (new stage)
            if upper.starts_with("FROM ") {
                stage_count += 1;
                let image = trimmed[5..].trim();
                let stage_name = if let Some(idx) = image.to_lowercase().find(" as ") {
                    image[idx + 4..].trim().to_string()
                } else {
                    format!("stage_{}", stage_count)
                };

                let qname = format!("{}:stage:{}", file_qname, stage_name);
                entities.push(CodeEntity {
                    kind: EntityKind::DockerStage,
                    name: stage_name,
                    qualified_name: qname.clone(),
                    file_path: file_path.to_string(),
                    repo: repo.to_string(),
                    start_line: line_num,
                    end_line: line_num,
                    start_col: 0,
                    end_col: 0,
                    signature: Some(image.to_string()),
                    body: None,
                    body_hash: None,
                    language: "dockerfile".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::Contains,
                    from_entity: file_qname.clone(),
                    to_entity: qname.clone(),
                    from_table: "file".to_string(),
                    to_table: "infra".to_string(),
                    metadata: None,
                });
                current_stage = qname;
            }
            // Other instructions
            else if let Some(space_idx) = trimmed.find(' ') {
                let instruction = &trimmed[..space_idx];
                let args = trimmed[space_idx + 1..].trim();

                let qname = format!("{}:{}:{}", file_qname, instruction.to_lowercase(), line_num);
                entities.push(CodeEntity {
                    kind: EntityKind::DockerInstruction,
                    name: instruction.to_uppercase(),
                    qualified_name: qname.clone(),
                    file_path: file_path.to_string(),
                    repo: repo.to_string(),
                    start_line: line_num,
                    end_line: line_num,
                    start_col: 0,
                    end_col: 0,
                    signature: Some(format!(
                        "{} {}",
                        instruction.to_uppercase(),
                        truncate(args, 80)
                    )),
                    body: Some(args.to_string()),
                    body_hash: None,
                    language: "dockerfile".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::Contains,
                    from_entity: current_stage.clone(),
                    to_entity: qname,
                    from_table: if current_stage == file_qname {
                        "file".to_string()
                    } else {
                        "infra".to_string()
                    },
                    to_table: "infra".to_string(),
                    metadata: None,
                });
            }
        }

        Ok((entities, relations))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

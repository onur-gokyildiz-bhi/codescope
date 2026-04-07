use super::ContentParser;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use anyhow::Result;

pub struct MarkdownParser;

/// YAML frontmatter extracted from skill/knowledge files
struct Frontmatter {
    description: Option<String>,
    node_type: Option<String>,
    created: Option<String>,
}

impl ContentParser for MarkdownParser {
    fn name(&self) -> &str {
        "markdown"
    }
    fn extensions(&self) -> &[&str] {
        &["md", "mdx"]
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
            language: "markdown".to_string(),
        });

        // Phase 1: Parse YAML frontmatter (skill graph support)
        let (frontmatter, _body_start) = parse_frontmatter(source);
        let is_skill_file = frontmatter.is_some();

        // Create skill entity from frontmatter
        let skill_qname = if let Some(ref fm) = frontmatter {
            let stem = std::path::Path::new(file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(file_path);
            let qname = format!("{}:skill:{}", repo, stem);

            let kind = match fm.node_type.as_deref() {
                Some("moc") => EntityKind::SkillMOC,
                _ => EntityKind::SkillNode,
            };

            entities.push(CodeEntity {
                kind,
                name: stem.to_string(),
                qualified_name: qname.clone(),
                file_path: file_path.to_string(),
                repo: repo.to_string(),
                start_line: 1,
                end_line: source.lines().count() as u32,
                start_col: 0,
                end_col: 0,
                signature: fm.created.clone(), // created date in signature field
                body: fm.description.clone(),  // description in body field
                body_hash: None,
                language: "skill".to_string(),
            });

            // File contains skill
            relations.push(CodeRelation {
                kind: RelationKind::Contains,
                from_entity: file_qname.clone(),
                to_entity: qname.clone(),
                from_table: "file".to_string(),
                to_table: "skill".to_string(),
                metadata: None,
            });

            Some(qname)
        } else {
            None
        };

        // Phase 2: Line-by-line parsing
        let mut in_code_block = false;
        let mut in_frontmatter = false;
        let mut frontmatter_seen = false;
        let mut code_block_start = 0u32;
        let mut code_block_lang = String::new();
        let mut current_section = file_qname.clone();

        for (i, line) in source.lines().enumerate() {
            let line_num = (i + 1) as u32;

            // Skip YAML frontmatter lines
            if line.trim() == "---" {
                if !frontmatter_seen && line_num <= 2 {
                    in_frontmatter = true;
                    frontmatter_seen = true;
                    continue;
                } else if in_frontmatter {
                    in_frontmatter = false;
                    continue;
                }
            }
            if in_frontmatter {
                continue;
            }

            // Code blocks
            if line.starts_with("```") {
                if in_code_block {
                    let qname = format!("{}:codeblock:{}", file_qname, code_block_start);
                    entities.push(CodeEntity {
                        kind: EntityKind::DocCodeBlock,
                        name: format!("code({})", code_block_lang),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(),
                        repo: repo.to_string(),
                        start_line: code_block_start,
                        end_line: line_num,
                        start_col: 0,
                        end_col: 0,
                        signature: Some(code_block_lang.clone()),
                        body: None,
                        body_hash: None,
                        language: "markdown".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: current_section.clone(),
                        to_entity: qname,
                        from_table: if current_section == file_qname {
                            "file".to_string()
                        } else {
                            "doc".to_string()
                        },
                        to_table: "doc".to_string(),
                        metadata: None,
                    });
                    in_code_block = false;
                } else {
                    in_code_block = true;
                    code_block_start = line_num;
                    code_block_lang = line.trim_start_matches('`').trim().to_string();
                }
                continue;
            }

            if in_code_block {
                continue;
            }

            // Headings
            if line.starts_with('#') {
                let level = line.chars().take_while(|c| *c == '#').count();
                let title = line[level..].trim().to_string();
                if title.is_empty() {
                    continue;
                }

                let qname = format!("{}:h{}:{}", file_qname, level, title.replace(' ', "_"));
                entities.push(CodeEntity {
                    kind: EntityKind::DocSection,
                    name: title.clone(),
                    qualified_name: qname.clone(),
                    file_path: file_path.to_string(),
                    repo: repo.to_string(),
                    start_line: line_num,
                    end_line: line_num,
                    start_col: 0,
                    end_col: 0,
                    signature: Some(format!("h{}", level)),
                    body: None,
                    body_hash: None,
                    language: "markdown".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::Contains,
                    from_entity: file_qname.clone(),
                    to_entity: qname.clone(),
                    from_table: "file".to_string(),
                    to_table: "doc".to_string(),
                    metadata: None,
                });
                current_section = qname;
            }

            // Standard links: [text](url)
            let mut search = line.as_bytes();
            while let Some(start) = find_subsequence(search, b"](") {
                let bracket_start = search[..start].iter().rposition(|&b| b == b'[');
                if let Some(bs) = bracket_start {
                    // Skip if this is actually a wikilink `[[` — don't double-count
                    if bs > 0 && search[bs - 1] == b'[' {
                        search = &search[start + 2..];
                        continue;
                    }
                    let text = std::str::from_utf8(&search[bs + 1..start]).unwrap_or("");
                    let rest = &search[start + 2..];
                    if let Some(end) = rest.iter().position(|&b| b == b')') {
                        let url = std::str::from_utf8(&rest[..end]).unwrap_or("");
                        if !text.is_empty() && !url.is_empty() {
                            let qname = format!(
                                "{}:link:{}:{}",
                                file_qname,
                                line_num,
                                text.replace(' ', "_")
                            );
                            entities.push(CodeEntity {
                                kind: EntityKind::DocLink,
                                name: text.to_string(),
                                qualified_name: qname.clone(),
                                file_path: file_path.to_string(),
                                repo: repo.to_string(),
                                start_line: line_num,
                                end_line: line_num,
                                start_col: 0,
                                end_col: 0,
                                signature: Some(url.to_string()),
                                body: None,
                                body_hash: None,
                                language: "markdown".to_string(),
                            });
                            relations.push(CodeRelation {
                                kind: RelationKind::References,
                                from_entity: current_section.clone(),
                                to_entity: qname,
                                from_table: if current_section == file_qname {
                                    "file".to_string()
                                } else {
                                    "doc".to_string()
                                },
                                to_table: "doc".to_string(),
                                metadata: None,
                            });
                        }
                        search = &rest[end + 1..];
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            // Wikilinks: [[target]] or [[target|display text]]
            // Only scan if this is a skill file (has YAML frontmatter)
            if is_skill_file {
                if let Some(ref skill_qn) = skill_qname {
                    let mut wl_search = line.as_bytes();
                    while let Some(start) = find_subsequence(wl_search, b"[[") {
                        let rest = &wl_search[start + 2..];
                        if let Some(end) = find_subsequence(rest, b"]]") {
                            let inner = std::str::from_utf8(&rest[..end]).unwrap_or("");
                            if !inner.is_empty() {
                                // Handle [[target|display text]] pipe syntax
                                let target = inner.split('|').next().unwrap_or(inner).trim();
                                if !target.is_empty() {
                                    let target_qname = format!("{}:skill:{}", repo, target);
                                    relations.push(CodeRelation {
                                        kind: RelationKind::LinksTo,
                                        from_entity: skill_qn.clone(),
                                        to_entity: target_qname,
                                        from_table: "skill".to_string(),
                                        to_table: "skill".to_string(),
                                        metadata: Some(serde_json::json!({
                                            "context": line.trim(),
                                            "line": line_num,
                                        })),
                                    });
                                }
                            }
                            wl_search = &rest[end + 2..];
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        Ok((entities, relations))
    }
}

/// Parse YAML frontmatter from the beginning of a markdown file.
/// Returns (Some(Frontmatter), body_start_line) or (None, 0).
fn parse_frontmatter(source: &str) -> (Option<Frontmatter>, usize) {
    let trimmed = source.trim_start();
    if !trimmed.starts_with("---") {
        return (None, 0);
    }

    // Find closing ---
    let after_first = &trimmed[3..].trim_start_matches(['\r', '\n']);
    if let Some(end) = after_first.find("\n---") {
        let yaml_text = &after_first[..end];
        let body_start = source[..source.len() - after_first.len() + end + 4]
            .lines()
            .count();

        // Parse YAML with serde_yaml
        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(yaml_text) {
            let description = value
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let node_type = value
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let created = value
                .get("created")
                .and_then(|v| v.as_str().or_else(|| v.as_str()))
                .map(|s| s.to_string());

            return (
                Some(Frontmatter {
                    description,
                    node_type,
                    created,
                }),
                body_start,
            );
        }
    }

    (None, 0)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

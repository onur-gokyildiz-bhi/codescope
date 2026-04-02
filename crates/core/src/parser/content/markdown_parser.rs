use anyhow::Result;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use super::ContentParser;

pub struct MarkdownParser;

impl ContentParser for MarkdownParser {
    fn name(&self) -> &str { "markdown" }
    fn extensions(&self) -> &[&str] { &["md", "mdx"] }

    fn parse(&self, file_path: &str, source: &str, repo: &str) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let file_qname = format!("{}:{}", repo, file_path);
        entities.push(CodeEntity {
            kind: EntityKind::File, name: file_path.to_string(),
            qualified_name: file_qname.clone(), file_path: file_path.to_string(),
            repo: repo.to_string(), start_line: 0, end_line: source.lines().count() as u32,
            start_col: 0, end_col: 0, signature: None, body: None, body_hash: None,
            language: "markdown".to_string(),
        });

        let mut in_code_block = false;
        let mut code_block_start = 0u32;
        let mut code_block_lang = String::new();
        let mut current_section = file_qname.clone();

        for (i, line) in source.lines().enumerate() {
            let line_num = (i + 1) as u32;

            // Code blocks
            if line.starts_with("```") {
                if in_code_block {
                    let qname = format!("{}:codeblock:{}", file_qname, code_block_start);
                    entities.push(CodeEntity {
                        kind: EntityKind::DocCodeBlock,
                        name: format!("code({})", code_block_lang),
                        qualified_name: qname.clone(),
                        file_path: file_path.to_string(), repo: repo.to_string(),
                        start_line: code_block_start, end_line: line_num,
                        start_col: 0, end_col: 0,
                        signature: Some(code_block_lang.clone()),
                        body: None, body_hash: None, language: "markdown".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains, from_entity: current_section.clone(),
                        to_entity: qname,
                        from_table: if current_section == file_qname { "file".to_string() } else { "doc".to_string() },
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

            if in_code_block { continue; }

            // Headings
            if line.starts_with('#') {
                let level = line.chars().take_while(|c| *c == '#').count();
                let title = line[level..].trim().to_string();
                if title.is_empty() { continue; }

                let qname = format!("{}:h{}:{}", file_qname, level, title.replace(' ', "_"));
                entities.push(CodeEntity {
                    kind: EntityKind::DocSection,
                    name: title.clone(),
                    qualified_name: qname.clone(),
                    file_path: file_path.to_string(), repo: repo.to_string(),
                    start_line: line_num, end_line: line_num,
                    start_col: 0, end_col: 0,
                    signature: Some(format!("h{}", level)),
                    body: None, body_hash: None, language: "markdown".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::Contains, from_entity: file_qname.clone(),
                    to_entity: qname.clone(),
                    from_table: "file".to_string(),
                    to_table: "doc".to_string(),
                    metadata: None,
                });
                current_section = qname;
            }

            // Links: [text](url)
            let mut search = line.as_bytes();
            while let Some(start) = find_subsequence(search, b"](") {
                let bracket_start = search[..start].iter().rposition(|&b| b == b'[');
                if let Some(bs) = bracket_start {
                    let text = std::str::from_utf8(&search[bs + 1..start]).unwrap_or("");
                    let rest = &search[start + 2..];
                    if let Some(end) = rest.iter().position(|&b| b == b')') {
                        let url = std::str::from_utf8(&rest[..end]).unwrap_or("");
                        if !text.is_empty() && !url.is_empty() {
                            let qname = format!("{}:link:{}:{}", file_qname, line_num, text.replace(' ', "_"));
                            entities.push(CodeEntity {
                                kind: EntityKind::DocLink,
                                name: text.to_string(),
                                qualified_name: qname.clone(),
                                file_path: file_path.to_string(), repo: repo.to_string(),
                                start_line: line_num, end_line: line_num,
                                start_col: 0, end_col: 0,
                                signature: Some(url.to_string()),
                                body: None, body_hash: None, language: "markdown".to_string(),
                            });
                            relations.push(CodeRelation {
                                kind: RelationKind::References,
                                from_entity: current_section.clone(),
                                to_entity: qname,
                                from_table: if current_section == file_qname { "file".to_string() } else { "doc".to_string() },
                                to_table: "doc".to_string(),
                                metadata: None,
                            });
                        }
                        search = &rest[end + 1..];
                    } else { break; }
                } else { break; }
            }
        }

        Ok((entities, relations))
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

//! Conversation indexing — extracts decisions, problems, and solutions
//! from Claude Code JSONL transcripts and links them to code entities.

mod classifier;
pub mod compressor;
mod entity_linker;
mod parser;

use anyhow::Result;
use std::path::Path;

use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use classifier::classify_segments;
use entity_linker::EntityLinker;
use parser::ConversationParser;

/// Result of indexing conversation transcripts
#[derive(Debug, Default)]
pub struct ConvIndexResult {
    pub sessions_indexed: usize,
    pub decisions: usize,
    pub problems: usize,
    pub solutions: usize,
    pub topics: usize,
    pub code_links: usize,
    pub skipped: usize,
}

/// Index a JSONL conversation transcript into entities and relations.
/// Designed to work with the existing GraphBuilder pipeline.
pub fn parse_conversation(
    jsonl_path: &Path,
    repo: &str,
    known_entities: &[String],
) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>, ConvIndexResult)> {
    let parser = ConversationParser::new();
    let linker = EntityLinker::new(known_entities);

    // Phase 1: Parse JSONL into conversation turns
    let parse_result = parser.parse_jsonl(jsonl_path)?;
    let session_id = &parse_result.session_id;
    let jsonl_name = jsonl_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.jsonl");

    let mut entities = Vec::new();
    let mut relations = Vec::new();
    let mut result = ConvIndexResult::default();

    // Create session entity
    let session_qname = format!("{}:conv:{}", repo, session_id);
    entities.push(CodeEntity {
        kind: EntityKind::ConversationSession,
        name: format!("Session {}", &session_id[..8.min(session_id.len())]),
        qualified_name: session_qname.clone(),
        file_path: jsonl_name.to_string(),
        repo: repo.to_string(),
        start_line: 0,
        end_line: parse_result.turns.len() as u32,
        start_col: 0,
        end_col: 0,
        signature: parse_result.first_timestamp.clone(), // Store timestamp in signature field
        body: Some(format!(
            "{} turns, {} messages",
            parse_result.turns.len(),
            parse_result.message_count
        )),
        body_hash: Some(parse_result.file_hash.clone()),
        language: "conversation".to_string(),
        cuda_qualifier: None,
    });

    // Phase 2: Classify turns into decisions/problems/solutions
    let segments = classify_segments(&parse_result.turns);

    // Phase 3: Create entities and link to code
    for seg in &segments {
        let slug = slug_from_name(&seg.name);
        let qname = format!("{}:conv:{}:{}", repo, session_id, slug);

        let kind = match seg.kind {
            classifier::SegmentKind::Decision => {
                result.decisions += 1;
                EntityKind::Decision
            }
            classifier::SegmentKind::Problem => {
                result.problems += 1;
                EntityKind::Problem
            }
            classifier::SegmentKind::Solution => {
                result.solutions += 1;
                EntityKind::Solution
            }
            classifier::SegmentKind::Topic => {
                result.topics += 1;
                EntityKind::ConversationTopic
            }
        };

        // Append rationale to body for Decision entities
        let body_text = {
            let compressed = compressor::compress_segment(&seg.body, 500);
            if matches!(seg.kind, classifier::SegmentKind::Decision) {
                if let Some(rationale) = &seg.rationale {
                    format!("{}\n\nRationale: {}", compressed, rationale)
                } else {
                    compressed
                }
            } else {
                compressed
            }
        };

        // Link to code entities found in the text
        let refs = linker.find_references(&seg.body);

        // Derive scope from the first linked code entity's file_path
        let scope = refs.first().map(|r| derive_scope_from_path(&r.name));

        entities.push(CodeEntity {
            kind: kind.clone(),
            name: seg.name.clone(),
            qualified_name: qname.clone(),
            file_path: jsonl_name.to_string(),
            repo: repo.to_string(),
            start_line: seg.line_number,
            end_line: seg.line_number,
            start_col: 0,
            end_col: 0,
            signature: seg.timestamp.clone(),
            body: Some(body_text),
            body_hash: scope, // Repurpose body_hash to carry scope to the builder
            language: "conversation".to_string(),
            cuda_qualifier: None,
        });

        // Session contains this entity
        relations.push(CodeRelation {
            kind: RelationKind::Contains,
            from_entity: session_qname.clone(),
            to_entity: qname.clone(),
            from_table: "conversation".to_string(),
            to_table: kind.table_name().to_string(),
            metadata: None,
        });

        for code_ref in &refs {
            let rel_kind = match seg.kind {
                classifier::SegmentKind::Decision => RelationKind::DecidedAbout,
                _ => RelationKind::DiscussedIn,
            };
            relations.push(CodeRelation {
                kind: rel_kind,
                from_entity: qname.clone(),
                to_entity: code_ref.qualified_name.clone(),
                from_table: kind.table_name().to_string(),
                to_table: code_ref.entity_table.clone(),
                metadata: None,
            });
            result.code_links += 1;
        }

        // Link solutions to preceding problems
        if matches!(seg.kind, classifier::SegmentKind::Solution) {
            if let Some(prev_problem) = find_preceding_problem(&segments, seg) {
                let prob_slug = slug_from_name(&prev_problem.name);
                let prob_qname = format!("{}:conv:{}:{}", repo, session_id, prob_slug);
                relations.push(CodeRelation {
                    kind: RelationKind::SolvesFor,
                    from_entity: qname.clone(),
                    to_entity: prob_qname,
                    from_table: "solution".to_string(),
                    to_table: "problem".to_string(),
                    metadata: None,
                });
            }
        }
    }

    result.sessions_indexed = 1;
    Ok((entities, relations, result))
}

/// Parse a memory markdown file into entities and relations.
/// Uses the existing MarkdownParser but tags entities with language="memory".
pub fn parse_memory_file(
    md_path: &Path,
    repo: &str,
    known_entities: &[String],
) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
    let content = std::fs::read_to_string(md_path)?;
    let filename = md_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.md");

    let md_parser = crate::parser::content::markdown_parser::MarkdownParser;
    use crate::parser::content::ContentParser;
    let (mut entities, mut relations) = md_parser.parse(filename, &content, repo)?;

    // Tag all entities as "memory" to distinguish from repo docs
    for entity in &mut entities {
        entity.language = "memory".to_string();
    }

    // Link to code entities mentioned in section bodies
    let linker = entity_linker::EntityLinker::new(known_entities);
    for entity in &entities {
        if let Some(body) = &entity.body {
            let refs = linker.find_references(body);
            for code_ref in &refs {
                relations.push(CodeRelation {
                    kind: RelationKind::DiscussedIn,
                    from_entity: entity.qualified_name.clone(),
                    to_entity: code_ref.qualified_name.clone(),
                    from_table: entity.kind.table_name().to_string(),
                    to_table: code_ref.entity_table.clone(),
                    metadata: None,
                });
            }
        }
    }

    Ok((entities, relations))
}

/// Find the most recent problem before this solution
fn find_preceding_problem<'a>(
    segments: &'a [classifier::ClassifiedSegment],
    solution: &classifier::ClassifiedSegment,
) -> Option<&'a classifier::ClassifiedSegment> {
    segments.iter().rev().find(|s| {
        matches!(s.kind, classifier::SegmentKind::Problem) && s.line_number < solution.line_number
    })
}

/// Generate skill note markdown files from conversation segments.
/// Returns Vec<(filename, content)> pairs ready to write to disk.
pub fn generate_skill_notes(
    segments: &[(String, String, String, Option<String>)], // (kind, name, body, timestamp)
    code_refs: &[String],
) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut decisions = Vec::new();
    let mut problems = Vec::new();
    let mut solutions = Vec::new();

    for (kind, name, body, timestamp) in segments {
        let slug = slug_from_name(name);
        let ts = timestamp.as_deref().unwrap_or("2026-01-01");
        let note_type = match kind.as_str() {
            "Decision" | "decision" => "decision",
            "Problem" | "problem" => "problem",
            "Solution" | "solution" => "solution",
            _ => "insight",
        };

        // Convert code entity references to wikilinks
        let mut linked_body = body.clone();
        for code_ref in code_refs {
            if body.contains(code_ref) {
                linked_body = linked_body.replace(code_ref, &format!("[[{}]]", code_ref));
            }
        }

        let content = format!(
            "---\ndescription: {}\ntype: {}\ncreated: {}\n---\n\n# {}\n\n{}\n\n---\n\nTopics:\n- [[{}s]]\n",
            name, note_type, ts, name, linked_body, note_type
        );

        let filename = format!("{}.md", slug);
        match note_type {
            "decision" => decisions.push(format!("- [[{}]] — {}", slug, name)),
            "problem" => problems.push(format!("- [[{}]] — {}", slug, name)),
            "solution" => solutions.push(format!("- [[{}]] — {}", slug, name)),
            _ => {}
        }

        files.push((filename, content));
    }

    // Generate index MOC
    let mut index = String::from("---\ndescription: Auto-generated skill graph from conversation history\ntype: moc\ncreated: 2026-01-01\n---\n\n# Project Knowledge\n\n");

    if !decisions.is_empty() {
        index.push_str("## Decisions\n\n");
        for d in &decisions {
            index.push_str(d);
            index.push('\n');
        }
        index.push('\n');
    }
    if !problems.is_empty() {
        index.push_str("## Problems\n\n");
        for p in &problems {
            index.push_str(p);
            index.push('\n');
        }
        index.push('\n');
    }
    if !solutions.is_empty() {
        index.push_str("## Solutions\n\n");
        for s in &solutions {
            index.push_str(s);
            index.push('\n');
        }
        index.push('\n');
    }

    files.push(("index.md".to_string(), index));
    files
}

/// Derive a module scope from a file path.
/// e.g. "crates/core/src/graph/builder.rs" -> "core::graph"
/// e.g. "src/main.rs" -> "root"
fn derive_scope_from_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();

    // Find the "src" directory and take crate name + module path
    if let Some(src_idx) = parts.iter().position(|&p| p == "src") {
        // Module path = directories after src/, excluding the file itself
        let module_parts: Vec<&str> = parts[src_idx + 1..]
            .iter()
            .copied()
            .filter(|p| !p.contains('.')) // exclude the file
            .collect();

        // Crate name = directory before src/ (if it exists and isn't the root)
        let crate_name = if src_idx > 0 { parts[src_idx - 1] } else { "" };

        if module_parts.is_empty() && crate_name.is_empty() {
            return "root".to_string();
        }

        let mut scope_parts = Vec::new();
        if !crate_name.is_empty() {
            scope_parts.push(crate_name);
        }
        scope_parts.extend(module_parts);

        if scope_parts.is_empty() {
            "root".to_string()
        } else {
            scope_parts.join("::")
        }
    } else {
        "root".to_string()
    }
}

fn slug_from_name(name: &str) -> String {
    name.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
        .replace("__", "_")
        .trim_matches('_')
        .chars()
        .take(60)
        .collect()
}

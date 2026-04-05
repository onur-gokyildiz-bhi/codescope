//! Conversation indexing — extracts decisions, problems, and solutions
//! from Claude Code JSONL transcripts and links them to code entities.

mod parser;
mod classifier;
mod entity_linker;
pub mod compressor;

use anyhow::Result;
use std::path::Path;

use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use parser::ConversationParser;
use classifier::classify_segments;
use entity_linker::EntityLinker;

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
    let jsonl_name = jsonl_path.file_name()
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
        body: Some(format!("{} turns, {} messages", parse_result.turns.len(), parse_result.message_count)),
        body_hash: Some(parse_result.file_hash.clone()),
        language: "conversation".to_string(),
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
            signature: None,
            body: Some(compressor::compress_segment(&seg.body, 500)),
            body_hash: None,
            language: "conversation".to_string(),
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

        // Link to code entities found in the text
        let refs = linker.find_references(&seg.body);
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
    let filename = md_path.file_name()
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
    segments.iter()
        .rev()
        .find(|s| {
            matches!(s.kind, classifier::SegmentKind::Problem)
                && s.line_number < solution.line_number
        })
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


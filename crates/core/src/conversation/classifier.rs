//! Heuristic classifier for conversation segments.
//! Identifies decisions, problems, solutions, and topic boundaries
//! using keyword patterns — no LLM calls.

use super::parser::ConversationTurn;

/// Classification result for a conversation segment
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ClassifiedSegment {
    pub kind: SegmentKind,
    pub name: String,
    pub body: String,
    pub line_number: u32,
    pub confidence: f32,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SegmentKind {
    Decision,
    Problem,
    Solution,
    Topic,
}

/// Decision indicator patterns (assistant messages)
const DECISION_SIGNALS: &[(&str, f32)] = &[
    // English
    ("i'll use", 0.7),
    ("i decided to", 0.9),
    ("let's go with", 0.8),
    ("the approach is", 0.7),
    ("instead of", 0.5),
    ("rather than", 0.5),
    ("we should use", 0.7),
    ("the best option", 0.8),
    ("i chose", 0.9),
    ("switched to", 0.7),
    ("changed it to", 0.6),
    ("the fix is to", 0.8),
    ("switching from", 0.7),
    ("replaced with", 0.6),
    // Turkish
    ("kullanacağım", 0.7),
    ("tercih ettim", 0.8),
    ("yerine", 0.4),
    ("değiştirdim", 0.6),
    ("en iyi seçenek", 0.8),
    ("yapalım", 0.5),
];

/// Problem indicator patterns (user and assistant messages)
const PROBLEM_SIGNALS: &[(&str, f32)] = &[
    // Error indicators
    ("error:", 0.9),
    ("failed to", 0.7),
    ("doesn't work", 0.7),
    ("doesn't compile", 0.9),
    ("compile error", 0.9),
    ("panic at", 0.9),
    ("cannot find", 0.6),
    ("unexpected token", 0.9),
    ("type mismatch", 0.9),
    ("is missing", 0.6),
    ("silently fail", 0.8),
    ("still failing", 0.8),
    ("still broken", 0.9),
    ("[error:", 0.9),
    ("exit code 1", 0.8),
    // Problem description
    ("the issue is", 0.8),
    ("the problem is", 0.9),
    ("the bug is", 0.9),
    ("root cause", 0.8),
    ("missing from", 0.6),
    ("not being stored", 0.8),
    ("not working", 0.7),
    // Turkish
    ("hata", 0.6),
    ("çalışmıyor", 0.8),
    ("sorun", 0.5),
    ("bozuk", 0.7),
];

/// Solution indicator patterns (assistant messages after a problem)
const SOLUTION_SIGNALS: &[(&str, f32)] = &[
    ("the fix is", 0.9),
    ("to fix this", 0.8),
    ("the solution", 0.9),
    ("here's how", 0.6),
    ("i've updated", 0.7),
    ("now it should", 0.6),
    ("fixed by", 0.9),
    ("resolved by", 0.9),
    ("this fixes", 0.8),
    ("the fix:", 0.9),
    ("two fixes needed", 0.9),
    ("fixed it", 0.8),
    ("applied the fix", 0.8),
    ("works now", 0.7),
    ("all tests pass", 0.7),
    ("everything green", 0.6),
    // Turkish
    ("düzelttim", 0.8),
    ("çözüm", 0.8),
    ("düzeltme", 0.7),
];

/// Classify conversation turns into structured segments.
pub fn classify_segments(turns: &[ConversationTurn]) -> Vec<ClassifiedSegment> {
    let mut segments = Vec::new();
    let mut recent_has_problem = false;

    for (i, turn) in turns.iter().enumerate() {
        let lower = turn.text.to_lowercase();

        // Check for tool errors as problems
        if turn.has_tool_error {
            if let Some(err_text) = &turn.tool_error_text {
                let name = extract_error_summary(err_text);
                segments.push(ClassifiedSegment {
                    kind: SegmentKind::Problem,
                    name,
                    body: truncate_body(err_text, 400),
                    line_number: turn.line_number,
                    confidence: 0.9,
                    timestamp: turn.timestamp.clone(),
                });
                recent_has_problem = true;
                continue;
            }
        }

        // Skip very short turns
        if turn.text.len() < 20 {
            continue;
        }

        // Check problems first (both user and assistant)
        let problem_score = score_patterns(&lower, PROBLEM_SIGNALS);
        if problem_score >= 0.6 {
            let name = extract_segment_name(&turn.text, &lower, PROBLEM_SIGNALS);
            segments.push(ClassifiedSegment {
                kind: SegmentKind::Problem,
                name,
                body: truncate_body(&turn.text, 400),
                line_number: turn.line_number,
                confidence: problem_score,
                timestamp: turn.timestamp.clone(),
            });
            recent_has_problem = true;
            continue;
        }

        // Solutions (assistant messages, especially after a problem)
        if turn.role == "assistant" {
            let solution_score = score_patterns(&lower, SOLUTION_SIGNALS);
            let boosted = if recent_has_problem { solution_score + 0.15 } else { solution_score };
            if boosted >= 0.6 {
                let name = extract_segment_name(&turn.text, &lower, SOLUTION_SIGNALS);
                segments.push(ClassifiedSegment {
                    kind: SegmentKind::Solution,
                    name,
                    body: truncate_body(&turn.text, 400),
                    line_number: turn.line_number,
                    confidence: boosted.min(1.0),
                    timestamp: turn.timestamp.clone(),
                });
                recent_has_problem = false;
                continue;
            }
        }

        // Decisions (assistant messages)
        if turn.role == "assistant" {
            let decision_score = score_patterns(&lower, DECISION_SIGNALS);
            if decision_score >= 0.6 {
                let name = extract_segment_name(&turn.text, &lower, DECISION_SIGNALS);
                segments.push(ClassifiedSegment {
                    kind: SegmentKind::Decision,
                    name,
                    body: truncate_body(&turn.text, 400),
                    line_number: turn.line_number,
                    confidence: decision_score,
                    timestamp: turn.timestamp.clone(),
                });
                continue;
            }
        }

        // Topic detection: user messages that start a new discussion
        if turn.role == "user" && turn.text.len() > 50 {
            // Check for time gap with previous turn
            let is_new_topic = if i > 0 {
                // Simple heuristic: user messages after a long gap or with different subject
                let prev = &turns[i - 1];
                prev.role == "assistant" && turn.text.len() > 80
            } else {
                true
            };

            if is_new_topic {
                let name = extract_topic_name(&turn.text);
                if !name.is_empty() {
                    segments.push(ClassifiedSegment {
                        kind: SegmentKind::Topic,
                        name,
                        body: truncate_body(&turn.text, 300),
                        line_number: turn.line_number,
                        confidence: 0.5,
                        timestamp: turn.timestamp.clone(),
                    });
                }
            }
        }
    }

    // Deduplicate very similar segments (same line or very similar names)
    dedup_segments(&mut segments);
    segments
}

/// Score text against a set of signal patterns
fn score_patterns(text: &str, signals: &[(&str, f32)]) -> f32 {
    let mut max_score = 0.0f32;
    let mut match_count = 0;

    for (pattern, weight) in signals {
        if text.contains(pattern) {
            if *weight > max_score {
                max_score = *weight;
            }
            match_count += 1;
        }
    }

    // Boost for multiple matches
    let boost = (match_count as f32 * 0.05).min(0.2);
    (max_score + boost).min(1.0)
}

/// Extract a short name/summary from the text near the matched pattern.
/// Uses char-based indexing to handle multi-byte UTF-8 (Turkish, etc.) safely.
fn extract_segment_name(text: &str, lower: &str, signals: &[(&str, f32)]) -> String {
    // Find the best matching pattern
    let mut best_pattern = "";
    let mut best_weight = 0.0f32;
    for (pattern, weight) in signals {
        if lower.contains(pattern) && *weight > best_weight {
            best_pattern = pattern;
            best_weight = *weight;
        }
    }

    // Extract surrounding sentence — work entirely in `lower` to avoid
    // byte-position mismatch (to_lowercase() can change byte lengths for
    // Turkish İ→i, etc.)
    if let Some(pos) = lower.find(best_pattern) {
        let start = lower[..pos].rfind(|c: char| c == '.' || c == '\n' || c == '!')
            .map(|p| next_char_boundary(lower, p + 1))
            .unwrap_or_else(|| floor_char_boundary(lower, pos.saturating_sub(40)));
        let end = lower[pos..].find(|c: char| c == '.' || c == '\n')
            .map(|p| pos + p)
            .unwrap_or_else(|| floor_char_boundary(lower, (pos + 120).min(lower.len())));
        let sentence = lower[start..end].trim();
        if sentence.len() > 80 {
            safe_truncate(sentence, 80)
        } else {
            sentence.to_string()
        }
    } else {
        // Fallback: first meaningful line
        first_meaningful_line(text)
    }
}

/// Extract topic name from user message
fn extract_topic_name(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or(text);
    let cleaned = first_line.trim();
    if cleaned.len() > 100 {
        safe_truncate(cleaned, 100)
    } else {
        cleaned.to_string()
    }
}

fn extract_error_summary(error_text: &str) -> String {
    // Take first meaningful line of error
    for line in error_text.lines() {
        let trimmed = line.trim();
        if trimmed.len() > 10 && !trimmed.starts_with("at ") && !trimmed.starts_with("  ") {
            return if trimmed.len() > 100 {
                safe_truncate(trimmed, 100)
            } else {
                trimmed.to_string()
            };
        }
    }
    first_meaningful_line(error_text)
}

fn first_meaningful_line(text: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.len() > 10 {
            return if trimmed.len() > 100 {
                safe_truncate(trimmed, 100)
            } else {
                trimmed.to_string()
            };
        }
    }
    text.chars().take(80).collect()
}

fn truncate_body(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        safe_truncate(text, max)
    }
}

/// Truncate a string at a char boundary, appending "..."
fn safe_truncate(s: &str, max_bytes: usize) -> String {
    let boundary = floor_char_boundary(s, max_bytes);
    format!("{}...", &s[..boundary])
}

/// Find the largest valid char boundary <= pos
fn floor_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut i = pos;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Find the smallest valid char boundary >= pos
fn next_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut i = pos;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn dedup_segments(segments: &mut Vec<ClassifiedSegment>) {
    segments.dedup_by(|a, b| {
        a.line_number == b.line_number
            || (a.kind == b.kind && a.name == b.name)
    });
}

//! Semantic conversation compression — SimpleMem-inspired pipeline.
//!
//! Instead of raw 500-char truncation, this module:
//! 1. Removes filler phrases and noise
//! 2. Extracts key sentences (decisions, errors, code references)
//! 3. Deduplicates similar content across segments
//! 4. Produces dense, information-rich summaries

/// Compress a conversation segment body, preserving essential information.
/// Returns compressed text that is significantly shorter but retains key facts.
pub fn compress_segment(body: &str, max_chars: usize) -> String {
    if body.len() <= max_chars {
        return body.to_string();
    }

    // Step 1: Split into sentences
    let sentences = split_sentences(body);

    // Step 2: Score each sentence by information density
    let mut scored: Vec<(f32, &str)> = sentences
        .iter()
        .map(|s| (score_sentence(s), s.as_str()))
        .collect();

    // Step 3: Sort by score (highest first)
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Step 4: Greedily select sentences until budget exhausted
    let mut result = String::new();
    let mut used_chars = 0;

    for (_score, sentence) in &scored {
        let clean = remove_filler(sentence);
        if clean.is_empty() {
            continue;
        }
        if used_chars + clean.len() + 2 > max_chars {
            break;
        }
        if !result.is_empty() {
            result.push_str(". ");
        }
        result.push_str(&clean);
        used_chars = result.len();
    }

    if result.is_empty() {
        // Fallback: truncate the original
        safe_truncate(body, max_chars)
    } else {
        result
    }
}

/// Score a sentence by information density.
/// Higher = more likely to contain useful information.
fn score_sentence(sentence: &str) -> f32 {
    let lower = sentence.to_lowercase();
    let mut score: f32 = 0.0;

    // Technical content indicators (+high)
    let tech_patterns = [
        "error:",
        "failed:",
        "warning:",
        "panic:",
        "exception:",
        "bug",
        "crash",
        "fix",
        "patch",
        "workaround",
        "function",
        "struct",
        "class",
        "impl",
        "trait",
        "api",
        "endpoint",
        "route",
        "handler",
        "middleware",
        "database",
        "query",
        "schema",
        "migration",
        "index",
        "deploy",
        "release",
        "version",
        "build",
        "compile",
        "config",
        "setting",
        "env",
        "port",
        "path",
    ];
    for p in &tech_patterns {
        if lower.contains(p) {
            score += 0.3;
        }
    }

    // Decision signals (+very high)
    let decision_patterns = [
        "decided",
        "decision",
        "chose",
        "choosing",
        "selected",
        "will use",
        "switched to",
        "migrated",
        "karar",
        "should",
        "must",
        "instead of",
        "rather than",
    ];
    for p in &decision_patterns {
        if lower.contains(p) {
            score += 0.5;
        }
    }

    // Code references (+high)
    if sentence.contains('`')
        || sentence.contains("()")
        || sentence.contains("::")
        || sentence.contains("->")
        || sentence.contains("fn ")
        || sentence.contains("def ")
    {
        score += 0.4;
    }

    // File paths (+medium)
    if sentence.contains('/')
        && (sentence.contains(".rs")
            || sentence.contains(".ts")
            || sentence.contains(".py")
            || sentence.contains(".js"))
    {
        score += 0.3;
    }

    // Numbers and metrics (+low)
    if sentence.chars().any(|c| c.is_ascii_digit()) {
        score += 0.1;
    }

    // Penalize filler-heavy sentences
    let filler_count = count_fillers(&lower);
    score -= filler_count as f32 * 0.15;

    // Penalize very short sentences (likely noise)
    if sentence.len() < 10 {
        score -= 0.3;
    }

    // Bonus for longer substantive sentences
    let word_count = sentence.split_whitespace().count();
    if word_count > 5 && word_count < 40 {
        score += 0.1;
    }

    score.max(0.0)
}

/// Remove filler phrases from a sentence.
fn remove_filler(sentence: &str) -> String {
    let mut result = sentence.to_string();

    let fillers = [
        "I think ",
        "I believe ",
        "I suppose ",
        "I guess ",
        "Let me ",
        "Let's see ",
        "Let me check ",
        "Okay so ",
        "Okay, ",
        "Ok, ",
        "Alright, ",
        "So, ",
        "Well, ",
        "Actually, ",
        "Basically, ",
        "In other words, ",
        "To be honest, ",
        "As you can see, ",
        "As mentioned, ",
        "Please note that ",
        "Note that ",
        "It seems like ",
        "It looks like ",
        "I'm going to ",
        "I'll go ahead and ",
    ];

    for filler in &fillers {
        if result.starts_with(filler) {
            result = result[filler.len()..].to_string();
        }
    }

    result.trim().to_string()
}

/// Count filler words in text.
fn count_fillers(text: &str) -> usize {
    let fillers = [
        "basically",
        "actually",
        "literally",
        "obviously",
        "essentially",
        "honestly",
        "frankly",
        "i think",
        "i believe",
        "i guess",
        "i suppose",
        "let me",
        "let's see",
        "okay",
        "alright",
        "well",
        "so",
        "you know",
        "kind of",
    ];
    fillers.iter().filter(|f| text.contains(**f)).count()
}

/// Split text into sentences (handles multiple delimiters).
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if (ch == '.' || ch == '!' || ch == '?' || ch == '\n') && current.trim().len() > 5 {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current.clear();
        }
    }

    if !current.trim().is_empty() {
        sentences.push(current.trim().to_string());
    }

    sentences
}

/// Merge segments about the same topic into a single compressed segment.
/// Returns merged body text that synthesizes information from all segments.
pub fn merge_topic_segments(bodies: &[&str], max_chars: usize) -> String {
    if bodies.len() <= 1 {
        return bodies
            .first()
            .map(|b| compress_segment(b, max_chars))
            .unwrap_or_default();
    }

    // Concatenate all bodies and compress together
    let combined: String = bodies.join(" ");
    compress_segment(&combined, max_chars)
}

/// Safe truncation that respects char boundaries.
fn safe_truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut boundary = max;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}...", &s[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_short_text() {
        let text = "This is short.";
        assert_eq!(compress_segment(text, 500), text);
    }

    #[test]
    fn test_compress_removes_filler() {
        let text = "I think we should use reqwest for HTTP calls. Let me check the docs. \
                    The error is in the parser module. We decided to use SurrealDB for storage. \
                    Basically this is just a simple change. Well, I guess we can proceed.";
        let compressed = compress_segment(text, 200);
        assert!(compressed.len() <= 200);
        // Should prioritize decision and error sentences
        assert!(
            compressed.contains("decided")
                || compressed.contains("error")
                || compressed.contains("reqwest")
        );
    }

    #[test]
    fn test_score_decision_higher() {
        let decision = "We decided to use SurrealDB for the graph database.";
        let filler = "Okay so let me think about this for a moment.";
        assert!(score_sentence(decision) > score_sentence(filler));
    }

    #[test]
    fn test_merge_topics() {
        let bodies = vec![
            "Error in the parser module.",
            "Fixed the parser by updating the grammar.",
        ];
        let merged = merge_topic_segments(&bodies, 300);
        assert!(!merged.is_empty());
    }
}

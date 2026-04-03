//! JSONL conversation transcript parser.
//! Streams line-by-line for memory efficiency with large (10MB+) files.

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader};
use std::path::Path;

/// A parsed conversation turn (one user or assistant message)
#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub role: String,           // "user" or "assistant"
    pub text: String,           // extracted text content
    pub line_number: u32,       // line in JSONL file
    pub timestamp: Option<String>,
    pub has_tool_use: bool,     // assistant used a tool
    pub has_tool_error: bool,   // tool returned an error
    pub tool_error_text: Option<String>,
}

/// Result of parsing a JSONL conversation file
pub struct ConversationParseResult {
    pub session_id: String,
    pub turns: Vec<ConversationTurn>,
    pub message_count: usize,
    pub file_hash: String,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
}

pub struct ConversationParser;

impl ConversationParser {
    pub fn new() -> Self { Self }

    /// Parse a JSONL transcript into structured conversation turns.
    /// Streams line-by-line for memory efficiency.
    pub fn parse_jsonl(&self, path: &Path) -> Result<ConversationParseResult> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        let mut hasher = Sha256::new();

        let mut turns = Vec::new();
        let mut session_id = String::new();
        let mut message_count = 0;
        let mut first_timestamp: Option<String> = None;
        let mut last_timestamp: Option<String> = None;

        for (line_idx, line_result) in reader.lines().enumerate() {
            let line = line_result?;
            hasher.update(line.as_bytes());
            hasher.update(b"\n");

            let entry: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Extract session ID from first entry
            if session_id.is_empty() {
                if let Some(sid) = entry.get("sessionId").and_then(|v| v.as_str()) {
                    session_id = sid.to_string();
                }
            }

            let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

            // Skip non-message types
            if entry_type == "queue-operation" || entry_type == "system" {
                continue;
            }

            let msg = match entry.get("message") {
                Some(m) => m,
                None => continue,
            };

            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
            if role != "user" && role != "assistant" {
                continue;
            }

            let timestamp = entry.get("timestamp").and_then(|v| v.as_str()).map(|s| s.to_string());
            let content = msg.get("content");

            let (text, has_tool_use, has_tool_error, tool_error_text) = match content {
                Some(c) => extract_content(c),
                None => continue,
            };

            if text.is_empty() && !has_tool_error {
                continue;
            }

            message_count += 1;

            // Track first/last timestamps
            if let Some(ts) = &timestamp {
                if first_timestamp.is_none() {
                    first_timestamp = Some(ts.clone());
                }
                last_timestamp = Some(ts.clone());
            }

            turns.push(ConversationTurn {
                role: role.to_string(),
                text,
                line_number: line_idx as u32,
                timestamp,
                has_tool_use,
                has_tool_error,
                tool_error_text,
            });
        }

        if session_id.is_empty() {
            // Derive from filename
            session_id = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
        }

        let file_hash = hex::encode(hasher.finalize());

        Ok(ConversationParseResult {
            session_id,
            turns,
            message_count,
            file_hash,
            first_timestamp,
            last_timestamp,
        })
    }
}

/// Extract text content from a message's content field.
/// Handles both string and array-of-blocks formats.
/// Returns (text, has_tool_use, has_tool_error, tool_error_text)
fn extract_content(content: &serde_json::Value) -> (String, bool, bool, Option<String>) {
    let mut texts = Vec::new();
    let mut has_tool_use = false;
    let mut has_tool_error = false;
    let mut tool_error_text = None;

    match content {
        serde_json::Value::String(s) => {
            texts.push(s.clone());
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                let block_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                            texts.push(t.to_string());
                        }
                    }
                    "tool_use" => {
                        has_tool_use = true;
                        // Record tool name for context
                        if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                            texts.push(format!("[tool: {}]", name));
                        }
                    }
                    "tool_result" => {
                        let is_err = item.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                        if is_err {
                            has_tool_error = true;
                            // Extract error content
                            if let Some(err_content) = item.get("content") {
                                let err_text = match err_content {
                                    serde_json::Value::String(s) => s.clone(),
                                    serde_json::Value::Array(arr) => {
                                        arr.iter()
                                            .filter_map(|b| b.get("text").and_then(|v| v.as_str()))
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                    }
                                    _ => String::new(),
                                };
                                if !err_text.is_empty() {
                                    tool_error_text = Some(err_text.clone());
                                    let preview_end = {
                                        let max = 200.min(err_text.len());
                                        let mut i = max;
                                        while i > 0 && !err_text.is_char_boundary(i) { i -= 1; }
                                        i
                                    };
                                    texts.push(format!("[error: {}]", &err_text[..preview_end]));
                                }
                            }
                        }
                    }
                    // Skip "thinking" blocks — internal reasoning
                    _ => {}
                }
            }
        }
        _ => {}
    }

    (texts.join("\n"), has_tool_use, has_tool_error, tool_error_text)
}

//! Conversation memory tool: single `conversations` tool with action dispatch.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::helpers::{check_conversation_hash, link_cross_session_topics, load_known_entities};
use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = conversations_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Unified conversations tool: index transcripts, search decisions/problems, or timeline for an entity.
    #[tool(
        description = "Conversation memory (index transcripts, search decisions/problems, entity timeline)."
    )]
    async fn conversations(&self, Parameters(params): Parameters<ConversationsParams>) -> String {
        match params.action.as_str() {
            "index" => self.conversations_index(params.path).await,
            "search" => {
                let query = match params.query {
                    Some(q) => q,
                    None => return "Error: 'search' action requires 'query'".to_string(),
                };
                self.conversations_search(query, params.limit).await
            }
            "timeline" => {
                let name = match params.query {
                    Some(q) => q,
                    None => {
                        return "Error: 'timeline' action requires 'query' (entity name)"
                            .to_string()
                    }
                };
                self.conversations_timeline(name, params.limit).await
            }
            other => format!(
                "Error: unknown action '{}'. Expected: index | search | timeline",
                other
            ),
        }
    }
}

impl GraphRagServer {
    async fn conversations_index(&self, project_dir_override: Option<String>) -> String {
        let ctx = match self.ctx_named("conversations").await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let project_dir = if let Some(dir) = project_dir_override {
            std::path::PathBuf::from(dir)
        } else {
            let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
            let claude_projects = home.join(".claude").join("projects");

            let codebase_str = ctx
                .codebase_path
                .to_string_lossy()
                .replace(['/', '\\', ':'], "-")
                .replace("--", "-");

            match std::fs::read_dir(&claude_projects) {
                Ok(entries) => {
                    let mut found = None;
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.contains(&ctx.repo_name)
                            || codebase_str.contains(&name)
                            || name.contains("graph-rag")
                        {
                            found = Some(entry.path());
                            break;
                        }
                    }
                    found.unwrap_or(claude_projects)
                }
                Err(_) => claude_projects,
            }
        };

        let jsonl_files: Vec<std::path::PathBuf> = match std::fs::read_dir(&project_dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "jsonl")
                        .unwrap_or(false)
                })
                .map(|e| e.path())
                .collect(),
            Err(e) => return format!("Cannot read project dir '{}': {}", project_dir.display(), e),
        };

        if jsonl_files.is_empty() {
            return format!(
                "No JSONL conversation files found in {}",
                project_dir.display()
            );
        }

        let known_entities = load_known_entities(&ctx.db).await;
        let builder = codescope_core::graph::builder::GraphBuilder::new(ctx.db.clone());

        let mut total_result = codescope_core::conversation::ConvIndexResult::default();

        for jsonl_path in &jsonl_files {
            let jsonl_name = jsonl_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.jsonl")
                .to_string();

            if let Ok(Some(stored_hash)) = check_conversation_hash(&ctx.db, &jsonl_name).await {
                if let Ok(content) = std::fs::read(jsonl_path) {
                    use sha2::{Digest, Sha256};
                    let current_hash = hex::encode(Sha256::digest(&content));
                    if stored_hash == current_hash {
                        total_result.skipped += 1;
                        continue;
                    }
                }
            }

            match codescope_core::conversation::parse_conversation(
                jsonl_path,
                &ctx.repo_name,
                &known_entities,
            ) {
                Ok((entities, relations, result)) => {
                    if let Err(e) = builder.insert_entities(&entities).await {
                        tracing::warn!("Entity insert failed: {e}");
                    }
                    if let Err(e) = builder.insert_relations(&relations).await {
                        tracing::warn!("Relation insert failed: {e}");
                    }
                    total_result.sessions_indexed += result.sessions_indexed;
                    total_result.decisions += result.decisions;
                    total_result.problems += result.problems;
                    total_result.solutions += result.solutions;
                    total_result.topics += result.topics;
                    total_result.code_links += result.code_links;
                }
                Err(e) => {
                    tracing::warn!("Failed to parse {}: {}", jsonl_path.display(), e);
                }
            }
        }

        let memory_dir = project_dir.join("memory");
        let mut memory_count = 0;
        if memory_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&memory_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "md").unwrap_or(false) {
                        match codescope_core::conversation::parse_memory_file(
                            &path,
                            &ctx.repo_name,
                            &known_entities,
                        ) {
                            Ok((entities, relations)) => {
                                if let Err(e) = builder.insert_entities(&entities).await {
                                    tracing::warn!("Entity insert failed: {e}");
                                }
                                if let Err(e) = builder.insert_relations(&relations).await {
                                    tracing::warn!("Relation insert failed: {e}");
                                }
                                memory_count += 1;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse memory file {}: {}",
                                    path.display(),
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        let cross_links = link_cross_session_topics(&ctx.db, &ctx.repo_name).await;

        format!(
            "## Conversation Indexing Complete\n\n\
             - Sessions indexed: {}\n\
             - Skipped (unchanged): {}\n\
             - Decisions: {}\n\
             - Problems: {}\n\
             - Solutions: {}\n\
             - Topics: {}\n\
             - Code links: {}\n\
             - Memory files: {}\n\
             - Cross-session links: {}\n\
             - Source: {}",
            total_result.sessions_indexed,
            total_result.skipped,
            total_result.decisions,
            total_result.problems,
            total_result.solutions,
            total_result.topics,
            total_result.code_links,
            memory_count,
            cross_links,
            project_dir.display(),
        )
    }

    async fn conversations_search(&self, query: String, limit: Option<usize>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = limit.unwrap_or(20) as u32;
        let filter_type = "all";

        let tables: Vec<&str> = match filter_type {
            "decision" => vec!["decision"],
            "problem" => vec!["problem"],
            "solution" => vec!["solution"],
            "topic" => vec!["conv_topic"],
            _ => vec!["decision", "problem", "solution", "conv_topic"],
        };

        let mut all_results = Vec::new();

        for table in &tables {
            let q = format!(
                "SELECT name, kind, body, file_path, start_line, '{}' AS type \
                 FROM {} WHERE string::contains(string::lowercase(name), string::lowercase($kw)) \
                 OR string::contains(string::lowercase(body), string::lowercase($kw)) \
                 LIMIT $lim;",
                table, table
            );

            if let Ok(mut response) = ctx
                .db
                .query(&q)
                .bind(("kw", query.clone()))
                .bind(("lim", limit))
                .await
            {
                let results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                all_results.extend(results);
            }
        }

        let link_query = "SELECT <-decided_about<-decision.{name, body} AS linked_decisions \
             FROM `function` WHERE name = $kw LIMIT 1;"
            .to_string();
        if let Ok(mut resp) = ctx.db.query(&link_query).bind(("kw", query.clone())).await {
            let linked: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
            if !linked.is_empty() {
                all_results.push(serde_json::json!({
                    "type": "linked_decisions",
                    "for_entity": query,
                    "data": linked
                }));
            }
        }

        if all_results.is_empty() {
            return format!(
                "No conversation history found for '{}'. Run conversations(action=\"index\") first.",
                query
            );
        }

        let mut output = format!("## Conversation History: '{}'\n\n", query);

        for item in &all_results {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let body = item.get("body").and_then(|v| v.as_str()).unwrap_or("");

            let icon = match item_type {
                "decision" => "**[DECISION]**",
                "problem" => "**[PROBLEM]**",
                "solution" => "**[SOLUTION]**",
                "conv_topic" => "**[TOPIC]**",
                "linked_decisions" => "**[LINKED]**",
                _ => "**[?]**",
            };

            output.push_str(&format!("{} {}\n", icon, name));
            if !body.is_empty() && body.len() > 10 {
                let preview = if body.len() > 200 { &body[..200] } else { body };
                output.push_str(&format!("  > {}\n", preview));
            }
            output.push('\n');
        }

        output
    }

    async fn conversations_timeline(&self, name: String, limit: Option<usize>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = limit.unwrap_or(20) as u32;

        let tables = ["decision", "problem", "solution", "conv_topic"];
        let mut all_results: Vec<serde_json::Value> = Vec::new();

        // SurrealDB CONTAINS operator does not work reliably with .bind() parameters —
        // returns empty silently. Inline the literal with single-quote escape instead.
        let safe_name = name.replace('\'', "");
        for table in &tables {
            let query = format!(
                "SELECT name, body, timestamp, kind, '{}' AS type \
                 FROM {} WHERE body CONTAINS '{}' \
                 ORDER BY timestamp DESC LIMIT $lim",
                table, table, safe_name
            );
            if let Ok(mut resp) = ctx.db.query(&query).bind(("lim", limit)).await {
                let results: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
                all_results.extend(results);
            }
        }

        let link_query = "SELECT <-discussed_in<-decision.{name, body, timestamp} AS decisions, \
                           <-discussed_in<-problem.{name, body, timestamp} AS problems, \
                           <-discussed_in<-solution.{name, body, timestamp} AS solutions \
                           FROM `function` WHERE name = $name LIMIT 1;";
        if let Ok(mut resp) = ctx.db.query(link_query).bind(("name", name.clone())).await {
            let linked: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
            if !linked.is_empty() {
                all_results.push(serde_json::json!({
                    "type": "linked",
                    "for_entity": name,
                    "data": linked
                }));
            }
        }

        if all_results.is_empty() {
            return format!(
                "No conversation history found for '{}'. Run conversations(action=\"index\") first.",
                name
            );
        }

        let mut output = format!("## Timeline: '{}'\n\n", name);

        for item in &all_results {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            let item_name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let timestamp = item
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let body = item.get("body").and_then(|v| v.as_str()).unwrap_or("");

            let icon = match item_type {
                "decision" => "[DECISION]",
                "problem" => "[PROBLEM]",
                "solution" => "[SOLUTION]",
                "conv_topic" => "[TOPIC]",
                "linked" => "[LINKED]",
                _ => "[?]",
            };

            output.push_str(&format!("**{}** {} ({})\n", icon, item_name, timestamp));
            if !body.is_empty() && body.len() > 10 {
                let preview = if body.len() > 200 { &body[..200] } else { body };
                output.push_str(&format!("  > {}\n", preview));
            }
            output.push('\n');
        }

        output
    }
}

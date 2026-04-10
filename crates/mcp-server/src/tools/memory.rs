//! Shared memory tools: memory_save, memory_search, memory_pin.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = memory_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Save a persistent memory note that survives across sessions
    #[tool(
        description = "Save a persistent memory note that survives across sessions and is shared between all agents \
        (Claude Code, Cursor, etc.) connected to this project. Use for recording: preferences, conventions, \
        important context, architectural notes, TODOs. Memories are searchable via conversation_search."
    )]
    async fn memory_save(
        &self,
        Parameters(params): Parameters<NaturalLanguageQueryParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let text = &params.question;
        let ts = chrono::Utc::now().to_rfc3339();
        let slug = text
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
            .chars()
            .take(60)
            .collect::<String>();
        let qname = format!("{}:memory:{}", ctx.repo_name, slug);

        let q = "UPSERT conv_topic SET name = $name, qualified_name = $qname, \
                 body = $body, repo = $repo, language = 'memory', kind = 'shared_memory', \
                 file_path = 'memory', start_line = 0, end_line = 0, timestamp = $ts";
        match ctx
            .db
            .query(q)
            .bind(("name", text.chars().take(100).collect::<String>()))
            .bind(("qname", qname))
            .bind(("body", text.to_string()))
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("ts", ts))
            .await
        {
            Ok(_) => format!("Memory saved. Accessible by all agents connected to '{}'.", ctx.repo_name),
            Err(e) => format!("Error saving memory: {}", e),
        }
    }

    /// Search shared memories and conversation history
    #[tool(
        description = "Search shared memories, decisions, and conversation history across all sessions. \
        Returns memories saved by any agent, plus auto-extracted decisions/problems/solutions."
    )]
    async fn memory_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = params.limit.unwrap_or(20) as i64;
        let safe = params.query.replace('\'', "");

        let scope_clause = if params.scope.is_some() {
            " AND scope ~ $scope"
        } else {
            ""
        };

        let q = format!(
            "SELECT name, body, kind, timestamp FROM conv_topic \
             WHERE repo = $repo AND (name ~ $search OR body ~ $search){scope} LIMIT $lim; \
             SELECT name, body, 'decision' AS kind, timestamp FROM decision \
             WHERE repo = $repo AND (name ~ $search OR body ~ $search){scope} LIMIT $lim; \
             SELECT name, body, 'problem' AS kind, timestamp FROM problem \
             WHERE repo = $repo AND (name ~ $search OR body ~ $search){scope} LIMIT $lim; \
             SELECT name, body, 'solution' AS kind, timestamp FROM solution \
             WHERE repo = $repo AND (name ~ $search OR body ~ $search){scope} LIMIT $lim",
            scope = scope_clause,
        );

        let scope_val = params.scope.unwrap_or_default();
        match ctx
            .db
            .query(&q)
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("search", safe))
            .bind(("lim", limit))
            .bind(("scope", scope_val))
            .await
        {
            Ok(mut r) => {
                let mut results: Vec<serde_json::Value> = Vec::new();
                for i in 0..4u32 {
                    let batch: Vec<serde_json::Value> = r.take(i as usize).unwrap_or_default();
                    results.extend(batch);
                }
                results.sort_by(|a, b| {
                    let ta = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    let tb = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    tb.cmp(ta)
                });
                results.truncate(limit as usize);
                if results.is_empty() {
                    return format!("No memories found for '{}'", params.query);
                }
                let mut output = format!("## Memory Search: '{}' ({} results)\n\n", params.query, results.len());
                for item in &results {
                    let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let body = item.get("body").and_then(|v| v.as_str()).unwrap_or("");
                    let ts = item.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    let date = if ts.len() >= 10 { &ts[..10] } else { ts };
                    let icon = match kind {
                        "shared_memory" => "[MEMORY]",
                        "decision" => "[DECISION]",
                        "problem" => "[PROBLEM]",
                        "solution" => "[SOLUTION]",
                        _ => "[NOTE]",
                    };
                    output.push_str(&format!("**{}** {} _{}_\n", icon, name, date));
                    if !body.is_empty() {
                        let preview = if body.len() > 150 { &body[..150] } else { body };
                        output.push_str(&format!("> {}\n\n", preview));
                    }
                }
                output
            }
            Err(e) => format!("Error searching memory: {}", e),
        }
    }

    /// Pin or adjust the tier of a decision/problem/solution memory
    #[tool(
        description = "Pin or adjust the priority tier of a decision, problem, or solution. \
        Tier 0 = critical (always shown, marked [PINNED]), 1 = important, 2 = contextual (default). \
        Matches by partial name across decision, problem, and solution tables."
    )]
    async fn memory_pin(&self, Parameters(params): Parameters<MemoryPinParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let tier = params.tier.min(2) as i64;

        let q = "UPDATE decision SET tier = $tier WHERE name ~ $name AND repo = $repo; \
                 UPDATE problem SET tier = $tier WHERE name ~ $name AND repo = $repo; \
                 UPDATE solution SET tier = $tier WHERE name ~ $name AND repo = $repo;";

        match ctx
            .db
            .query(q)
            .bind(("tier", tier))
            .bind(("name", params.name.clone()))
            .bind(("repo", ctx.repo_name.clone()))
            .await
        {
            Ok(mut r) => {
                let mut total = 0usize;
                for i in 0..3u32 {
                    let updated: Vec<serde_json::Value> = r.take(i as usize).unwrap_or_default();
                    total += updated.len();
                }
                if total == 0 {
                    format!("No matching memories found for '{}'. Try a broader name.", params.name)
                } else {
                    let tier_label = match params.tier {
                        0 => "critical (always shown)",
                        1 => "important",
                        _ => "contextual",
                    };
                    format!("Updated {} record(s) matching '{}' to tier {} ({}).", total, params.name, params.tier, tier_label)
                }
            }
            Err(e) => format!("Error pinning memory: {}", e),
        }
    }
}

//! Unified memory tool: save, search, pin — dispatched via action param.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = memory_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Unified memory tool — action=save|search|pin
    #[tool(
        description = "Memory: action=save|search|pin. save: persist note. search: query decisions/problems/solutions. pin: adjust tier."
    )]
    async fn memory(&self, Parameters(params): Parameters<MemoryParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        match params.action.as_str() {
            "save" => {
                let text = match params.text.as_deref() {
                    Some(t) if !t.is_empty() => t,
                    _ => return "action=save requires 'text' parameter".into(),
                };
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
                    Ok(_) => format!(
                        "Memory saved. Accessible by all agents connected to '{}'.",
                        ctx.repo_name
                    ),
                    Err(e) => format!("Error saving memory: {}", e),
                }
            }

            "search" => {
                let query = match params.text.as_deref() {
                    Some(t) if !t.is_empty() => t,
                    _ => return "action=search requires 'text' parameter (the query)".into(),
                };
                let limit = params.limit.unwrap_or(20) as i64;
                let safe = query.replace('\'', "");

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
                            let batch: Vec<serde_json::Value> =
                                r.take(i as usize).unwrap_or_default();
                            results.extend(batch);
                        }
                        results.sort_by(|a, b| {
                            let ta = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                            let tb = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                            tb.cmp(ta)
                        });
                        results.truncate(limit as usize);
                        if results.is_empty() {
                            return format!("No memories found for '{}'", query);
                        }
                        let mut output = format!(
                            "## Memory Search: '{}' ({} results)\n\n",
                            query,
                            results.len()
                        );
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

            "pin" => {
                let name = match params.text.as_deref() {
                    Some(t) if !t.is_empty() => t,
                    _ => return "action=pin requires 'text' parameter (the memory name)".into(),
                };
                let tier = params.tier.unwrap_or(2).min(2) as i64;

                let q = "UPDATE decision SET tier = $tier WHERE name ~ $name AND repo = $repo; \
                         UPDATE problem SET tier = $tier WHERE name ~ $name AND repo = $repo; \
                         UPDATE solution SET tier = $tier WHERE name ~ $name AND repo = $repo;";

                match ctx
                    .db
                    .query(q)
                    .bind(("tier", tier))
                    .bind(("name", name.to_string()))
                    .bind(("repo", ctx.repo_name.clone()))
                    .await
                {
                    Ok(mut r) => {
                        let mut total = 0usize;
                        for i in 0..3u32 {
                            let updated: Vec<serde_json::Value> =
                                r.take(i as usize).unwrap_or_default();
                            total += updated.len();
                        }
                        if total == 0 {
                            format!(
                                "No matching memories found for '{}'. Try a broader name.",
                                name
                            )
                        } else {
                            let tier_label = match tier {
                                0 => "critical (always shown)",
                                1 => "important",
                                _ => "contextual",
                            };
                            format!(
                                "Updated {} record(s) matching '{}' to tier {} ({}).",
                                total, name, tier, tier_label
                            )
                        }
                    }
                    Err(e) => format!("Error pinning memory: {}", e),
                }
            }

            other => format!("Unknown action '{}'. Valid: save, search, pin.", other),
        }
    }
}

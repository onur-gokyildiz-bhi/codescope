//! Unified knowledge graph tool: save, search, link, lint — dispatched via action param.
//! General-purpose knowledge management beyond code — concepts, entities, sources, claims.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::KnowledgeParams;
use crate::server::GraphRagServer;

#[tool_router(router = knowledge_router, vis = "pub(crate)")]
impl GraphRagServer {
    #[tool(
        description = "Knowledge graph: action=save|search|link|lint. save: store concept/entity. search: find by title/content/tags. link: create typed edge. lint: health check."
    )]
    async fn knowledge(&self, Parameters(params): Parameters<KnowledgeParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        match params.action.as_str() {
            "save" => {
                let title = match params.title.as_deref() {
                    Some(t) if !t.is_empty() => t.to_string(),
                    _ => return "action=save requires 'title' parameter".into(),
                };
                let content = match params.content.as_deref() {
                    Some(c) if !c.is_empty() => c.to_string(),
                    _ => return "action=save requires 'content' parameter".into(),
                };
                let kind = match params.kind.as_deref() {
                    Some(k) if !k.is_empty() => k.to_string(),
                    _ => return "action=save requires 'kind' parameter".into(),
                };

                let id = crate::server::slugify(&title);
                let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
                let tags_json = params
                    .tags
                    .as_ref()
                    .map(|t| {
                        let items: Vec<String> = t.iter().map(|s| format!("'{}'", s)).collect();
                        format!("[{}]", items.join(", "))
                    })
                    .unwrap_or_else(|| "[]".to_string());

                let query = format!(
                    "UPSERT knowledge:{id} SET \
                     title = $title, \
                     content = $content, \
                     kind = $kind, \
                     repo = $repo, \
                     source_url = $source_url, \
                     confidence = $confidence, \
                     tags = {tags_json}, \
                     created_at = created_at ?? $now, \
                     updated_at = $now",
                );

                match ctx
                    .db
                    .query(&query)
                    .bind(("title", title.clone()))
                    .bind(("content", content))
                    .bind(("kind", kind.clone()))
                    .bind(("repo", ctx.repo_name.clone()))
                    .bind(("source_url", params.source_url.unwrap_or_default()))
                    .bind((
                        "confidence",
                        params.confidence.unwrap_or_else(|| "medium".into()),
                    ))
                    .bind(("now", now))
                    .await
                {
                    Ok(_) => format!(
                        "Knowledge saved: **{}** [{}]\nID: knowledge:{}\nTags: {:?}",
                        title,
                        kind,
                        id,
                        params.tags.unwrap_or_default()
                    ),
                    Err(e) => format!("Error saving knowledge: {}", e),
                }
            }

            "search" => {
                let query_str = match params.query.clone().or(params.title.clone()) {
                    Some(q) if !q.is_empty() => q,
                    _ => {
                        return "action=search requires 'query' (or 'title') parameter".into();
                    }
                };

                let limit = params.limit.unwrap_or(20);
                let kind_filter = params
                    .kind
                    .as_ref()
                    .map(|k| format!("AND kind = '{}'", k.replace('\'', "")))
                    .unwrap_or_default();

                // Inline tag literal because SurrealDB CONTAINS doesn't work reliably
                // with .bind() parameters (but does work with LET-declared vars).
                let tag_literal = query_str.replace('\'', "");
                let query = format!(
                    "SELECT title, kind, content, confidence, source_url, tags, created_at, updated_at \
                     FROM knowledge \
                     WHERE (string::contains(string::lowercase(title), string::lowercase($query)) \
                        OR string::contains(string::lowercase(content), string::lowercase($query)) \
                        OR tags CONTAINS '{tag_literal}') \
                     {kind_filter} \
                     ORDER BY updated_at DESC \
                     LIMIT {limit}"
                );

                match ctx
                    .db
                    .query(&query)
                    .bind(("query", query_str.clone()))
                    .await
                {
                    Ok(mut r) => {
                        let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if results.is_empty() {
                            return format!("No knowledge found for '{}'", query_str);
                        }
                        let mut output = format!("Found {} knowledge nodes:\n\n", results.len());
                        for (i, r) in results.iter().enumerate() {
                            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                            let kind = r.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                            let confidence =
                                r.get("confidence").and_then(|v| v.as_str()).unwrap_or("-");
                            let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            let preview: String = content.chars().take(150).collect();
                            let tags = r
                                .get("tags")
                                .and_then(|v| v.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|t| t.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default();
                            output.push_str(&format!(
                                "{}. **{}** [{}] (confidence: {})\n   {}{}\n",
                                i + 1,
                                title,
                                kind,
                                confidence,
                                preview,
                                if content.len() > 150 { "..." } else { "" }
                            ));
                            if !tags.is_empty() {
                                output.push_str(&format!("   tags: {}\n", tags));
                            }
                            output.push('\n');
                        }
                        output
                    }
                    Err(e) => format!("Error searching knowledge: {}", e),
                }
            }

            "link" => {
                let from_entity = match params.from_entity.as_deref() {
                    Some(v) if !v.is_empty() => v.to_string(),
                    _ => return "action=link requires 'from_entity' parameter".into(),
                };
                let to_entity = match params.to_entity.as_deref() {
                    Some(v) if !v.is_empty() => v.to_string(),
                    _ => return "action=link requires 'to_entity' parameter".into(),
                };
                let relation = match params.relation.as_deref() {
                    Some(v) if !v.is_empty() => v.to_string(),
                    _ => return "action=link requires 'relation' parameter".into(),
                };

                let from_id = crate::server::slugify(&from_entity);
                let to_id = crate::server::slugify(&to_entity);
                let edge_table = match relation.as_str() {
                    "supports" => "supports",
                    "contradicts" => "contradicts",
                    _ => "related_to",
                };
                let context_set = params
                    .context
                    .as_ref()
                    .map(|c| format!(", context = '{}'", c.replace('\'', "''")))
                    .unwrap_or_default();

                // Try knowledge→knowledge first, then knowledge→code, then code→knowledge
                let attempts = [
                    format!(
                        "RELATE knowledge:{from_id}->{edge_table}->knowledge:{to_id} \
                         SET relation = $relation{context_set}"
                    ),
                    format!(
                        "RELATE knowledge:{from_id}->{edge_table}->`function`:{to_id} \
                         SET relation = $relation{context_set}"
                    ),
                    format!(
                        "RELATE `function`:{from_id}->{edge_table}->knowledge:{to_id} \
                         SET relation = $relation{context_set}"
                    ),
                ];

                for attempt in &attempts {
                    if ctx
                        .db
                        .query(attempt)
                        .bind(("relation", relation.clone()))
                        .await
                        .is_ok()
                    {
                        return format!(
                            "Linked: **{}** —[{}]→ **{}**{}",
                            from_entity,
                            relation,
                            to_entity,
                            params
                                .context
                                .as_ref()
                                .map(|c| format!(" ({})", c))
                                .unwrap_or_default()
                        );
                    }
                }

                format!(
                    "Could not link '{}' to '{}'. Ensure both entities exist in the graph.",
                    from_entity, to_entity
                )
            }

            "lint" => {
                let check = params.check.unwrap_or_else(|| "all".to_string());

                let mut report = String::from("## Knowledge Graph Health Report\n\n");
                let mut issues = Vec::new();

                // Stats
                let stats_q = "SELECT count() AS cnt, kind FROM knowledge GROUP BY kind";
                if let Ok(mut r) = ctx.db.query(stats_q).await {
                    let stats: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                    let total: i64 = stats
                        .iter()
                        .map(|s| s.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0))
                        .sum();
                    report.push_str(&format!("**Total knowledge nodes:** {}\n", total));
                    for s in &stats {
                        let kind = s.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                        let cnt = s.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);
                        report.push_str(&format!("  - {}: {}\n", kind, cnt));
                    }
                    report.push('\n');
                }

                // Check: orphans
                if check == "all" || check == "orphans" {
                    let orphan_q = "SELECT title, kind FROM knowledge \
                                   WHERE count(<-supports) = 0 AND count(->supports) = 0 \
                                   AND count(<-contradicts) = 0 AND count(->contradicts) = 0 \
                                   AND count(<-related_to) = 0 AND count(->related_to) = 0 \
                                   LIMIT 20";
                    if let Ok(mut r) = ctx.db.query(orphan_q).await {
                        let orphans: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if !orphans.is_empty() {
                            issues.push(format!("**Orphan nodes:** {} (no edges)", orphans.len()));
                            for o in &orphans {
                                let title = o.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                                let kind = o.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                                report.push_str(&format!("  - orphan: {} [{}]\n", title, kind));
                            }
                        }
                    }
                }

                // Check: contradictions
                if check == "all" || check == "contradictions" {
                    let contra_q =
                        "SELECT title, content FROM knowledge WHERE kind = 'contradiction' LIMIT 10";
                    if let Ok(mut r) = ctx.db.query(contra_q).await {
                        let contras: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if !contras.is_empty() {
                            issues.push(format!(
                                "**Unresolved contradictions:** {} [high severity]",
                                contras.len()
                            ));
                            for c in &contras {
                                let title = c.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                                report.push_str(&format!("  - contradiction: {}\n", title));
                            }
                        }
                    }
                }

                // Check: low confidence
                if check == "all" || check == "low_confidence" {
                    let low_q =
                        "SELECT title, kind FROM knowledge WHERE confidence = 'low' LIMIT 20";
                    if let Ok(mut r) = ctx.db.query(low_q).await {
                        let low: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if !low.is_empty() {
                            issues.push(format!(
                                "**Low-confidence nodes:** {} — consider /autoresearch to corroborate",
                                low.len()
                            ));
                        }
                    }
                }

                if issues.is_empty() {
                    report.push_str("**No issues found.** Knowledge graph is healthy.\n");
                } else {
                    report.push_str("\n### Issues\n\n");
                    for (i, issue) in issues.iter().enumerate() {
                        report.push_str(&format!("{}. {}\n", i + 1, issue));
                    }
                }

                report
            }

            other => format!(
                "Unknown action '{}'. Valid: save, search, link, lint.",
                other
            ),
        }
    }
}

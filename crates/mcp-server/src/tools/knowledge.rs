//! Unified knowledge graph tool: save, search, link, lint — dispatched via action param.
//! General-purpose knowledge management beyond code — concepts, entities, sources, claims.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::helpers::{connect_global_db, GLOBAL_REPO};
use crate::params::KnowledgeParams;
use crate::server::GraphRagServer;

/// Parsed scope selection from the `scope` param.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Project,
    Global,
    Both,
}

fn parse_scope(raw: Option<&str>) -> Scope {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("global") => Scope::Global,
        Some("both") => Scope::Both,
        // default (None, empty, "project", or unknown) -> project
        _ => Scope::Project,
    }
}

/// Run the core save-query against a given DB/repo. Returns Ok on success.
async fn save_to_db(
    db: &Surreal<Db>,
    repo: &str,
    id: &str,
    title: &str,
    content: &str,
    kind: &str,
    source_url: &str,
    confidence: &str,
    tags_json: &str,
    now: &str,
) -> Result<(), surrealdb::Error> {
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
    db.query(&query)
        .bind(("title", title.to_string()))
        .bind(("content", content.to_string()))
        .bind(("kind", kind.to_string()))
        .bind(("repo", repo.to_string()))
        .bind(("source_url", source_url.to_string()))
        .bind(("confidence", confidence.to_string()))
        .bind(("now", now.to_string()))
        .await?
        .check()?;
    Ok(())
}

/// Search one DB for knowledge entities. Returns the raw rows.
async fn search_db(
    db: &Surreal<Db>,
    query_str: &str,
    kind_filter: &str,
    limit: usize,
) -> Vec<serde_json::Value> {
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
    match db
        .query(&query)
        .bind(("query", query_str.to_string()))
        .await
    {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

#[tool_router(router = knowledge_router, vis = "pub(crate)")]
impl GraphRagServer {
    #[tool(
        description = "Knowledge graph: action=save|search|link|lint. Optional scope=project|global|both (default: project). global: cross-project shared knowledge."
    )]
    #[tracing::instrument(skip_all, fields(action = %params.action))]
    async fn knowledge(&self, Parameters(params): Parameters<KnowledgeParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let scope = parse_scope(params.scope.as_deref());

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

                let source_url = params.source_url.clone().unwrap_or_default();
                let confidence = params.confidence.clone().unwrap_or_else(|| "medium".into());

                // Scope "both" is search-only; for save it behaves like "global"
                // (avoids silent double-writes surprising the caller).
                let use_global = matches!(scope, Scope::Global | Scope::Both);

                let save_result = if use_global {
                    let gdb = match connect_global_db().await {
                        Ok(d) => d,
                        Err(e) => return format!("Error opening global DB: {}", e),
                    };
                    save_to_db(
                        &gdb,
                        GLOBAL_REPO,
                        &id,
                        &title,
                        &content,
                        &kind,
                        &source_url,
                        &confidence,
                        &tags_json,
                        &now,
                    )
                    .await
                } else {
                    save_to_db(
                        &ctx.db,
                        &ctx.repo_name,
                        &id,
                        &title,
                        &content,
                        &kind,
                        &source_url,
                        &confidence,
                        &tags_json,
                        &now,
                    )
                    .await
                };

                match save_result {
                    Ok(_) => format!(
                        "Knowledge saved [{}]: **{}** [{}]\nID: knowledge:{}\nTags: {:?}",
                        if use_global { "global" } else { "project" },
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

                // Gather rows from one or both DBs, tag with origin for display.
                let mut tagged: Vec<(String, serde_json::Value)> = Vec::new();

                if matches!(scope, Scope::Project | Scope::Both) {
                    for row in search_db(&ctx.db, &query_str, &kind_filter, limit).await {
                        tagged.push(("project".to_string(), row));
                    }
                }
                if matches!(scope, Scope::Global | Scope::Both) {
                    match connect_global_db().await {
                        Ok(gdb) => {
                            for row in search_db(&gdb, &query_str, &kind_filter, limit).await {
                                tagged.push(("global".to_string(), row));
                            }
                        }
                        Err(e) => {
                            // Don't fail the whole search if global DB is unreachable;
                            // fall through with whatever project returned.
                            tracing::warn!("global DB unavailable for search: {}", e);
                        }
                    }
                }

                // Dedupe by title (prefer whichever was seen first — project first
                // in Both mode, so project wins over global on conflict).
                if matches!(scope, Scope::Both) {
                    let mut seen = std::collections::HashSet::new();
                    tagged.retain(|(_, r)| {
                        let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        seen.insert(title.to_string())
                    });
                }

                // Apply the limit across the merged set and sort by updated_at DESC.
                tagged.sort_by(|a, b| {
                    let ta = a.1.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
                    let tb = b.1.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
                    tb.cmp(ta)
                });
                tagged.truncate(limit);

                if tagged.is_empty() {
                    return format!("No knowledge found for '{}'", query_str);
                }

                let mut output = format!("Found {} knowledge nodes:\n\n", tagged.len());
                for (i, (origin, r)) in tagged.iter().enumerate() {
                    let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let kind = r.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                    let confidence = r.get("confidence").and_then(|v| v.as_str()).unwrap_or("-");
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
                    let origin_tag = if matches!(scope, Scope::Both) {
                        format!(" <{}>", origin)
                    } else {
                        String::new()
                    };
                    output.push_str(&format!(
                        "{}. **{}**{} [{}] (confidence: {})\n   {}{}\n",
                        i + 1,
                        title,
                        origin_tag,
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

                // Select DB by scope. For "both" we link in the global DB (link
                // semantics don't cross DBs anyway — edges must live with nodes).
                let use_global = matches!(scope, Scope::Global | Scope::Both);
                let link_db: Surreal<Db> = if use_global {
                    match connect_global_db().await {
                        Ok(d) => d,
                        Err(e) => return format!("Error opening global DB: {}", e),
                    }
                } else {
                    ctx.db.clone()
                };

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
                    if link_db
                        .query(attempt)
                        .bind(("relation", relation.clone()))
                        .await
                        .is_ok()
                    {
                        return format!(
                            "Linked [{}]: **{}** —[{}]→ **{}**{}",
                            if use_global { "global" } else { "project" },
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

                // Lint runs against the chosen DB (default: project). "both" falls back to project.
                let use_global = matches!(scope, Scope::Global);
                let lint_db: Surreal<Db> = if use_global {
                    match connect_global_db().await {
                        Ok(d) => d,
                        Err(e) => return format!("Error opening global DB: {}", e),
                    }
                } else {
                    ctx.db.clone()
                };

                let mut report = format!(
                    "## Knowledge Graph Health Report [{}]\n\n",
                    if use_global { "global" } else { "project" }
                );
                let mut issues = Vec::new();

                // Stats
                let stats_q = "SELECT count() AS cnt, kind FROM knowledge GROUP BY kind";
                if let Ok(mut r) = lint_db.query(stats_q).await {
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
                    if let Ok(mut r) = lint_db.query(orphan_q).await {
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
                    if let Ok(mut r) = lint_db.query(contra_q).await {
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
                    if let Ok(mut r) = lint_db.query(low_q).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scope_defaults_to_project() {
        assert!(matches!(parse_scope(None), Scope::Project));
        assert!(matches!(parse_scope(Some("")), Scope::Project));
        assert!(matches!(parse_scope(Some("project")), Scope::Project));
        assert!(matches!(parse_scope(Some("bogus")), Scope::Project));
    }

    #[test]
    fn parse_scope_recognises_global_and_both() {
        assert!(matches!(parse_scope(Some("global")), Scope::Global));
        assert!(matches!(parse_scope(Some("GLOBAL")), Scope::Global));
        assert!(matches!(parse_scope(Some("both")), Scope::Both));
        assert!(matches!(parse_scope(Some(" both ")), Scope::Both));
    }

    /// End-to-end: saving with scope=global writes to a different DB than
    /// scope=project (different repo_name ctx), and scope=both returns the union.
    #[tokio::test]
    async fn global_scope_is_visible_across_project_contexts() {
        use surrealdb::engine::local::Mem;

        // Two project DBs simulating two different repos.
        let project_a: Surreal<Db> = Surreal::new::<Mem>(()).await.unwrap();
        project_a
            .use_ns("codescope")
            .use_db("repo_a")
            .await
            .unwrap();
        codescope_core::graph::schema::init_schema(&project_a)
            .await
            .unwrap();

        let project_b: Surreal<Db> = Surreal::new::<Mem>(()).await.unwrap();
        project_b
            .use_ns("codescope")
            .use_db("repo_b")
            .await
            .unwrap();
        codescope_core::graph::schema::init_schema(&project_b)
            .await
            .unwrap();

        // A shared in-memory "global" DB stands in for ~/.codescope/db/_global.
        let global: Surreal<Db> = Surreal::new::<Mem>(()).await.unwrap();
        global
            .use_ns("codescope")
            .use_db(GLOBAL_REPO)
            .await
            .unwrap();
        codescope_core::graph::schema::init_schema(&global)
            .await
            .unwrap();

        let now = "2026-04-14T00:00:00";

        // Save a project-scope entity to repo_a.
        save_to_db(
            &project_a,
            "repo_a",
            "local_decision",
            "Local Decision",
            "only visible in repo_a",
            "decision",
            "",
            "medium",
            "[]",
            now,
        )
        .await
        .unwrap();

        // Save a global-scope entity.
        save_to_db(
            &global,
            GLOBAL_REPO,
            "global_decision",
            "Global Decision",
            "visible across projects",
            "decision",
            "",
            "high",
            "['status:done']",
            now,
        )
        .await
        .unwrap();

        // From repo_b's perspective: scope=project must NOT find the global entity.
        let b_only = search_db(&project_b, "Global", "", 20).await;
        assert!(
            b_only.is_empty(),
            "project scope must not leak global entities"
        );

        // scope=global finds it from repo_b.
        let b_global = search_db(&global, "Global", "", 20).await;
        assert_eq!(b_global.len(), 1, "global scope finds global entity");
        assert_eq!(
            b_global[0].get("title").and_then(|v| v.as_str()),
            Some("Global Decision")
        );

        // scope=both (union): from repo_a we see both entries, deduped by title.
        let mut union: Vec<serde_json::Value> = Vec::new();
        union.extend(search_db(&project_a, "Decision", "", 20).await);
        union.extend(search_db(&global, "Decision", "", 20).await);
        let mut seen = std::collections::HashSet::new();
        union.retain(|r| {
            let t = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
            seen.insert(t.to_string())
        });
        assert_eq!(union.len(), 2, "union returns both entries");
        let titles: std::collections::HashSet<_> = union
            .iter()
            .filter_map(|r| r.get("title").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        assert!(titles.contains("Local Decision"));
        assert!(titles.contains("Global Decision"));
    }
}

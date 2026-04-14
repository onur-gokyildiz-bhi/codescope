//! Knowledge graph tools: knowledge_save, knowledge_search, knowledge_link, knowledge_lint.
//! General-purpose knowledge management beyond code — concepts, entities, sources, claims.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use serde::{Deserialize, Serialize};

use crate::server::GraphRagServer;

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct KnowledgeSaveParams {
    /// Title of the knowledge entity
    pub title: String,
    /// Full content / description
    pub content: String,
    /// Kind: concept, entity, source, claim, contradiction, question
    pub kind: String,
    /// Source URL if ingested from web
    #[serde(alias = "url")]
    pub source_url: Option<String>,
    /// Confidence level: high, medium, low
    pub confidence: Option<String>,
    /// Tags for categorization
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct KnowledgeSearchParams {
    /// Search query
    pub query: String,
    /// Filter by kind (optional)
    pub kind: Option<String>,
    /// Max results (default 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct KnowledgeLinkParams {
    /// Source entity title (knowledge or code entity name)
    pub from_entity: String,
    /// Target entity title (knowledge or code entity name)
    pub to_entity: String,
    /// Relation type: supports, contradicts, related_to, implemented_by, uses, extends
    #[serde(alias = "relation_type")]
    pub relation: String,
    /// Optional context explaining the relationship
    pub context: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct KnowledgeLintParams {
    /// Which check to run: orphans, low_confidence, contradictions, unlinked_code, stale, all
    #[serde(default = "default_lint_check")]
    pub check: String,
}

fn default_lint_check() -> String {
    "all".to_string()
}

#[tool_router(router = knowledge_router, vis = "pub(crate)")]
impl GraphRagServer {
    #[tool(
        description = "Save knowledge entity: concept, entity, source, claim, decision."
    )]
    async fn knowledge_save(&self, Parameters(params): Parameters<KnowledgeSaveParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let id = crate::server::slugify(&params.title);
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
            .bind(("title", params.title.clone()))
            .bind(("content", params.content.clone()))
            .bind(("kind", params.kind.clone()))
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
                params.title,
                params.kind,
                id,
                params.tags.unwrap_or_default()
            ),
            Err(e) => format!("Error saving knowledge: {}", e),
        }
    }

    #[tool(
        description = "Search knowledge graph: concepts, entities, sources, claims."
    )]
    async fn knowledge_search(
        &self,
        Parameters(params): Parameters<KnowledgeSearchParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let limit = params.limit.unwrap_or(20);
        let kind_filter = params
            .kind
            .as_ref()
            .map(|k| format!("AND kind = '{}'", k))
            .unwrap_or_default();

        let query = format!(
            "SELECT title, kind, content, confidence, source_url, tags, created_at \
             FROM knowledge \
             WHERE (string::contains(string::lowercase(title), string::lowercase($query)) \
                OR string::contains(string::lowercase(content), string::lowercase($query))) \
             {kind_filter} \
             ORDER BY updated_at DESC \
             LIMIT {limit}"
        );

        match ctx
            .db
            .query(&query)
            .bind(("query", params.query.clone()))
            .await
        {
            Ok(mut r) => {
                let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if results.is_empty() {
                    return format!("No knowledge found for '{}'", params.query);
                }
                let mut output = format!("Found {} knowledge nodes:\n\n", results.len());
                for (i, r) in results.iter().enumerate() {
                    let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let kind = r.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                    let confidence = r.get("confidence").and_then(|v| v.as_str()).unwrap_or("-");
                    let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    let preview: String = content.chars().take(150).collect();
                    output.push_str(&format!(
                        "{}. **{}** [{}] (confidence: {})\n   {}{}\n\n",
                        i + 1,
                        title,
                        kind,
                        confidence,
                        preview,
                        if content.len() > 150 { "..." } else { "" }
                    ));
                }
                output
            }
            Err(e) => format!("Error searching knowledge: {}", e),
        }
    }

    #[tool(
        description = "Create typed edge between two knowledge or code entities."
    )]
    async fn knowledge_link(&self, Parameters(params): Parameters<KnowledgeLinkParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let from_id = crate::server::slugify(&params.from_entity);
        let to_id = crate::server::slugify(&params.to_entity);
        let edge_table = match params.relation.as_str() {
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
                .bind(("relation", params.relation.clone()))
                .await
                .is_ok()
            {
                return format!(
                    "Linked: **{}** —[{}]→ **{}**{}",
                    params.from_entity,
                    params.relation,
                    params.to_entity,
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
            params.from_entity, params.to_entity
        )
    }

    #[tool(
        description = "Knowledge graph health check: orphans, missing links, stale claims."
    )]
    async fn knowledge_lint(&self, Parameters(params): Parameters<KnowledgeLintParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

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
        if params.check == "all" || params.check == "orphans" {
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
        if params.check == "all" || params.check == "contradictions" {
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
        if params.check == "all" || params.check == "low_confidence" {
            let low_q = "SELECT title, kind FROM knowledge WHERE confidence = 'low' LIMIT 20";
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
}

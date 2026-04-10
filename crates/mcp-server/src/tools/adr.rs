//! Architecture Decision Records: manage_adr.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = adr_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Manage Architecture Decision Records
    #[tool(description = "Manage Architecture Decision Records (ADRs). Actions: \
        'list' — show all recorded decisions, \
        'create' — record a new architectural decision with title and body, \
        'get' — retrieve a specific ADR by ID. \
        ADRs are stored in the graph and linked to conversation history.")]
    async fn manage_adr(&self, Parameters(params): Parameters<ManageAdrParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        match params.action.as_str() {
            "list" => {
                let q = "SELECT name, body, timestamp, qualified_name FROM decision WHERE repo = $repo ORDER BY timestamp DESC LIMIT 50";
                match ctx.db.query(q).bind(("repo", ctx.repo_name.clone())).await {
                    Ok(mut r) => {
                        let decisions: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if decisions.is_empty() {
                            return "No ADRs found. Decisions are auto-extracted from conversations, or create one with action='create'.".into();
                        }
                        let mut output = format!("## Architecture Decision Records ({} total)\n\n", decisions.len());
                        for (i, d) in decisions.iter().enumerate() {
                            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let ts = d.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                            let body = d.get("body").and_then(|v| v.as_str()).unwrap_or("");
                            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
                            output.push_str(&format!("### ADR-{:03}: {}\n", i + 1, name));
                            if !date.is_empty() {
                                output.push_str(&format!("*Date: {}*\n\n", date));
                            }
                            if body.len() > 200 {
                                output.push_str(&format!("{}...\n\n", &body[..200]));
                            } else if !body.is_empty() {
                                output.push_str(&format!("{}\n\n", body));
                            }
                        }
                        output
                    }
                    Err(e) => format!("Error listing ADRs: {}", e),
                }
            }
            "create" => {
                let title = params.title.as_deref().unwrap_or("Untitled Decision");
                let body = params.body.as_deref().unwrap_or("");
                let qname = format!(
                    "{}:adr:{}",
                    ctx.repo_name,
                    title.to_lowercase().replace(' ', "_").chars().take(60).collect::<String>()
                );
                let ts = chrono::Utc::now().to_rfc3339();

                let q = "UPSERT decision SET name = $name, qualified_name = $qname, \
                         body = $body, repo = $repo, language = 'adr', \
                         file_path = 'adr', start_line = 0, end_line = 0, \
                         timestamp = $ts";
                match ctx.db.query(q)
                    .bind(("name", title.to_string()))
                    .bind(("qname", qname))
                    .bind(("body", body.to_string()))
                    .bind(("repo", ctx.repo_name.clone()))
                    .bind(("ts", ts))
                    .await
                {
                    Ok(_) => format!("ADR created: **{}**", title),
                    Err(e) => format!("Error creating ADR: {}", e),
                }
            }
            "get" => {
                let id = params.id.as_deref().unwrap_or("");
                let q = "SELECT * FROM decision WHERE name CONTAINS $search AND repo = $repo LIMIT 1";
                match ctx.db.query(q)
                    .bind(("search", id.to_string()))
                    .bind(("repo", ctx.repo_name.clone()))
                    .await
                {
                    Ok(mut r) => {
                        let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if let Some(d) = results.first() {
                            serde_json::to_string_pretty(d).unwrap_or_else(|_| "Error formatting".into())
                        } else {
                            format!("No ADR found matching '{}'", id)
                        }
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }
            _ => "Invalid action. Use 'list', 'create', or 'get'.".into(),
        }
    }
}

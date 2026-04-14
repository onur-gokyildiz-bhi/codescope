//! HTTP cross-service linking tool: http_analysis (modes: calls, endpoint_callers).

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = http_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// HTTP analysis — find client calls or endpoint callers.
    #[tool(
        description = "HTTP analysis: mode=calls|endpoint_callers. calls: find HTTP client calls (filter by method). endpoint_callers: find code calling a URL pattern."
    )]
    async fn http_analysis(&self, Parameters(params): Parameters<HttpAnalysisParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match params.mode.as_str() {
            "calls" => match gq.find_http_calls(params.query.as_deref()).await {
                Ok(results) => {
                    if results.is_empty() {
                        return "No HTTP client calls found in the codebase.".into();
                    }
                    let mut output = format!("Found {} HTTP client calls:\n\n", results.len());
                    for (i, r) in results.iter().enumerate() {
                        let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let file = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let line = r.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                        output.push_str(&format!("{}. **{}** ({}:{})\n", i + 1, name, file, line));
                    }
                    output
                }
                Err(e) => format!("Error: {}", e),
            },
            "endpoint_callers" => {
                let query = match params.query.as_deref() {
                    Some(q) if !q.is_empty() => q,
                    _ => {
                        return "Error: endpoint_callers mode requires a URL pattern in `query`."
                            .into();
                    }
                };
                match gq.find_endpoint_callers(query).await {
                    Ok(results) => {
                        if results.is_empty() {
                            return format!(
                                "No functions found calling endpoint matching '{}'",
                                query
                            );
                        }
                        let mut output = format!(
                            "Found {} callers of endpoint '{}':\n\n",
                            results.len(),
                            query
                        );
                        for (i, r) in results.iter().enumerate() {
                            let caller =
                                r.get("caller_name").and_then(|v| v.as_str()).unwrap_or("?");
                            let file = r.get("caller_file").and_then(|v| v.as_str()).unwrap_or("?");
                            let method = r.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                            let http_call =
                                r.get("http_call").and_then(|v| v.as_str()).unwrap_or("?");
                            output.push_str(&format!(
                                "{}. **{}** ({}) calls {} {}\n",
                                i + 1,
                                caller,
                                file,
                                method,
                                http_call,
                            ));
                        }
                        output
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }
            other => format!(
                "Error: unknown mode '{}'. Use 'calls' or 'endpoint_callers'.",
                other
            ),
        }
    }
}

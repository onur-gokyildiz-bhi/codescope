//! HTTP cross-service linking tools: find_http_calls, find_endpoint_callers.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = http_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Find HTTP client calls in the codebase
    #[tool(
        description = "Find all HTTP client calls (reqwest, fetch, axios, requests) in the codebase. Optionally filter by HTTP method (GET, POST, PUT, DELETE, PATCH). Shows which functions make HTTP requests and to which endpoints."
    )]
    async fn find_http_calls(&self, Parameters(params): Parameters<HttpCallParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_http_calls(params.method.as_deref()).await {
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
        }
    }

    /// Find which functions call a specific HTTP endpoint
    #[tool(
        description = "Find all code functions that call a specific HTTP endpoint by URL pattern. Example: '/users' finds all code that makes HTTP requests to any /users endpoint. Shows the calling function, HTTP method, and location."
    )]
    async fn find_endpoint_callers(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_endpoint_callers(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!(
                        "No functions found calling endpoint matching '{}'",
                        params.query
                    );
                }
                let mut output = format!(
                    "Found {} callers of endpoint '{}':\n\n",
                    results.len(),
                    params.query
                );
                for (i, r) in results.iter().enumerate() {
                    let caller = r.get("caller_name").and_then(|v| v.as_str()).unwrap_or("?");
                    let file = r.get("caller_file").and_then(|v| v.as_str()).unwrap_or("?");
                    let method = r.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                    let http_call = r.get("http_call").and_then(|v| v.as_str()).unwrap_or("?");
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
}

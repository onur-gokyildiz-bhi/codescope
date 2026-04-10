//! Search and basic graph query tools:
//! search_functions, find_function, file_entities, find_callers, find_callees,
//! graph_stats, raw_query, supported_languages.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = search_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Search for functions by name or pattern in the code graph
    #[tool(
        description = "Search for functions by name or pattern. Returns matching functions with file paths and line numbers."
    )]
    async fn search_functions(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = params.limit.unwrap_or(20);
        let gq = GraphQuery::new(ctx.db);

        match gq.search_functions(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No functions found matching '{}'", params.query);
                }
                let mut output = format!(
                    "Found {} functions matching '{}':\n\n",
                    results.len().min(limit),
                    params.query
                );
                for (i, r) in results.iter().enumerate().take(limit) {
                    output.push_str(&format!(
                        "{}. **{}** ({}:{})\n",
                        i + 1,
                        r.name.as_deref().unwrap_or("?"),
                        r.file_path.as_deref().unwrap_or("?"),
                        r.start_line.unwrap_or(0),
                    ));
                    if let Some(sig) = &r.signature {
                        output.push_str(&format!("   `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Error searching: {}", e),
        }
    }

    /// Find a function by exact name
    #[tool(
        description = "Find a function by exact name. Returns detailed info including signature, file path, and line numbers."
    )]
    async fn find_function(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_function(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No function found with name '{}'", params.query);
                }
                serde_json::to_string_pretty(&results)
                    .unwrap_or_else(|_| "Error formatting results".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// List all code entities (functions, classes) in a specific file
    #[tool(
        description = "List all functions and classes in a file. Provides an overview of the file's structure."
    )]
    async fn file_entities(&self, Parameters(params): Parameters<FileEntitiesParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.file_entities(&params.file_path).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No entities found in '{}'", params.file_path);
                }
                let mut output = format!("Entities in {}:\n\n", params.file_path);
                for r in &results {
                    output.push_str(&format!(
                        "- **{}** (L{}-{})\n",
                        r.name.as_deref().unwrap_or("?"),
                        r.start_line.unwrap_or(0),
                        r.end_line.unwrap_or(0),
                    ));
                    if let Some(sig) = &r.signature {
                        output.push_str(&format!("  `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find all functions that call the specified function (callers / incoming calls)
    #[tool(
        description = "Find all functions that call the specified function. Useful for understanding who depends on a function."
    )]
    async fn find_callers(&self, Parameters(params): Parameters<FindCallersParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_callers(&params.function_name).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No callers found for '{}'", params.function_name);
                }
                serde_json::to_string_pretty(&results)
                    .unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find all functions called by the specified function (callees / outgoing calls)
    #[tool(
        description = "Find all functions called by the specified function. Useful for understanding a function's dependencies."
    )]
    async fn find_callees(&self, Parameters(params): Parameters<FindCalleesParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_callees(&params.function_name).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No callees found for '{}'", params.function_name);
                }
                serde_json::to_string_pretty(&results)
                    .unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Get statistics about the indexed code graph
    #[tool(
        description = "Get statistics about the code graph: number of files, functions, classes, and relationships indexed."
    )]
    async fn graph_stats(&self) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.stats().await {
            Ok(stats) => serde_json::to_string_pretty(&stats)
                .unwrap_or_else(|_| "Error formatting stats".into()),
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Execute a raw SurrealQL query against the code graph
    #[tool(
        description = "Execute a raw SurrealQL query against the code graph database. Use for advanced queries like graph traversals."
    )]
    async fn raw_query(&self, Parameters(params): Parameters<RawQueryParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.raw_query(&params.query).await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Query error: {}", e),
        }
    }

    #[tool(description = "List all programming languages supported by the code graph parser.")]
    async fn supported_languages(&self) -> String {
        let parser = codescope_core::parser::CodeParser::new();
        let languages = parser.supported_languages();
        format!("Supported languages: {}", languages.join(", "))
    }
}

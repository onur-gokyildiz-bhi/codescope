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
        description = "Fuzzy/substring search for functions by name. Case-insensitive. \
        Returns matching functions with file paths and line numbers. \
        Use this when you know roughly what the function is called but not the exact name. \
        If you know the exact name, prefer `find_function` instead (cheaper, single result)."
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
    #[tool(description = "Lookup a function by its EXACT name (case-sensitive). \
        Returns full info including signature, file path, line numbers, and qualified name. \
        Use this when you know the precise function name. \
        For fuzzy/partial matches use `search_functions`. \
        For full neighborhood (callers + callees + siblings) use `explore`.")]
    async fn find_function(&self, Parameters(params): Parameters<FindFunctionParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_function(&params.name).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No function found with name '{}'", params.name);
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
        description = "Find DIRECT (1-hop) callers of a function — who calls it from one level up. \
        Use this for the immediate question 'who uses this function?'. \
        For TRANSITIVE callers across multiple hops (full blast radius of a change) \
        use `impact_analysis` instead — same data, BFS to configurable depth."
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
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find all functions called by the specified function (callees / outgoing calls)
    #[tool(
        description = "Find DIRECT (1-hop) callees of a function — what it calls one level down. \
        Use this to understand a function's immediate dependencies. \
        Mirror of `find_callers`. For full neighborhood (both directions plus context) use `explore`."
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
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "Error formatting".into())
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
        description = "ESCAPE HATCH: execute a raw SurrealQL query against the code graph database. \
        Prefer the dedicated tools first (search_functions, find_function, find_callers, impact_analysis, \
        explore, type_hierarchy, etc.) — they are cheaper and harder to get wrong. \
        Use raw_query only when no dedicated tool fits, e.g., custom aggregations or multi-edge joins. \
        SurrealQL note: `function` is a reserved word, always backtick it (`\\`function\\``). \
        For multi-hop traversal use the native syntax `<-calls<-\\`function\\`<-calls<-\\`function\\`.name`, \
        NOT nested subqueries (slow scan instead of indexed graph walk)."
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

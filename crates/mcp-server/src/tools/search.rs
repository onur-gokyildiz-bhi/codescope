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
        description = "Fuzzy search functions by name. Returns matches with file paths and line numbers."
    )]
    async fn search_functions(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = params.limit.unwrap_or(20);
        let gq = GraphQuery::new(ctx.db.clone());

        match gq.search_functions(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No functions found matching '{}'", params.query);
                }

                // Graph-ranked reranking: boost by caller count (simplified PPR)
                let names: Vec<String> = results
                    .iter()
                    .take(limit)
                    .filter_map(|r| r.name.clone())
                    .collect();

                let mut caller_counts: std::collections::HashMap<String, u64> =
                    std::collections::HashMap::new();

                if !names.is_empty() {
                    let placeholders: Vec<String> = names
                        .iter()
                        .enumerate()
                        .map(|(i, _)| format!("$n{}", i))
                        .collect();
                    let in_list = placeholders.join(", ");
                    let rank_q = format!(
                        "SELECT out.name AS name, count() AS cnt \
                         FROM calls WHERE out.name IN [{}] AND in.name != NONE \
                         GROUP BY name",
                        in_list
                    );
                    let mut q = ctx.db.query(&rank_q);
                    for (i, name) in names.iter().enumerate() {
                        q = q.bind((format!("n{}", i), name.clone()));
                    }
                    if let Ok(mut r) = q.await {
                        let rows: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        for row in &rows {
                            let n = row.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            let c = row.get("cnt").and_then(|v| v.as_u64()).unwrap_or(0);
                            caller_counts.insert(n.to_string(), c);
                        }
                    }
                }

                // Sort by caller count (descending), then by original order
                let mut ranked: Vec<(usize, &codescope_core::graph::query::SearchResult)> =
                    results.iter().enumerate().take(limit).collect();
                ranked.sort_by(|a, b| {
                    let ca = a.1.name.as_deref().and_then(|n| caller_counts.get(n)).unwrap_or(&0);
                    let cb = b.1.name.as_deref().and_then(|n| caller_counts.get(n)).unwrap_or(&0);
                    cb.cmp(ca)
                });

                let mut output = format!(
                    "Found {} functions matching '{}' (ranked by graph importance):\n\n",
                    results.len().min(limit),
                    params.query
                );
                for (i, (_, r)) in ranked.iter().enumerate() {
                    let name = r.name.as_deref().unwrap_or("?");
                    let callers = caller_counts.get(name).unwrap_or(&0);
                    output.push_str(&format!(
                        "{}. **{}** ({}:{}) [{} callers]\n",
                        i + 1,
                        name,
                        r.file_path.as_deref().unwrap_or("?"),
                        r.start_line.unwrap_or(0),
                        callers,
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
    #[tool(description = "Exact function lookup by name. Returns signature, file path, line numbers.")]
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
        description = "List all functions and classes in a file."
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
        description = "Find direct callers of a function (1-hop). For transitive use impact_analysis."
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
        description = "Find direct callees of a function (1-hop). For full neighborhood use explore."
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
        description = "Code graph statistics: files, functions, classes, relationships count."
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
        description = "Raw SurrealQL query. Backtick `function`. Prefer dedicated tools first."
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

    #[tool(description = "List supported programming languages.")]
    async fn supported_languages(&self) -> String {
        let parser = codescope_core::parser::CodeParser::new();
        let languages = parser.supported_languages();
        format!("Supported languages: {}", languages.join(", "))
    }
}

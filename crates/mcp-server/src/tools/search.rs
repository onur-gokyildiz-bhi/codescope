//! Search and basic graph query tools:
//! search (unified: fuzzy|exact|file|cross_type|neighborhood|backlinks),
//! find_callers, find_callees, graph_stats, raw_query, supported_languages,
//! retrieve_archived.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = search_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Unified search: dispatches to fuzzy, exact, file, cross_type, neighborhood, or backlinks.
    #[tool(
        description = "Unified search: mode=fuzzy|exact|file|cross_type|neighborhood|backlinks. fuzzy: search by name substring. exact: find function by exact name. file: list entities in file. cross_type: search all entity types. neighborhood: callers+callees+siblings. backlinks: reverse references."
    )]
    #[tracing::instrument(skip_all, fields(mode = %params.mode))]
    async fn search(&self, Parameters(params): Parameters<SearchUnifiedParams>) -> String {
        match params.mode.as_str() {
            "fuzzy" => search_fuzzy(self, &params).await,
            "exact" => search_exact(self, &params).await,
            "file" => search_file(self, &params).await,
            "cross_type" => search_cross_type(self, &params).await,
            "neighborhood" => search_neighborhood(self, &params).await,
            "backlinks" => search_backlinks(self, &params).await,
            other => format!(
                "Unknown search mode '{}'. Use 'fuzzy', 'exact', 'file', 'cross_type', 'neighborhood', or 'backlinks'.",
                other
            ),
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
        description = "Find direct callees of a function (1-hop). For full neighborhood use search mode=neighborhood."
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
    #[tool(description = "Code graph statistics: files, functions, classes, relationships count.")]
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
    #[tool(description = "Raw SurrealQL query. Backtick `function`. Prefer dedicated tools first.")]
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

    /// Retrieve full output of an archived large result by ID
    #[tool(
        description = "Retrieve full output of an archived large result. Use when a tool returned a summary with a retrieval ID."
    )]
    async fn retrieve_archived(
        &self,
        Parameters(params): Parameters<RetrieveArchivedParams>,
    ) -> String {
        let archive = self.result_archive().read().await;
        match archive.get(&params.id) {
            Some(content) => content.clone(),
            None => format!("No archived result found with ID '{}'", params.id),
        }
    }
}

// === Mode helpers (not registered as tools) ===

async fn search_fuzzy(server: &GraphRagServer, params: &SearchUnifiedParams) -> String {
    let ctx = match server.ctx().await {
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
                let ca =
                    a.1.name
                        .as_deref()
                        .and_then(|n| caller_counts.get(n))
                        .unwrap_or(&0);
                let cb =
                    b.1.name
                        .as_deref()
                        .and_then(|n| caller_counts.get(n))
                        .unwrap_or(&0);
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

async fn search_exact(server: &GraphRagServer, params: &SearchUnifiedParams) -> String {
    let ctx = match server.ctx().await {
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

async fn search_file(server: &GraphRagServer, params: &SearchUnifiedParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let gq = GraphQuery::new(ctx.db);

    match gq.file_entities(&params.query).await {
        Ok(results) => {
            if results.is_empty() {
                return format!("No entities found in '{}'", params.query);
            }
            let mut output = format!("Entities in {}:\n\n", params.query);
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

async fn search_cross_type(server: &GraphRagServer, params: &SearchUnifiedParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let gq = GraphQuery::new(ctx.db);
    let limit = params.limit.unwrap_or(10);

    match gq.cross_search(&params.query, limit).await {
        Ok(result) => {
            let total = result
                .get("total_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let mut output = format!("## Related: '{}' ({} results)\n\n", params.query, total);

            for (key, icon, label) in [
                ("functions", "fn", "Functions"),
                ("classes", "cls", "Classes"),
                ("configs", "cfg", "Config Keys"),
                ("docs", "doc", "Documentation"),
                ("packages", "pkg", "Packages"),
                ("files", "file", "Files"),
                ("imports", "imp", "Imports"),
                ("infra", "inf", "Infrastructure"),
            ] {
                if let Some(items) = result.get(key).and_then(|v| v.as_array()) {
                    output.push_str(&format!("### {} [{}] ({})\n", label, icon, items.len()));
                    for item in items {
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let fp = item.get("file_path").and_then(|v| v.as_str());
                        if let Some(fp) = fp {
                            output.push_str(&format!("- {} ({})\n", name, fp));
                        } else {
                            output.push_str(&format!("- {}\n", name));
                        }
                    }
                    output.push('\n');
                }
            }

            output
        }
        Err(e) => format!("Error searching for '{}': {}", params.query, e),
    }
}

async fn search_neighborhood(server: &GraphRagServer, params: &SearchUnifiedParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let gq = GraphQuery::new(ctx.db);

    match gq.explore(&params.query).await {
        Ok(result) => {
            let mut output = format!("## Explore: {}\n\n", params.query);

            if let Some(entity_type) = result.get("entity_type").and_then(|v| v.as_str()) {
                output.push_str(&format!("**Type:** {}\n\n", entity_type));
            }

            if let Some(matches) = result.get("matches").and_then(|v| v.as_array()) {
                output.push_str("### Entity\n");
                for m in matches {
                    if let Some(fp) = m.get("file_path").and_then(|v| v.as_str()) {
                        let line = m.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                        output.push_str(&format!("- **{}** ({}:{})\n", params.query, fp, line));
                    }
                    if let Some(sig) = m.get("signature").and_then(|v| v.as_str()) {
                        output.push_str(&format!("  `{}`\n", sig));
                    }
                }
                output.push('\n');
            }

            if let Some(callers) = result.get("called_by").and_then(|v| v.as_array()) {
                if !callers.is_empty() {
                    output.push_str(&format!("### Called By ({} functions)\n", callers.len()));
                    for c in callers {
                        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        output.push_str(&format!("- {} ({})\n", name, fp));
                    }
                    output.push('\n');
                }
            }

            if let Some(callees) = result.get("calls_to").and_then(|v| v.as_array()) {
                if !callees.is_empty() {
                    output.push_str(&format!("### Calls ({} functions)\n", callees.len()));
                    for c in callees {
                        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        output.push_str(&format!("- {} ({})\n", name, fp));
                    }
                    output.push('\n');
                }
            }

            if let Some(siblings) = result.get("sibling_functions").and_then(|v| v.as_array()) {
                if !siblings.is_empty() {
                    output.push_str(&format!("### Same File ({} siblings)\n", siblings.len()));
                    for s in siblings {
                        let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let sig = s.get("signature").and_then(|v| v.as_str());
                        if let Some(sig) = sig {
                            output.push_str(&format!("- {} `{}`\n", name, sig));
                        } else {
                            output.push_str(&format!("- {}\n", name));
                        }
                    }
                    output.push('\n');
                }
            }

            if let Some(funcs) = result.get("functions").and_then(|v| v.as_array()) {
                output.push_str(&format!("### Functions ({})\n", funcs.len()));
                for f in funcs {
                    let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let sig = f.get("signature").and_then(|v| v.as_str());
                    let line = f.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                    if let Some(sig) = sig {
                        output.push_str(&format!("- L{}: {} `{}`\n", line, name, sig));
                    } else {
                        output.push_str(&format!("- L{}: {}\n", line, name));
                    }
                }
                output.push('\n');
            }

            crate::helpers::maybe_archive(server.result_archive(), "explore", output).await
        }
        Err(e) => format!("Error exploring '{}': {}", params.query, e),
    }
}

async fn search_backlinks(server: &GraphRagServer, params: &SearchUnifiedParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let gq = GraphQuery::new(ctx.db);

    match gq.backlinks(&params.query).await {
        Ok(result) => {
            let total = result
                .get("total_backlinks")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let mut output = format!("## Backlinks: {} ({} links)\n\n", params.query, total);

            if let Some(callers) = result.get("callers").and_then(|v| v.as_array()) {
                output.push_str(&format!("### Callers ({})\n", callers.len()));
                for c in callers {
                    let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    let sig = c.get("signature").and_then(|v| v.as_str());
                    if let Some(sig) = sig {
                        output.push_str(&format!("- **{}** ({}) `{}`\n", name, fp, sig));
                    } else {
                        output.push_str(&format!("- **{}** ({})\n", name, fp));
                    }
                }
                output.push('\n');
            }

            if let Some(importers) = result.get("importers").and_then(|v| v.as_array()) {
                output.push_str(&format!("### Imported By ({})\n", importers.len()));
                for i in importers {
                    let name = i.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let fp = i.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    output.push_str(&format!("- {} ({})\n", name, fp));
                }
                output.push('\n');
            }

            if let Some(containers) = result.get("contained_in").and_then(|v| v.as_array()) {
                output.push_str(&format!("### Defined In ({})\n", containers.len()));
                for c in containers {
                    let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    output.push_str(&format!("- {}\n", name));
                }
                output.push('\n');
            }

            if let Some(deps) = result.get("dependents").and_then(|v| v.as_array()) {
                output.push_str(&format!("### Dependents ({})\n", deps.len()));
                for d in deps {
                    let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let fp = d.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    output.push_str(&format!("- {} ({})\n", name, fp));
                }
                output.push('\n');
            }

            if total == 0 {
                output.push_str(
                    "No backlinks found. The entity may not exist or has no incoming references.\n",
                );
            }

            output
        }
        Err(e) => format!("Error finding backlinks for '{}': {}", params.query, e),
    }
}

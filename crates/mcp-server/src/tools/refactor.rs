//! Symbol-level refactoring tools: rename_symbol, find_unused, safe_delete.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = refactor_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Find all references to a symbol for rename planning
    #[tool(description = "Find all references to a symbol: definitions, call sites, imports.")]
    async fn rename_symbol(&self, Parameters(params): Parameters<RenameSymbolParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_all_references(&params.symbol_name).await {
            Ok(result) => {
                let total = result
                    .get("total_references")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if total == 0 {
                    return format!("No references found for symbol '{}'", params.symbol_name);
                }
                let mut output = format!(
                    "**Symbol: {}** — {} references found\n\n",
                    params.symbol_name, total
                );

                if let Some(refs) = result.get("references").and_then(|v| v.as_array()) {
                    for r in refs {
                        let ref_type = r.get("ref_type").and_then(|v| v.as_str()).unwrap_or("?");
                        let file = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let line = r.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                        let name = r
                            .get("name")
                            .or_else(|| r.get("caller_name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        output.push_str(&format!(
                            "- [{}] **{}** at {}:{}\n",
                            ref_type, name, file, line
                        ));
                    }
                }

                output.push_str(&format!(
                    "\nTo rename '{}', all {} locations above need updating.",
                    params.symbol_name, total
                ));
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find unused symbols (functions with zero references)
    #[tool(description = "Find unused functions with zero callers. Filters entry points.")]
    async fn find_unused(&self, Parameters(params): Parameters<DeadCodeParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);
        let min_lines = params.min_lines.unwrap_or(3);

        match gq.find_unused_symbols(min_lines).await {
            Ok(results) => {
                if results.is_empty() {
                    return "No unused symbols found (or all are entry points/trivial).".into();
                }
                let limit = params.limit.unwrap_or(50);
                let mut output = format!(
                    "Found {} potentially unused symbols (min {} lines):\n\n",
                    results.len().min(limit),
                    min_lines
                );
                for (i, r) in results.iter().enumerate().take(limit) {
                    let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let file = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    let lines = r.get("line_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let sig = r.get("signature").and_then(|v| v.as_str()).unwrap_or("");
                    output.push_str(&format!(
                        "{}. **{}** ({}, {} lines)\n",
                        i + 1,
                        name,
                        file,
                        lines
                    ));
                    if !sig.is_empty() {
                        output.push_str(&format!("   `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Check if a symbol can be safely deleted
    #[tool(description = "Check if a symbol can be safely deleted (zero callers/importers).")]
    async fn safe_delete(&self, Parameters(params): Parameters<SafeDeleteParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.safe_delete_check(&params.symbol_name).await {
            Ok(result) => {
                let safe = result
                    .get("safe_to_delete")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let callers = result
                    .get("caller_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let imports = result
                    .get("import_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                if safe {
                    let mut output = format!(
                        "**{}** can be safely deleted. No callers or imports reference it.\n\n",
                        params.symbol_name
                    );
                    if let Some(defs) = result.get("definitions").and_then(|v| v.as_array()) {
                        output.push_str("Definitions to remove:\n");
                        for d in defs {
                            let file = d.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                            let line = d.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                            output.push_str(&format!("- {}:{}\n", file, line));
                        }
                    }
                    output
                } else {
                    format!(
                        "**{}** is NOT safe to delete.\n\n\
                         - {} callers still reference it\n\
                         - {} imports mention it\n\n\
                         Use `rename_symbol` to see all references.",
                        params.symbol_name, callers, imports
                    )
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    }
}

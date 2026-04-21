//! Symbol-level refactoring: consolidated `refactor` tool with action dispatch.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = refactor_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Consolidated refactor tool — rename planning, dead-code discovery, safe-delete check.
    #[tool(description = "Refactor ops (rename references, find unused, safe-delete check).")]
    async fn refactor(&self, Parameters(params): Parameters<RefactorParams>) -> String {
        let ctx = match self.gated_ctx_named("refactor").await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let repo_name = ctx.repo_name.clone();
        let gq = GraphQuery::new(ctx.db);

        match params.action.as_str() {
            "rename" => match gq.find_all_references(&params.name).await {
                Ok(result) => {
                    let total = result
                        .get("total_references")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if total == 0 {
                        return format!("No references found for symbol '{}'", params.name);
                    }
                    let mut output = format!(
                        "**Symbol: {}** — {} references found\n\n",
                        params.name, total
                    );

                    if let Some(refs) = result.get("references").and_then(|v| v.as_array()) {
                        for r in refs {
                            let ref_type =
                                r.get("ref_type").and_then(|v| v.as_str()).unwrap_or("?");
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
                        params.name, total
                    ));
                    output
                }
                Err(e) => format!("Error: {}", e),
            },
            "find_unused" => {
                // `name` is required by the schema but unused for this action; min_lines
                // defaults to 3 (matches prior DeadCodeParams behavior).
                let min_lines = 3u32;
                match gq.find_unused_symbols(min_lines, &repo_name).await {
                    Ok(results) => {
                        if results.is_empty() {
                            return "No unused symbols found (or all are entry points/trivial)."
                                .into();
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
            "safe_delete" => match gq.safe_delete_check(&params.name).await {
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
                            params.name
                        );
                        if let Some(defs) = result.get("definitions").and_then(|v| v.as_array()) {
                            output.push_str("Definitions to remove:\n");
                            for d in defs {
                                let file =
                                    d.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                                let line =
                                    d.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                                output.push_str(&format!("- {}:{}\n", file, line));
                            }
                        }
                        output
                    } else {
                        format!(
                            "**{}** is NOT safe to delete.\n\n\
                             - {} callers still reference it\n\
                             - {} imports mention it\n\n\
                             Use `refactor` with action=\"rename\" to see all references.",
                            params.name, callers, imports
                        )
                    }
                }
                Err(e) => format!("Error: {}", e),
            },
            other => format!(
                "Unknown action '{}'. Valid: rename | find_unused | safe_delete",
                other
            ),
        }
    }
}

//! Call graph analysis tools: impact_analysis, type_hierarchy.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::helpers::maybe_archive;
use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = callgraph_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Analyze the impact of changing a function — what else could be affected
    #[tool(
        description = "Transitive blast radius via BFS (depth 1-5). Shows callers, importers, implementors."
    )]
    #[tracing::instrument(skip_all, fields(function_name = %params.function_name, depth = params.depth.unwrap_or(3)))]
    async fn impact_analysis(
        &self,
        Parameters(params): Parameters<ImpactAnalysisParams>,
    ) -> String {
        let ctx = match self.gated_ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let depth = params.depth.unwrap_or(3).min(5);

        let mut output = format!("## Impact Analysis: {}\n\n", params.function_name);

        // Step 1: Find the function
        let func_query =
            "SELECT name, qualified_name, file_path, start_line FROM `function` WHERE name = $name";
        let func_info: Vec<serde_json::Value> = match ctx
            .db
            .query(func_query)
            .bind(("name", params.function_name.clone()))
            .await
        {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(e) => return format!("Error: {}", e),
        };

        if let Some(info) = func_info.first() {
            output.push_str(&format!(
                "**Location:** {}:{}\n\n",
                info.get("file_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
                info.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0),
            ));
        }

        // Step 2: Iterative BFS for callers up to `depth` hops.
        //
        // Each hop uses SurrealDB's native inverse graph traversal
        // (`<-calls<-\`function\``) which walks indexed edges in a single
        // statement. This is 50-200x faster than the previous
        // `FROM calls WHERE out.name IN [...]` approach on real repos —
        // the old path was a full scan of the calls table (~60 ms / hop on
        // ripgrep, ~120 ms / hop on tokio); the native traversal runs in
        // under a millisecond regardless of corpus size.
        //
        // Max 100 unique callers per hop after dedup, as a safety cap
        // against pathological fan-out.
        const MAX_CALLERS_PER_HOP: usize = 100;

        let mut current_names = vec![params.function_name.clone()];
        let mut all_seen: std::collections::HashSet<String> =
            std::collections::HashSet::from([params.function_name.clone()]);

        for hop in 0..depth {
            if current_names.is_empty() {
                break;
            }

            let placeholders: Vec<String> = current_names
                .iter()
                .enumerate()
                .map(|(i, _)| format!("$n{}", i))
                .collect();
            let in_list = placeholders.join(", ");
            let query = format!(
                "SELECT <-calls<-`function` AS callers \
                 FROM `function` WHERE name IN [{}]",
                in_list
            );

            let mut q = ctx.db.query(&query);
            for (i, name) in current_names.iter().enumerate() {
                q = q.bind((format!("n{}", i), name.clone()));
            }

            let target_rows: Vec<serde_json::Value> = match q.await {
                Ok(mut r) => r.take(0).unwrap_or_default(),
                Err(e) => {
                    output.push_str(&format!("\nError at hop {}: {}\n", hop + 1, e));
                    break;
                }
            };

            // Flatten: each target row holds a `callers` array of function
            // records. Dedup by name across all targets in this hop.
            let mut new_names = Vec::new();
            let mut hop_callers = Vec::new();
            'outer: for row in &target_rows {
                let Some(callers) = row.get("callers").and_then(|v| v.as_array()) else {
                    continue;
                };
                for c in callers {
                    let Some(name) = c.get("name").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if all_seen.insert(name.to_string()) {
                        new_names.push(name.to_string());
                        hop_callers.push(c.clone());
                        if hop_callers.len() >= MAX_CALLERS_PER_HOP {
                            break 'outer;
                        }
                    }
                }
            }

            let label = if hop == 0 {
                "Direct Callers".to_string()
            } else {
                format!("Indirect Callers ({} hops)", hop + 1)
            };

            output.push_str(&format!("### {}\n", label));
            if hop_callers.is_empty() {
                output.push_str("None found\n\n");
            } else {
                for c in &hop_callers {
                    let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let file = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    output.push_str(&format!("- `{}` ({})\n", name, file));
                }
                output.push('\n');
            }

            current_names = new_names;
        }

        // Extended impact: imports edge (who imports the file containing this function)
        if let Some(info) = func_info.first() {
            let fp = info.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
            if !fp.is_empty() {
                let import_q = "SELECT <-imports<-file AS importers FROM file WHERE path = $fp";
                if let Ok(mut r) = ctx.db.query(import_q).bind(("fp", fp.to_string())).await {
                    let rows: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                    let mut importers = Vec::new();
                    for row in &rows {
                        if let Some(arr) = row.get("importers").and_then(|v| v.as_array()) {
                            for imp in arr {
                                let p = imp.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                                if p != fp && !importers.contains(&p.to_string()) {
                                    importers.push(p.to_string());
                                }
                            }
                        }
                    }
                    if !importers.is_empty() {
                        output.push_str(&format!(
                            "### Files Importing This Module ({})\n",
                            importers.len()
                        ));
                        for p in &importers {
                            output.push_str(&format!("- `{}`\n", p));
                        }
                        output.push('\n');
                    }
                }

                // Type-level impact: if any class in this file is implemented/inherited
                let type_q = "SELECT name, kind FROM class WHERE file_path = $fp";
                if let Ok(mut r) = ctx.db.query(type_q).bind(("fp", fp.to_string())).await {
                    let classes: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                    for cls in &classes {
                        let cls_name = cls.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        // GROUP BY dedups duplicate `implements` edges on
                        // legacy DBs (see find_callers fix rationale).
                        let impl_q = "SELECT in.name AS name, in.file_path AS file_path FROM implements WHERE out.name = $name GROUP BY name, file_path";
                        if let Ok(mut ir) = ctx
                            .db
                            .query(impl_q)
                            .bind(("name", cls_name.to_string()))
                            .await
                        {
                            let impls: Vec<serde_json::Value> = ir.take(0).unwrap_or_default();
                            if !impls.is_empty() {
                                output.push_str(&format!(
                                    "### Types Implementing `{}` ({})\n",
                                    cls_name,
                                    impls.len()
                                ));
                                for im in &impls {
                                    let n = im.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                    let f =
                                        im.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                                    output.push_str(&format!("- `{}` ({})\n", n, f));
                                }
                                output.push('\n');
                            }
                        }
                    }
                }
            }
        }

        maybe_archive(self.result_archive(), "impact_analysis", output).await
    }

    /// Show inheritance chain for a class/struct/trait/interface
    #[tool(description = "Type inheritance graph: parents, subtypes, interfaces, implementors.")]
    async fn type_hierarchy(&self, Parameters(params): Parameters<TypeHierarchyParams>) -> String {
        let ctx = match self.gated_ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);
        let depth = params.depth.unwrap_or(3);

        match gq.type_hierarchy(&params.name, depth).await {
            Ok(result) => {
                let mut output = format!("## Type Hierarchy: {}\n\n", params.name);

                if let Some(entity) = result.get("entity") {
                    let kind = entity.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                    let file = entity
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    output.push_str(&format!("**{}** [{}] in {}\n\n", params.name, kind, file));
                }

                for (key, title) in [
                    ("parents", "Extends"),
                    ("children", "Subtypes"),
                    ("implements", "Implements"),
                    ("implemented_by", "Implemented By"),
                ] {
                    if let Some(items) = result.get(key).and_then(|v| v.as_array()) {
                        if !items.is_empty() {
                            output.push_str(&format!("### {}\n", title));
                            for item in items {
                                let name = item
                                    .get("parent")
                                    .or(item.get("child"))
                                    .or(item.get("iface"))
                                    .or(item.get("implementor"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                output.push_str(&format!("- {}\n", name));
                            }
                            output.push('\n');
                        }
                    }
                }

                if output.lines().count() <= 3 {
                    format!(
                        "No type '{}' found or no inheritance relationships.",
                        params.name
                    )
                } else {
                    output
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    }
}

//! Obsidian-like context exploration tool: context_bundle.
//!
//! Other exploration modes (explore, related, backlinks) moved into the
//! unified `search` tool in `search.rs` (modes: neighborhood, cross_type, backlinks).

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::helpers::derive_scope_from_file_path;
use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = exploration_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Get full context for a file — like opening an Obsidian note with all linked content
    #[tool(
        description = "File context: functions, callers, classes, imports, decisions. Use before Read."
    )]
    #[tracing::instrument(skip_all, fields(file_path = %params.file_path))]
    async fn context_bundle(&self, Parameters(params): Parameters<ContextBundleParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let db = ctx.db.clone();
        let gq = GraphQuery::new(ctx.db);
        let cache = self.context_cache().clone();

        match gq.file_context(&params.file_path).await {
            Ok(result) => {
                let mut output = format!("## Context: {}\n\n", params.file_path);

                if let Some(file) = result.get("file") {
                    let lang = file.get("language").and_then(|v| v.as_str()).unwrap_or("?");
                    let lines = file.get("line_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    output.push_str(&format!(
                        "**Language:** {} | **Lines:** {}\n\n",
                        lang, lines
                    ));
                }

                if let Some(funcs) = result.get("functions").and_then(|v| v.as_array()) {
                    output.push_str(&format!("### Functions ({})\n", funcs.len()));
                    for f in funcs {
                        let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let sig = f.get("signature").and_then(|v| v.as_str());
                        let s = f.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                        let e = f.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0);
                        if let Some(sig) = sig {
                            output.push_str(&format!("- **{}** (L{}-{}) `{}`\n", name, s, e, sig));
                        } else {
                            output.push_str(&format!("- **{}** (L{}-{})\n", name, s, e));
                        }
                        if let Some(ext) = f.get("external_callers").and_then(|v| v.as_array()) {
                            for caller in ext {
                                let cn = caller.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let cf = caller
                                    .get("file_path")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                output.push_str(&format!("    ← called by **{}** ({})\n", cn, cf));
                            }
                        }
                    }
                    output.push('\n');
                }

                if let Some(classes) = result.get("classes").and_then(|v| v.as_array()) {
                    if !classes.is_empty() {
                        output.push_str(&format!("### Classes ({})\n", classes.len()));
                        for c in classes {
                            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let kind = c.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                            output.push_str(&format!("- {} {}\n", kind, name));
                        }
                        output.push('\n');
                    }
                }

                if let Some(imports) = result.get("imports").and_then(|v| v.as_array()) {
                    if !imports.is_empty() {
                        output.push_str(&format!("### Imports ({})\n", imports.len()));
                        for i in imports {
                            let name = i.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            output.push_str(&format!("- {}\n", name));
                        }
                        output.push('\n');
                    }
                }

                for (key, label) in [
                    ("configs", "Config"),
                    ("docs", "Documentation"),
                    ("packages", "Packages"),
                    ("infra", "Infrastructure"),
                ] {
                    if let Some(items) = result.get(key).and_then(|v| v.as_array()) {
                        output.push_str(&format!("### {} ({})\n", label, items.len()));
                        for item in items {
                            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                            output.push_str(&format!("- [{}] {}\n", kind, name));
                        }
                        output.push('\n');
                    }
                }

                let cross_query = format!(
                    "SELECT in.name AS caller, in.file_path AS caller_file \
                     FROM calls WHERE out.file_path = '{}' AND in.file_path != '{}' AND in.name != NONE LIMIT 20; \
                     SELECT out.name AS callee, out.file_path AS callee_file \
                     FROM calls WHERE in.file_path = '{}' AND out.file_path != '{}' AND out.name != NONE LIMIT 20;",
                    params.file_path.replace('\'', "\\'"),
                    params.file_path.replace('\'', "\\'"),
                    params.file_path.replace('\'', "\\'"),
                    params.file_path.replace('\'', "\\'"),
                );
                if let Ok(mut cross_resp) = db.query(&cross_query).await {
                    let incoming: Vec<serde_json::Value> = cross_resp.take(0).unwrap_or_default();
                    let outgoing: Vec<serde_json::Value> = cross_resp.take(1).unwrap_or_default();

                    if !incoming.is_empty() {
                        output.push_str(&format!(
                            "### Incoming Cross-File Calls ({})\n",
                            incoming.len()
                        ));
                        for c in &incoming {
                            let caller = c.get("caller").and_then(|v| v.as_str()).unwrap_or("?");
                            let file = c.get("caller_file").and_then(|v| v.as_str()).unwrap_or("?");
                            output.push_str(&format!("- **{}** from {}\n", caller, file));
                        }
                        output.push('\n');
                    }

                    if !outgoing.is_empty() {
                        output.push_str(&format!(
                            "### Outgoing Cross-File Calls ({})\n",
                            outgoing.len()
                        ));
                        for c in &outgoing {
                            let callee = c.get("callee").and_then(|v| v.as_str()).unwrap_or("?");
                            let file = c.get("callee_file").and_then(|v| v.as_str()).unwrap_or("?");
                            output.push_str(&format!("- **{}** in {}\n", callee, file));
                        }
                        output.push('\n');
                    }
                }

                let file_scope = derive_scope_from_file_path(&params.file_path);
                let file_decisions: Vec<serde_json::Value> = db
                    .query(
                        "SELECT name, body, timestamp, tier FROM decision \
                         WHERE repo = $repo AND scope ~ $scope \
                         ORDER BY tier ASC, timestamp DESC LIMIT 5",
                    )
                    .bind(("repo", ctx.repo_name.clone()))
                    .bind(("scope", file_scope.clone()))
                    .await
                    .ok()
                    .and_then(|mut r| r.take(0).ok())
                    .unwrap_or_default();

                if !file_decisions.is_empty() {
                    output.push_str("\n### Past Decisions About This File\n");
                    for d in &file_decisions {
                        let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let tier = d.get("tier").and_then(|v| v.as_u64()).unwrap_or(2);
                        let prefix = if tier == 0 { "[PINNED] " } else { "" };
                        output.push_str(&format!("- {}{}\n", prefix, name));
                    }
                }

                let file_problems: Vec<serde_json::Value> = db
                    .query(
                        "SELECT name, timestamp FROM problem \
                         WHERE repo = $repo AND scope ~ $scope \
                         ORDER BY timestamp DESC LIMIT 5",
                    )
                    .bind(("repo", ctx.repo_name.clone()))
                    .bind(("scope", file_scope))
                    .await
                    .ok()
                    .and_then(|mut r| r.take(0).ok())
                    .unwrap_or_default();

                if !file_problems.is_empty() {
                    output.push_str("\n### Known Issues With This File\n");
                    for p in &file_problems {
                        let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        output.push_str(&format!("- {}\n", name));
                    }
                }

                // Delta mode: compare with cached output
                let file_key = params.file_path.clone();
                let mut cache_guard = cache.write().await;
                if let Some(prev) = cache_guard.get(&file_key) {
                    if *prev == output {
                        tracing::info!(
                            target: "codescope.cache",
                            outcome = "unchanged",
                            file = %file_key,
                            "delta_mode_hit"
                        );
                        drop(cache_guard);
                        return format!(
                            "## Context: {} (UNCHANGED)\n\nNo structural changes since last check. \
                             {} functions, same callers, same imports.",
                            params.file_path,
                            result.get("functions").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
                        );
                    }
                    // Compute diff: find added/removed lines
                    let prev_lines: std::collections::HashSet<&str> = prev.lines().collect();
                    let curr_lines: std::collections::HashSet<&str> = output.lines().collect();
                    let added: Vec<&&str> = curr_lines.difference(&prev_lines).collect();
                    let removed: Vec<&&str> = prev_lines.difference(&curr_lines).collect();

                    let mut delta = format!("## Context: {} (DELTA)\n\n", params.file_path);
                    if !added.is_empty() {
                        delta.push_str(&format!("### Added ({} lines)\n", added.len()));
                        for line in &added {
                            delta.push_str(&format!("+ {}\n", line));
                        }
                        delta.push('\n');
                    }
                    if !removed.is_empty() {
                        delta.push_str(&format!("### Removed ({} lines)\n", removed.len()));
                        for line in &removed {
                            delta.push_str(&format!("- {}\n", line));
                        }
                        delta.push('\n');
                    }
                    tracing::info!(
                        target: "codescope.cache",
                        outcome = "changed",
                        added = added.len(),
                        removed = removed.len(),
                        "delta_mode_miss"
                    );
                    cache_guard.insert(file_key, output);
                    delta
                } else {
                    tracing::info!(
                        target: "codescope.cache",
                        outcome = "cold",
                        "delta_mode_cold"
                    );
                    cache_guard.insert(file_key, output.clone());
                    output
                }
            }
            Err(e) => format!("Error getting context for '{}': {}", params.file_path, e),
        }
    }
}

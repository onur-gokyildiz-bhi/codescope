//! Obsidian-like context exploration tools: explore, context_bundle, related, backlinks.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::helpers::derive_scope_from_file_path;
use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = exploration_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Explore an entity's full graph neighborhood — like Obsidian's local graph view
    #[tool(
        description = "Explore the full neighborhood of any entity (function, class, config, doc, package, file). \
        Shows all connections: callers, callees, sibling functions, containing file, related configs/docs. \
        Use this to deeply understand how any piece of code or config fits into the system."
    )]
    async fn explore(&self, Parameters(params): Parameters<ExploreParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.explore(&params.name).await {
            Ok(result) => {
                let mut output = format!("## Explore: {}\n\n", params.name);

                if let Some(entity_type) = result.get("entity_type").and_then(|v| v.as_str()) {
                    output.push_str(&format!("**Type:** {}\n\n", entity_type));
                }

                if let Some(matches) = result.get("matches").and_then(|v| v.as_array()) {
                    output.push_str("### Entity\n");
                    for m in matches {
                        if let Some(fp) = m.get("file_path").and_then(|v| v.as_str()) {
                            let line = m.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                            output.push_str(&format!("- **{}** ({}:{})\n", params.name, fp, line));
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

                output
            }
            Err(e) => format!("Error exploring '{}': {}", params.name, e),
        }
    }

    /// Get full context for a file — like opening an Obsidian note with all linked content
    #[tool(
        description = "Get complete context for a file: all functions (with external callers), classes, imports, configs, docs, and packages. \
        Shows cross-file connections. Use this to understand a file's role in the system before reading or modifying it."
    )]
    async fn context_bundle(&self, Parameters(params): Parameters<ContextBundleParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let db = ctx.db.clone();
        let gq = GraphQuery::new(ctx.db);

        match gq.file_context(&params.file_path).await {
            Ok(result) => {
                let mut output = format!("## Context: {}\n\n", params.file_path);

                if let Some(file) = result.get("file") {
                    let lang = file.get("language").and_then(|v| v.as_str()).unwrap_or("?");
                    let lines = file.get("line_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    output.push_str(&format!("**Language:** {} | **Lines:** {}\n\n", lang, lines));
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
                                let cf = caller.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
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
                        output.push_str(&format!("### Incoming Cross-File Calls ({})\n", incoming.len()));
                        for c in &incoming {
                            let caller = c.get("caller").and_then(|v| v.as_str()).unwrap_or("?");
                            let file = c.get("caller_file").and_then(|v| v.as_str()).unwrap_or("?");
                            output.push_str(&format!("- **{}** from {}\n", caller, file));
                        }
                        output.push('\n');
                    }

                    if !outgoing.is_empty() {
                        output.push_str(&format!("### Outgoing Cross-File Calls ({})\n", outgoing.len()));
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

                output
            }
            Err(e) => format!("Error getting context for '{}': {}", params.file_path, e),
        }
    }

    /// Search across ALL entity types — universal knowledge graph search
    #[tool(
        description = "Search across the entire knowledge graph: code, configs, docs, packages, infrastructure. \
        Unlike search_functions which only searches functions, this searches everything. \
        Use this when you need to find all mentions of a concept across code AND non-code files."
    )]
    async fn related(&self, Parameters(params): Parameters<RelatedParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);
        let limit = params.limit.unwrap_or(10);

        match gq.cross_search(&params.keyword, limit).await {
            Ok(result) => {
                let total = result
                    .get("total_results")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let mut output = format!("## Related: '{}' ({} results)\n\n", params.keyword, total);

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
            Err(e) => format!("Error searching for '{}': {}", params.keyword, e),
        }
    }

    /// Find all entities that reference/link to a given entity — Obsidian-like backlinks
    #[tool(
        description = "Find all backlinks to an entity: who calls it, who imports it, what file contains it, what depends on it. \
        Like Obsidian's backlinks panel — shows everything that points TO this entity. \
        Use this to understand the impact of changing something."
    )]
    async fn backlinks(&self, Parameters(params): Parameters<BacklinksParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);

        match gq.backlinks(&params.name).await {
            Ok(result) => {
                let total = result
                    .get("total_backlinks")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let mut output = format!("## Backlinks: {} ({} links)\n\n", params.name, total);

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
                    output.push_str("No backlinks found. The entity may not exist or has no incoming references.\n");
                }

                output
            }
            Err(e) => format!("Error finding backlinks for '{}': {}", params.name, e),
        }
    }
}

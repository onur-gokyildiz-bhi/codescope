//! Code quality tools: lint (dead_code|smells|custom), edit_preflight.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::helpers::maybe_archive;
use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = quality_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Lint: run a built-in or custom quality check.
    #[tool(description = "Quality lint checks (dead_code, smells, or a custom SurrealQL rule).")]
    async fn lint(&self, Parameters(params): Parameters<LintParams>) -> String {
        match params.mode.as_str() {
            "dead_code" => {
                let out = lint_dead_code(self, &params).await;
                maybe_archive(self.result_archive(), "lint_dead_code", out).await
            }
            "smells" => {
                let out = lint_smells(self, &params).await;
                maybe_archive(self.result_archive(), "lint_smells", out).await
            }
            "custom" => lint_custom(self, &params).await,
            other => format!(
                "Unknown lint mode '{}'. Use 'dead_code', 'smells', or 'custom'.",
                other
            ),
        }
    }

    /// Pre-flight check before editing a file — validates against team patterns
    #[tool(description = "Check if edit aligns with team coding patterns.")]
    async fn edit_preflight(&self, Parameters(params): Parameters<EditPreflightParams>) -> String {
        let ctx = match self.gated_ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let mut warnings = Vec::new();
        let mut info = Vec::new();

        let file_q = format!(
            "SELECT language FROM file WHERE path CONTAINS '{}' AND repo = '{}' LIMIT 1",
            params.file_path.replace('\'', ""),
            ctx.repo_name.replace('\'', "")
        );
        let mut lang = "unknown".to_string();
        if let Ok(mut r) = ctx.db.query(&file_q).await {
            let files: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
            if let Some(f) = files.first() {
                lang = f
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
            }
        }

        let name = &params.entity_name;
        let has_underscore = name.contains('_');
        let starts_lower = name
            .chars()
            .next()
            .map(|c| c.is_lowercase())
            .unwrap_or(true);

        match lang.as_str() {
            "rust" | "python" | "ruby" | "elixir" => {
                if !has_underscore && name.len() > 3 && starts_lower {
                    warnings.push(format!(
                        "Naming: '{}' uses camelCase but {} convention is snake_case",
                        name, lang
                    ));
                }
            }
            "typescript" | "javascript" | "java" | "dart" | "kotlin" | "go" => {
                if has_underscore && starts_lower {
                    warnings.push(format!(
                        "Naming: '{}' uses snake_case but {} convention is camelCase",
                        name, lang
                    ));
                }
            }
            _ => {}
        }

        let siblings_q = format!(
            "SELECT name FROM `function` WHERE file_path CONTAINS '{}' AND repo = '{}' LIMIT 20",
            params.file_path.replace('\'', ""),
            ctx.repo_name.replace('\'', "")
        );
        if let Ok(mut r) = ctx.db.query(&siblings_q).await {
            let siblings: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
            let sibling_names: Vec<&str> = siblings
                .iter()
                .filter_map(|s| s.get("name").and_then(|v| v.as_str()))
                .collect();

            if !sibling_names.is_empty() {
                let snake_count = sibling_names.iter().filter(|n| n.contains('_')).count();
                let ratio = snake_count as f32 / sibling_names.len() as f32;

                if ratio > 0.7 && !has_underscore && name.len() > 3 {
                    warnings.push(format!(
                        "Style: {}% of siblings use snake_case, but '{}' doesn't",
                        (ratio * 100.0) as u32,
                        name
                    ));
                } else if ratio < 0.3 && has_underscore {
                    warnings.push(format!(
                        "Style: {}% of siblings use camelCase, but '{}' uses snake_case",
                        ((1.0 - ratio) * 100.0) as u32,
                        name
                    ));
                }

                info.push(format!(
                    "File has {} existing functions",
                    sibling_names.len()
                ));
            }
        }

        let size_q = format!(
            "SELECT line_count FROM file WHERE path CONTAINS '{}' AND repo = '{}' LIMIT 1",
            params.file_path.replace('\'', ""),
            ctx.repo_name.replace('\'', "")
        );
        if let Ok(mut r) = ctx.db.query(&size_q).await {
            let sizes: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
            if let Some(s) = sizes.first() {
                let lines = s.get("line_count").and_then(|v| v.as_u64()).unwrap_or(0);
                if lines > 500 {
                    warnings.push(format!(
                        "File size: {} lines — consider splitting into smaller modules",
                        lines
                    ));
                }
                info.push(format!("File is {} lines", lines));
            }
        }

        let mut output = format!(
            "## Edit Preflight: {} in {}\n\n",
            params.entity_name, params.file_path
        );
        output.push_str(&format!("**Language:** {}\n\n", lang));

        if warnings.is_empty() {
            output.push_str("**All checks passed.** Edit aligns with team patterns.\n\n");
        } else {
            output.push_str(&format!("**{} warnings:**\n", warnings.len()));
            for w in &warnings {
                output.push_str(&format!("- {} {}\n", "!!!", w));
            }
            output.push('\n');
        }

        if !info.is_empty() {
            output.push_str("**Context:**\n");
            for i in &info {
                output.push_str(&format!("- {}\n", i));
            }
        }

        output
    }
}

// === Lint mode helpers (not registered as tools) ===

async fn lint_dead_code(server: &GraphRagServer, params: &LintParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let min_lines = params.min_lines.unwrap_or(3);
    let limit = params.limit.unwrap_or(50);

    let query = format!(
        "SELECT name, file_path, start_line, end_line, signature, \
                math::max(end_line - start_line, 0) AS size \
         FROM `function` \
         WHERE count(<-calls) = 0 \
           AND repo = '{}' \
           AND end_line > start_line \
           AND math::max(end_line - start_line, 0) >= {} \
           AND name != 'main' \
           AND !(name ~ '^test') \
           AND !(name ~ '_test$') \
           AND !(name ~ 'handler') \
           AND !(name ~ '^new$') \
           AND !(name ~ '^default$') \
           AND !(name ~ '^from$') \
           AND !(name ~ '^into$') \
           AND !(name ~ '^drop$') \
           AND !(name ~ '^fmt$') \
           AND !(name ~ '^serialize$') \
           AND !(name ~ '^deserialize$') \
           AND !(signature ~ 'override') \
           AND !(signature ~ 'virtual') \
           AND !(signature ~ 'abstract') \
           AND !(signature ~ '@Override') \
           AND !(name ~ '^Execute') \
           AND !(name ~ '^On[A-Z]') \
           AND !(name ~ '^Handle[A-Z]') \
           AND !(name ~ 'Async$') \
         ORDER BY end_line - start_line DESC \
         LIMIT {}",
        ctx.repo_name.replace('\'', ""),
        min_lines,
        limit
    );

    match ctx.db.query(&query).await {
        Ok(mut response) => {
            let results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
            if results.is_empty() {
                return "No dead code found (all functions have callers or are entry points)."
                    .into();
            }

            let mut output = format!(
                "## Dead Code: {} potentially unused functions\n\n",
                results.len()
            );
            output.push_str("| # | Function | File | Lines | Size |\n");
            output.push_str("|---|----------|------|-------|------|\n");

            for (i, r) in results.iter().enumerate() {
                let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                let fp = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                let start = r.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                let size = r.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                output.push_str(&format!(
                    "| {} | **{}** | {} | L{} | {} lines |\n",
                    i + 1,
                    name,
                    fp,
                    start,
                    size
                ));
            }

            output.push_str(&format!(
                "\n*Filtered: min {} lines, excluded main/test/handler/trait impls.*",
                min_lines
            ));
            output
        }
        Err(e) => format!("Error finding dead code: {}", e),
    }
}

async fn lint_smells(server: &GraphRagServer, params: &LintParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let limit = params.limit.unwrap_or(10);
    let mut output = "## Code Smell Report\n\n".to_string();

    let god_q = format!(
        "SELECT name, file_path, math::max(end_line - start_line, 0) AS lines \
         FROM `function` WHERE end_line - start_line > 200 AND repo = '{}' ORDER BY end_line - start_line DESC LIMIT {}",
        ctx.repo_name.replace('\'', ""),
        limit
    );
    if let Ok(mut r) = ctx.db.query(&god_q).await {
        let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
        output.push_str(&format!(
            "### God Functions (>200 lines): {}\n",
            results.len()
        ));
        if results.is_empty() {
            output.push_str("None found.\n\n");
        } else {
            output.push_str("| Function | File | Lines |\n|----------|------|-------|\n");
            for r in &results {
                let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                let fp = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                let lines = r.get("lines").and_then(|v| v.as_u64()).unwrap_or(0);
                output.push_str(&format!("| **{}** | {} | {} |\n", name, fp, lines));
            }
            output.push('\n');
        }
    }

    let fanin_q = format!(
        "SELECT out.name AS name, out.file_path AS file_path, count() AS caller_count \
         FROM calls WHERE out.repo = '{}' AND in.repo = '{}' \
         GROUP BY out.name, out.file_path ORDER BY caller_count DESC LIMIT {}",
        ctx.repo_name.replace('\'', ""),
        ctx.repo_name.replace('\'', ""),
        limit
    );
    if let Ok(mut r) = ctx.db.query(&fanin_q).await {
        let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
        output.push_str(&format!(
            "### High Fan-In (most callers): {}\n",
            results.len()
        ));
        if !results.is_empty() {
            output.push_str("| Function | File | Callers |\n|----------|------|---------|\n");
            for r in &results {
                let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                let fp = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                let count = r.get("caller_count").and_then(|v| v.as_u64()).unwrap_or(0);
                output.push_str(&format!("| **{}** | {} | {} |\n", name, fp, count));
            }
            output.push('\n');
        }
    }

    let fanout_q = format!(
        "SELECT in.name AS name, in.file_path AS file_path, count() AS callee_count \
         FROM calls WHERE out.repo = '{}' AND in.repo = '{}' \
         GROUP BY in.name, in.file_path ORDER BY callee_count DESC LIMIT {}",
        ctx.repo_name.replace('\'', ""),
        ctx.repo_name.replace('\'', ""),
        limit
    );
    if let Ok(mut r) = ctx.db.query(&fanout_q).await {
        let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
        output.push_str(&format!(
            "### High Fan-Out (most callees): {}\n",
            results.len()
        ));
        if !results.is_empty() {
            output.push_str("| Function | File | Callees |\n|----------|------|---------|\n");
            for r in &results {
                let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                let fp = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                let count = r.get("callee_count").and_then(|v| v.as_u64()).unwrap_or(0);
                output.push_str(&format!("| **{}** | {} | {} |\n", name, fp, count));
            }
            output.push('\n');
        }
    }

    let dense_q = format!(
        "SELECT file_path, count() AS func_count FROM `function` WHERE repo = '{}' GROUP BY file_path ORDER BY func_count DESC LIMIT {}",
        ctx.repo_name.replace('\'', ""),
        limit
    );
    if let Ok(mut r) = ctx.db.query(&dense_q).await {
        let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
        output.push_str(&format!(
            "### Dense Files (most functions): {}\n",
            results.len()
        ));
        if !results.is_empty() {
            output.push_str("| File | Functions |\n|------|-----------|\n");
            for r in &results {
                let fp = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                let count = r.get("func_count").and_then(|v| v.as_u64()).unwrap_or(0);
                output.push_str(&format!("| {} | {} |\n", fp, count));
            }
            output.push('\n');
        }
    }

    let gq = GraphQuery::new(ctx.db.clone());
    let cycles = gq
        .detect_circular_deps(&ctx.repo_name)
        .await
        .unwrap_or_default();
    if !cycles.is_empty() {
        output.push_str(&format!("\n### Circular Dependencies ({})\n", cycles.len()));
        for c in &cycles {
            let a = c.get("file_a").and_then(|v| v.as_str()).unwrap_or("?");
            let b = c.get("file_b").and_then(|v| v.as_str()).unwrap_or("?");
            output.push_str(&format!("- {} <-> {}\n", a, b));
        }
    }

    let dupes = gq
        .find_duplicate_functions(&ctx.repo_name)
        .await
        .unwrap_or_default();
    if !dupes.is_empty() {
        output.push_str(&format!("\n### Duplicate Functions ({})\n", dupes.len()));
        for d in &dupes {
            let names = d
                .get("names")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let cnt = d.get("cnt").and_then(|v| v.as_u64()).unwrap_or(0);
            output.push_str(&format!("- {} identical copies: {}\n", cnt, names));
        }
    }

    output
}

async fn lint_custom(server: &GraphRagServer, params: &LintParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let rule = match params.query.as_deref() {
        Some(q) if !q.trim().is_empty() => q,
        _ => return "Missing 'query' (SurrealQL) for mode=custom.".into(),
    };
    let description = params.description.as_deref().unwrap_or("custom rule");

    let mut output = format!("## Custom Lint: {}\n\n", description);
    output.push_str(&format!("**Rule query:** `{}`\n\n", rule));

    match ctx.db.query(rule).await {
        Ok(mut response) => {
            let results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
            if results.is_empty() {
                output.push_str("No violations found.\n");
            } else {
                output.push_str(&format!("**{} violations found:**\n\n", results.len()));
                for (i, r) in results.iter().enumerate() {
                    output.push_str(&format!("{}. ", i + 1));
                    if let Some(obj) = r.as_object() {
                        let parts: Vec<String> = obj
                            .iter()
                            .filter(|(k, _)| k.as_str() != "id")
                            .map(|(k, v)| {
                                let val = match v.as_str() {
                                    Some(s) => s.to_string(),
                                    None => v.to_string(),
                                };
                                format!("**{}**: {}", k, val)
                            })
                            .collect();
                        output.push_str(&parts.join(" | "));
                    } else {
                        output.push_str(&r.to_string());
                    }
                    output.push('\n');
                }
            }
            output
        }
        Err(e) => {
            output.push_str(&format!("Query error: {}\n", e));
            output
        }
    }
}

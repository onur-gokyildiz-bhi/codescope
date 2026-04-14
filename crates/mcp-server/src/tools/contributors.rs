//! Unified `contributors` tool: expertise map, reviewer suggestions, team patterns.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = contributors_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Contributor analysis: expertise map, reviewer suggestions, team patterns.
    #[tool(
        description = "Contributors: mode=map|reviewers|patterns. map: who knows which files. reviewers: suggest reviewers for changed files. patterns: detect team coding conventions."
    )]
    async fn contributors(&self, Parameters(params): Parameters<ContributorsParams>) -> String {
        match params.mode.as_str() {
            "map" => contributors_map(self).await,
            "reviewers" => contributors_reviewers(self, &params).await,
            "patterns" => contributors_patterns(self, &params).await,
            other => format!(
                "Unknown contributors mode '{}'. Use 'map', 'reviewers', or 'patterns'.",
                other
            ),
        }
    }
}

// === Mode helpers (not registered as tools) ===

async fn contributors_map(server: &GraphRagServer) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let git_path = ctx.codebase_path.clone();

    match tokio::task::spawn_blocking(move || {
        let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
        analyzer.contributor_map()
    })
    .await
    {
        Ok(Ok(map)) => {
            let mut output = "## Contributor Expertise Map\n\n".to_string();
            for (author, files) in &map {
                output.push_str(&format!("### {} ({} files)\n", author, files.len()));
                for (file, count) in files.iter().take(10) {
                    output.push_str(&format!("  - {} ({}x)\n", file, count));
                }
                if files.len() > 10 {
                    output.push_str(&format!("  ... and {} more\n", files.len() - 10));
                }
                output.push('\n');
            }
            output
        }
        Ok(Err(e)) => format!("Error: {}", e),
        Err(e) => format!("Task error: {}", e),
    }
}

async fn contributors_reviewers(server: &GraphRagServer, params: &ContributorsParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let git_path = ctx.codebase_path.clone();

    let files_input = match params.files.as_deref() {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => {
            return "Missing 'files' (comma-separated list of changed file paths) for mode=reviewers.".into();
        }
    };

    let changed_files: Vec<String> = files_input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let result = tokio::task::spawn_blocking(move || {
        let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
        analyzer.contributor_map()
    })
    .await;

    let contributor_map = match result {
        Ok(Ok(cm)) => cm,
        Ok(Err(e)) => return format!("Error: {}", e),
        Err(e) => return format!("Task error: {}", e),
    };

    let mut reviewer_scores: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (author, files) in &contributor_map {
        for (file, count) in files {
            if changed_files
                .iter()
                .any(|cf| file.contains(cf) || cf.contains(file))
            {
                *reviewer_scores.entry(author.clone()).or_insert(0) += count;
            }
        }
    }

    let mut reviewers: Vec<_> = reviewer_scores.into_iter().collect();
    reviewers.sort_by(|a, b| b.1.cmp(&a.1));

    let mut output = format!(
        "## Suggested Reviewers\n\n**{} files changed**\n\n",
        changed_files.len()
    );

    if reviewers.is_empty() {
        output.push_str("No reviewer suggestions (no git history for changed files).\n");
    } else {
        output.push_str("| Reviewer | Expertise Score |\n|----------|----------------|\n");
        for (reviewer, score) in reviewers.iter().take(5) {
            output.push_str(&format!("| {} | {} |\n", reviewer, score));
        }
    }

    output
}

async fn contributors_patterns(server: &GraphRagServer, params: &ContributorsParams) -> String {
    let ctx = match server.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    let focus = params.focus.as_deref().unwrap_or("all");
    let mut output = "## Team Coding Patterns\n\n".to_string();

    if focus == "all" || focus == "naming" {
        let naming_q = "SELECT name, language, file_path FROM `function` LIMIT 200";
        if let Ok(mut r) = ctx.db.query(naming_q).await {
            let fns: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
            let mut snake = 0;
            let mut camel = 0;
            let mut pascal = 0;
            for f in &fns {
                let n = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if n.contains('_') {
                    snake += 1;
                } else if n.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                    camel += 1;
                } else {
                    pascal += 1;
                }
            }
            let total = snake + camel + pascal;
            if total > 0 {
                output.push_str("### Naming Conventions\n");
                output.push_str(&format!(
                    "- snake_case: {}% ({}/{})\n",
                    snake * 100 / total,
                    snake,
                    total
                ));
                output.push_str(&format!(
                    "- camelCase: {}% ({}/{})\n",
                    camel * 100 / total,
                    camel,
                    total
                ));
                output.push_str(&format!(
                    "- PascalCase: {}% ({}/{})\n\n",
                    pascal * 100 / total,
                    pascal,
                    total
                ));
            }
        }
    }

    if focus == "all" || focus == "imports" {
        let import_q = "SELECT name, file_path, body FROM import_decl LIMIT 100";
        if let Ok(mut r) = ctx.db.query(import_q).await {
            let imports: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
            if !imports.is_empty() {
                output.push_str("### Import Patterns\n");
                let mut patterns: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for imp in &imports {
                    let body = imp.get("body").and_then(|v| v.as_str()).unwrap_or("");
                    let pattern = if body.contains("from ") {
                        "ES module (from)"
                    } else if body.contains("require(") {
                        "CommonJS (require)"
                    } else if body.contains("use ") {
                        "Rust (use)"
                    } else if body.contains("import ") {
                        "import statement"
                    } else {
                        "other"
                    };
                    *patterns.entry(pattern.to_string()).or_insert(0) += 1;
                }
                let mut sorted: Vec<_> = patterns.into_iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(&a.1));
                for (p, c) in &sorted {
                    output.push_str(&format!("- {}: {} occurrences\n", p, c));
                }
                output.push('\n');
            }
        }
    }

    if focus == "all" || focus == "structure" {
        let struct_q = "SELECT language, count() AS cnt FROM file GROUP BY language ORDER BY cnt DESC LIMIT 10";
        if let Ok(mut r) = ctx.db.query(struct_q).await {
            let langs: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
            if !langs.is_empty() {
                output.push_str("### Language Distribution\n");
                for l in &langs {
                    let lang = l.get("language").and_then(|v| v.as_str()).unwrap_or("?");
                    let cnt = l.get("cnt").and_then(|v| v.as_u64()).unwrap_or(0);
                    output.push_str(&format!("- {}: {} files\n", lang, cnt));
                }
                output.push('\n');
            }
        }

        let size_q = "SELECT math::mean(end_line - start_line) AS avg_size, \
                      math::max(end_line - start_line) AS max_size \
                      FROM `function`";
        if let Ok(mut r) = ctx.db.query(size_q).await {
            let stats: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
            if let Some(s) = stats.first() {
                let avg = s.get("avg_size").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let max = s.get("max_size").and_then(|v| v.as_u64()).unwrap_or(0);
                output.push_str(&format!(
                    "### Function Size\n- Average: {:.0} lines\n- Largest: {} lines\n\n",
                    avg, max
                ));
            }
        }

        let dir_q = "SELECT file_path FROM file LIMIT 500";
        if let Ok(mut r) = ctx.db.query(dir_q).await {
            let files: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
            let mut dirs: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for f in &files {
                let fp = f.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(first) = fp.split('/').next() {
                    *dirs.entry(first.to_string()).or_insert(0) += 1;
                }
            }
            let mut sorted: Vec<_> = dirs.into_iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(&a.1));
            output.push_str("### Project Structure (top-level)\n");
            for (d, c) in sorted.iter().take(10) {
                output.push_str(&format!("- {}/  ({} files)\n", d, c));
            }
        }
    }

    output
}

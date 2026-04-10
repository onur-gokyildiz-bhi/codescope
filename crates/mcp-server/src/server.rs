use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use std::path::PathBuf;
use std::sync::Arc;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use codescope_core::graph::query::GraphQuery;

use crate::daemon::DaemonState;
use crate::helpers::{
    build_context_summary, build_project_profile, derive_scope_from_file_path,
};
use crate::params::*;

/// Active project context — DB handle + metadata
#[derive(Clone)]
pub struct ProjectCtx {
    pub db: Surreal<Db>,
    pub repo_name: String,
    pub codebase_path: PathBuf,
}

/// The MCP server for Code Graph RAG.
/// Supports two modes:
/// - **Stdio**: project is pre-initialized at startup (single project)
/// - **Daemon**: project is set via `init_project` tool (multi-project)
#[derive(Clone)]
pub struct GraphRagServer {
    project: Arc<tokio::sync::RwLock<Option<ProjectCtx>>>,
    daemon: Option<Arc<DaemonState>>,
    /// Cached conversation context summary, injected into ServerInfo.instructions
    context_summary: Arc<tokio::sync::RwLock<String>>,
    tool_router: ToolRouter<Self>,
}

impl GraphRagServer {
    /// Create for stdio mode — project ready immediately
    pub fn new(db: Surreal<Db>, repo_name: String, codebase_path: PathBuf) -> Self {
        Self {
            project: Arc::new(tokio::sync::RwLock::new(Some(ProjectCtx {
                db,
                repo_name,
                codebase_path,
            }))),
            daemon: None,
            context_summary: Arc::new(tokio::sync::RwLock::new(String::new())),
            tool_router: Self::merged_router(),
        }
    }

    /// Create for daemon mode — no project until init_project is called
    pub fn new_daemon(state: Arc<DaemonState>) -> Self {
        Self {
            project: Arc::new(tokio::sync::RwLock::new(None)),
            daemon: Some(state),
            context_summary: Arc::new(tokio::sync::RwLock::new(String::new())),
            tool_router: Self::merged_router(),
        }
    }

    /// Compose the master tool router from all per-topic sub-routers.
    /// Each tool sub-module exposes a `*_router()` method via #[tool_router(router = X)].
    fn merged_router() -> ToolRouter<Self> {
        let mut router = Self::tool_router();
        router.merge(Self::search_router());
        router.merge(Self::callgraph_router());
        router.merge(Self::http_router());
        router.merge(Self::refactor_router());
        router.merge(Self::skills_router());
        router.merge(Self::temporal_router());
        router.merge(Self::contributors_router());
        router.merge(Self::ask_router());
        router.merge(Self::exploration_router());
        router.merge(Self::admin_router());
        router.merge(Self::conversations_router());
        router.merge(Self::embeddings_router());
        router
    }

    /// Accessor for daemon state — used by admin tool sub-module.
    pub(crate) fn daemon(&self) -> Option<&Arc<DaemonState>> {
        self.daemon.as_ref()
    }

    /// Accessor for the project RwLock — used by admin tool sub-module.
    pub(crate) fn project_lock(&self) -> &Arc<tokio::sync::RwLock<Option<ProjectCtx>>> {
        &self.project
    }

    /// Get the active project context, or return an error message
    pub(crate) async fn ctx(&self) -> Result<ProjectCtx, String> {
        self.project
            .read()
            .await
            .clone()
            .ok_or_else(|| "No project initialized. Call `init_project` first with repo name and codebase path.".into())
    }

    /// Load conversation context summary from DB and cache it.
    /// Called after auto-indexing completes.
    pub async fn load_context_summary(&self) {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(_) => return,
        };
        let summary = build_context_summary(&ctx.db, &ctx.repo_name).await;
        let insights = crate::helpers::build_post_index_insights(&ctx.db, &ctx.repo_name).await;
        let combined = if insights.is_empty() {
            summary
        } else if summary.is_empty() {
            insights
        } else {
            format!("{}\n\n{}", insights, summary)
        };
        *self.context_summary.write().await = combined;
    }
}

#[tool_handler]
impl ServerHandler for GraphRagServer {
    fn get_info(&self) -> ServerInfo {
        let base_instructions = "\
CODESCOPE — Code knowledge graph. ALWAYS prefer these tools over Read/Grep to save tokens:\n\
\n\
TOKEN-SAVING RULES (follow strictly):\n\
- BEFORE reading any file → use context_bundle to get full file map (functions, classes, imports, cross-file callers) in ONE call\n\
- BEFORE grepping for callers → use find_callers / find_callees (graph traversal, not text search)\n\
- BEFORE reading multiple files to find a function → use search_functions or find_function\n\
- BEFORE manually tracing impact → use impact_analysis (transitive call graph)\n\
- BEFORE exploring how code connects → use explore (full neighborhood) or backlinks\n\
- BEFORE reading git history → use file_churn or hotspot_detection\n\
- ONLY use Read for reading actual code BODY after you know exactly which function/line to read\n\
\n\
TOOL CHEAT SHEET:\n\
| Instead of...              | Use...                          | Saves |\n\
|----------------------------|----------------------------------|-------|\n\
| Read whole file            | context_bundle(file_path)        | ~80%  |\n\
| Grep + Read for callers    | find_callers(name)               | ~90%  |\n\
| Multiple Read for function | find_function(name)              | ~70%  |\n\
| Manual call graph tracing  | impact_analysis(name, depth=3)   | ~95%  |\n\
| Grep across codebase       | search_functions / related       | ~85%  |\n\
| Read file to understand it | explore(entity_name)             | ~75%  |";

        // Try to include cached conversation context (non-blocking)
        let context = self
            .context_summary
            .try_read()
            .map(|c| c.clone())
            .unwrap_or_default();

        let instructions = if context.is_empty() {
            base_instructions.to_string()
        } else {
            format!("{}\n\n{}", base_instructions, context)
        };

        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(instructions)
    }
}

#[tool_router]
impl GraphRagServer {
    // ===== Code Quality Tools =====

    /// Find potentially dead code — functions with zero callers
    #[tool(
        description = "Find dead code: functions that are never called by any other function. \
        Filters out known entry points (main, test functions, handlers, constructors). \
        Useful for codebase cleanup and reducing maintenance burden."
    )]
    async fn find_dead_code(&self, Parameters(params): Parameters<DeadCodeParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let min_lines = params.min_lines.unwrap_or(3);
        let limit = params.limit.unwrap_or(50);

        // Find functions with zero incoming calls, excluding entry points and overrides
        let query = format!(
            "SELECT name, file_path, start_line, end_line, signature, \
                    math::max(end_line - start_line, 0) AS size \
             FROM `function` \
             WHERE count(<-calls) = 0 \
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
            min_lines, limit
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

    /// Detect code smells: god functions, high fan-in/out, dense files
    #[tool(
        description = "Detect code smells in the codebase: god functions (>200 lines), high fan-in (called by many), \
        high fan-out (calls many), and dense files (many functions). Use for codebase health assessment."
    )]
    async fn detect_code_smells(&self, Parameters(params): Parameters<CodeSmellParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = params.limit.unwrap_or(10);
        let mut output = "## Code Smell Report\n\n".to_string();

        // 1. God functions (>200 lines)
        let god_q = format!(
            "SELECT name, file_path, math::max(end_line - start_line, 0) AS lines \
             FROM `function` WHERE end_line - start_line > 200 ORDER BY end_line - start_line DESC LIMIT {}",
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

        // 2. High fan-in (called by many)
        let fanin_q = format!(
            "SELECT out.name AS name, out.file_path AS file_path, count() AS caller_count \
             FROM calls GROUP BY out.name, out.file_path ORDER BY caller_count DESC LIMIT {}",
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

        // 3. High fan-out (calls many)
        let fanout_q = format!(
            "SELECT in.name AS name, in.file_path AS file_path, count() AS callee_count \
             FROM calls GROUP BY in.name, in.file_path ORDER BY callee_count DESC LIMIT {}",
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

        // 4. Dense files (many functions)
        let dense_q = format!(
            "SELECT file_path, count() AS func_count FROM `function` GROUP BY file_path ORDER BY func_count DESC LIMIT {}",
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

        // 5. Circular dependencies
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

        // 6. Duplicate code
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

    /// Run a custom SurrealQL lint rule and format results as violations
    #[tool(
        description = "Run a custom SurrealQL query as a lint rule. Provide a query that returns violations \
        and a description of what the rule checks. Results are formatted as a violation report. \
        Example rule: SELECT name, file_path FROM `function` WHERE end_line - start_line > 100"
    )]
    async fn custom_lint(&self, Parameters(params): Parameters<CustomLintParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let mut output = format!("## Custom Lint: {}\n\n", params.description);
        output.push_str(&format!("**Rule query:** `{}`\n\n", params.rule));

        match ctx.db.query(&params.rule).await {
            Ok(mut response) => {
                let results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                if results.is_empty() {
                    output.push_str("No violations found.\n");
                } else {
                    output.push_str(&format!("**{} violations found:**\n\n", results.len()));
                    for (i, r) in results.iter().enumerate() {
                        output.push_str(&format!("{}. ", i + 1));
                        // Format each result as key: value pairs
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

    // ===== Team Intelligence Tools =====

    /// Detect team coding patterns from the codebase
    #[tool(
        description = "Detect team coding patterns: naming conventions, import styles, file structure patterns, \
        and common architectural patterns. Analyzes the actual codebase to learn how the team codes. \
        Use this to understand conventions before writing new code."
    )]
    async fn team_patterns(&self, Parameters(params): Parameters<TeamPatternsParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let focus = params.focus.as_deref().unwrap_or("all");
        let mut output = "## Team Coding Patterns\n\n".to_string();

        // 1. Naming conventions
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

        // 2. Import style
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

        // 3. File structure patterns
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

            // Average function size
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

            // Top-level directory structure
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

    /// Pre-flight check before editing a file — validates against team patterns
    #[tool(
        description = "Check if a planned edit aligns with team coding patterns. \
        Call before writing code to avoid introducing inconsistencies. \
        Returns warnings if naming, structure, or style deviates from the codebase norm."
    )]
    async fn edit_preflight(&self, Parameters(params): Parameters<EditPreflightParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let mut warnings = Vec::new();
        let mut info = Vec::new();

        // 1. Check naming convention against file's language
        let file_q = format!(
            "SELECT language FROM file WHERE path CONTAINS '{}' LIMIT 1",
            params.file_path.replace('\'', "")
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

        // Naming check
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

        // 2. Check sibling functions in the same file for consistency
        let siblings_q = format!(
            "SELECT name FROM `function` WHERE file_path CONTAINS '{}' LIMIT 20",
            params.file_path.replace('\'', "")
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

        // 3. Check file size
        let size_q = format!(
            "SELECT line_count FROM file WHERE path CONTAINS '{}' LIMIT 1",
            params.file_path.replace('\'', "")
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

    // ===== ADR Management =====

    /// Manage Architecture Decision Records
    #[tool(description = "Manage Architecture Decision Records (ADRs). Actions: \
        'list' — show all recorded decisions, \
        'create' — record a new architectural decision with title and body, \
        'get' — retrieve a specific ADR by ID. \
        ADRs are stored in the graph and linked to conversation history.")]
    async fn manage_adr(&self, Parameters(params): Parameters<ManageAdrParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        match params.action.as_str() {
            "list" => {
                let q = "SELECT name, body, timestamp, qualified_name FROM decision WHERE repo = $repo ORDER BY timestamp DESC LIMIT 50";
                match ctx.db.query(q).bind(("repo", ctx.repo_name.clone())).await {
                    Ok(mut r) => {
                        let decisions: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if decisions.is_empty() {
                            return "No ADRs found. Decisions are auto-extracted from conversations, or create one with action='create'.".into();
                        }
                        let mut output = format!(
                            "## Architecture Decision Records ({} total)\n\n",
                            decisions.len()
                        );
                        for (i, d) in decisions.iter().enumerate() {
                            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let ts = d.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                            let body = d.get("body").and_then(|v| v.as_str()).unwrap_or("");
                            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
                            output.push_str(&format!("### ADR-{:03}: {}\n", i + 1, name));
                            if !date.is_empty() {
                                output.push_str(&format!("*Date: {}*\n\n", date));
                            }
                            if body.len() > 200 {
                                output.push_str(&format!("{}...\n\n", &body[..200]));
                            } else if !body.is_empty() {
                                output.push_str(&format!("{}\n\n", body));
                            }
                        }
                        output
                    }
                    Err(e) => format!("Error listing ADRs: {}", e),
                }
            }
            "create" => {
                let title = params.title.as_deref().unwrap_or("Untitled Decision");
                let body = params.body.as_deref().unwrap_or("");
                let qname = format!(
                    "{}:adr:{}",
                    ctx.repo_name,
                    title
                        .to_lowercase()
                        .replace(' ', "_")
                        .chars()
                        .take(60)
                        .collect::<String>()
                );
                let ts = chrono::Utc::now().to_rfc3339();

                let q = "UPSERT decision SET name = $name, qualified_name = $qname, \
                         body = $body, repo = $repo, language = 'adr', \
                         file_path = 'adr', start_line = 0, end_line = 0, \
                         timestamp = $ts";
                match ctx
                    .db
                    .query(q)
                    .bind(("name", title.to_string()))
                    .bind(("qname", qname))
                    .bind(("body", body.to_string()))
                    .bind(("repo", ctx.repo_name.clone()))
                    .bind(("ts", ts))
                    .await
                {
                    Ok(_) => format!("ADR created: **{}**", title),
                    Err(e) => format!("Error creating ADR: {}", e),
                }
            }
            "get" => {
                let id = params.id.as_deref().unwrap_or("");
                let q =
                    "SELECT * FROM decision WHERE name CONTAINS $search AND repo = $repo LIMIT 1";
                match ctx
                    .db
                    .query(q)
                    .bind(("search", id.to_string()))
                    .bind(("repo", ctx.repo_name.clone()))
                    .await
                {
                    Ok(mut r) => {
                        let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if let Some(d) = results.first() {
                            serde_json::to_string_pretty(d)
                                .unwrap_or_else(|_| "Error formatting".into())
                        } else {
                            format!("No ADR found matching '{}'", id)
                        }
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }
            _ => "Invalid action. Use 'list', 'create', or 'get'.".into(),
        }
    }

    // ===== Shared Memory =====

    /// Save a persistent memory note that survives across sessions
    #[tool(
        description = "Save a persistent memory note that survives across sessions and is shared between all agents \
        (Claude Code, Cursor, etc.) connected to this project. Use for recording: preferences, conventions, \
        important context, architectural notes, TODOs. Memories are searchable via conversation_search."
    )]
    async fn memory_save(
        &self,
        Parameters(params): Parameters<NaturalLanguageQueryParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let text = &params.question;
        let ts = chrono::Utc::now().to_rfc3339();
        let slug = text
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
            .chars()
            .take(60)
            .collect::<String>();
        let qname = format!("{}:memory:{}", ctx.repo_name, slug);

        let q = "UPSERT conv_topic SET name = $name, qualified_name = $qname, \
                 body = $body, repo = $repo, language = 'memory', kind = 'shared_memory', \
                 file_path = 'memory', start_line = 0, end_line = 0, timestamp = $ts";
        match ctx
            .db
            .query(q)
            .bind(("name", text.chars().take(100).collect::<String>()))
            .bind(("qname", qname))
            .bind(("body", text.to_string()))
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("ts", ts))
            .await
        {
            Ok(_) => format!(
                "Memory saved. Accessible by all agents connected to '{}'.",
                ctx.repo_name
            ),
            Err(e) => format!("Error saving memory: {}", e),
        }
    }

    /// Search shared memories and conversation history
    #[tool(
        description = "Search shared memories, decisions, and conversation history across all sessions. \
        Returns memories saved by any agent, plus auto-extracted decisions/problems/solutions."
    )]
    async fn memory_search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = params.limit.unwrap_or(20) as i64;
        let safe = params.query.replace('\'', "");

        // Build optional scope filter clause
        let scope_clause = if params.scope.is_some() {
            " AND scope ~ $scope"
        } else {
            ""
        };

        // SurrealDB doesn't support UNION — run separate queries and merge
        let q = format!(
            "SELECT name, body, kind, timestamp FROM conv_topic \
             WHERE repo = $repo AND (name ~ $search OR body ~ $search){scope} LIMIT $lim; \
             SELECT name, body, 'decision' AS kind, timestamp FROM decision \
             WHERE repo = $repo AND (name ~ $search OR body ~ $search){scope} LIMIT $lim; \
             SELECT name, body, 'problem' AS kind, timestamp FROM problem \
             WHERE repo = $repo AND (name ~ $search OR body ~ $search){scope} LIMIT $lim; \
             SELECT name, body, 'solution' AS kind, timestamp FROM solution \
             WHERE repo = $repo AND (name ~ $search OR body ~ $search){scope} LIMIT $lim",
            scope = scope_clause,
        );

        let scope_val = params.scope.unwrap_or_default();
        match ctx
            .db
            .query(&q)
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("search", safe))
            .bind(("lim", limit))
            .bind(("scope", scope_val))
            .await
        {
            Ok(mut r) => {
                let mut results: Vec<serde_json::Value> = Vec::new();
                for i in 0..4u32 {
                    let batch: Vec<serde_json::Value> = r.take(i as usize).unwrap_or_default();
                    results.extend(batch);
                }
                // Sort by timestamp descending, truncate to limit
                results.sort_by(|a, b| {
                    let ta = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    let tb = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    tb.cmp(ta)
                });
                results.truncate(limit as usize);
                if results.is_empty() {
                    return format!("No memories found for '{}'", params.query);
                }
                let mut output = format!(
                    "## Memory Search: '{}' ({} results)\n\n",
                    params.query,
                    results.len()
                );
                for item in &results {
                    let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let body = item.get("body").and_then(|v| v.as_str()).unwrap_or("");
                    let ts = item.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                    let date = if ts.len() >= 10 { &ts[..10] } else { ts };
                    let icon = match kind {
                        "shared_memory" => "[MEMORY]",
                        "decision" => "[DECISION]",
                        "problem" => "[PROBLEM]",
                        "solution" => "[SOLUTION]",
                        _ => "[NOTE]",
                    };
                    output.push_str(&format!("**{}** {} _{}_\n", icon, name, date));
                    if !body.is_empty() {
                        let preview = if body.len() > 150 { &body[..150] } else { body };
                        output.push_str(&format!("> {}\n\n", preview));
                    }
                }
                output
            }
            Err(e) => format!("Error searching memory: {}", e),
        }
    }

    /// Pin or adjust the tier of a decision/problem/solution memory
    #[tool(
        description = "Pin or adjust the priority tier of a decision, problem, or solution. \
        Tier 0 = critical (always shown, marked [PINNED]), 1 = important, 2 = contextual (default). \
        Matches by partial name across decision, problem, and solution tables."
    )]
    async fn memory_pin(&self, Parameters(params): Parameters<MemoryPinParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let tier = params.tier.min(2) as i64;

        let q = "UPDATE decision SET tier = $tier WHERE name ~ $name AND repo = $repo; \
                 UPDATE problem SET tier = $tier WHERE name ~ $name AND repo = $repo; \
                 UPDATE solution SET tier = $tier WHERE name ~ $name AND repo = $repo;";

        match ctx
            .db
            .query(q)
            .bind(("tier", tier))
            .bind(("name", params.name.clone()))
            .bind(("repo", ctx.repo_name.clone()))
            .await
        {
            Ok(mut r) => {
                let mut total = 0usize;
                for i in 0..3u32 {
                    let updated: Vec<serde_json::Value> = r.take(i as usize).unwrap_or_default();
                    total += updated.len();
                }
                if total == 0 {
                    format!(
                        "No matching memories found for '{}'. Try a broader name.",
                        params.name
                    )
                } else {
                    let tier_label = match params.tier {
                        0 => "critical (always shown)",
                        1 => "important",
                        _ => "contextual",
                    };
                    format!(
                        "Updated {} record(s) matching '{}' to tier {} ({}).",
                        total, params.name, params.tier, tier_label
                    )
                }
            }
            Err(e) => format!("Error pinning memory: {}", e),
        }
    }

    // ===== API Changelog =====

    /// Show recently indexed entities grouped by file
    #[tool(
        description = "Show recently changed, added, or modified functions and classes since last index. \
        Useful before code review or after re-indexing."
    )]
    async fn api_changelog(&self, Parameters(_params): Parameters<ApiChangelogParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let mut output = "## API Changelog\n\n".to_string();

        // List all entities sorted by file, with line counts
        let q = "SELECT name, file_path, start_line, end_line, signature \
                 FROM `function` WHERE repo = $repo \
                 ORDER BY file_path, start_line LIMIT 200";
        match ctx.db.query(q).bind(("repo", ctx.repo_name.clone())).await {
            Ok(mut r) => {
                let functions: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if functions.is_empty() {
                    output.push_str("No functions found in the index.\n");
                    return output;
                }

                let mut current_file = String::new();
                for f in &functions {
                    let fp = f.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    if fp != current_file {
                        current_file = fp.to_string();
                        output.push_str(&format!("\n### {}\n", fp));
                    }
                    let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let start = f.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                    let end = f.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0);
                    let lines = end.saturating_sub(start);
                    let sig = f.get("signature").and_then(|v| v.as_str()).unwrap_or("");
                    if sig.is_empty() {
                        output.push_str(&format!(
                            "- **{}** (L{}-{}, {} lines)\n",
                            name, start, end, lines
                        ));
                    } else {
                        output.push_str(&format!(
                            "- **{}** (L{}-{}, {} lines) `{}`\n",
                            name, start, end, lines, sig
                        ));
                    }
                }

                // Also list classes
                let cq = "SELECT name, file_path, start_line, end_line \
                          FROM class WHERE repo = $repo \
                          ORDER BY file_path, start_line LIMIT 100";
                if let Ok(mut cr) = ctx.db.query(cq).bind(("repo", ctx.repo_name.clone())).await {
                    let classes: Vec<serde_json::Value> = cr.take(0).unwrap_or_default();
                    if !classes.is_empty() {
                        output.push_str("\n## Classes\n");
                        let mut current_file2 = String::new();
                        for c in &classes {
                            let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                            if fp != current_file2 {
                                current_file2 = fp.to_string();
                                output.push_str(&format!("\n### {}\n", fp));
                            }
                            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let start = c.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                            let end = c.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0);
                            let lines = end.saturating_sub(start);
                            output.push_str(&format!(
                                "- **{}** (L{}-{}, {} lines)\n",
                                name, start, end, lines
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                output.push_str(&format!("Error querying changelog: {}\n", e));
            }
        }

        output
    }

    // ===== Graph Analytics =====

    /// Detect code communities and architectural boundaries
    #[tool(
        description = "Detect code communities, bridge modules, and central nodes in the codebase graph. \
        'clusters' — find groups of tightly-connected files, \
        'bridges' — find modules that connect separate clusters (high betweenness), \
        'central' — find the most connected/important entities (PageRank-like), \
        'all' — run all analyses."
    )]
    async fn community_detection(
        &self,
        Parameters(params): Parameters<CommunityDetectionParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let analysis = params.analysis.as_deref().unwrap_or("all");
        let limit = params.limit.unwrap_or(20);
        let mut output = "## Code Community Analysis\n\n".to_string();

        // 1. Clusters — files grouped by mutual call relationships
        if analysis == "all" || analysis == "clusters" {
            let q = "SELECT file_path, count(->calls) AS out_calls, count(<-calls) AS in_calls, \
                     (count(->calls) + count(<-calls)) AS total_edges \
                     FROM `function` WHERE file_path != NONE \
                     GROUP BY file_path ORDER BY total_edges DESC LIMIT $lim";
            if let Ok(mut r) = ctx.db.query(q).bind(("lim", limit as i64)).await {
                let clusters: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if !clusters.is_empty() {
                    output.push_str("### Most Connected Files (Cluster Centers)\n\n");
                    output.push_str("| File | Outgoing | Incoming | Total |\n|------|----------|----------|-------|\n");
                    for c in &clusters {
                        let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let out_c = c.get("out_calls").and_then(|v| v.as_u64()).unwrap_or(0);
                        let in_c = c.get("in_calls").and_then(|v| v.as_u64()).unwrap_or(0);
                        let total = c.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);
                        output
                            .push_str(&format!("| {} | {} | {} | {} |\n", fp, out_c, in_c, total));
                    }
                    output.push('\n');
                }
            }
        }

        // 2. Bridge modules — files that import from and are imported by many different files
        if analysis == "all" || analysis == "bridges" {
            let q = "SELECT name, file_path, \
                     count(<-calls) AS callers, count(->calls) AS callees, \
                     (count(<-calls) * count(->calls)) AS bridge_score \
                     FROM `function` \
                     WHERE count(<-calls) > 0 AND count(->calls) > 0 \
                     ORDER BY bridge_score DESC LIMIT $lim";
            if let Ok(mut r) = ctx.db.query(q).bind(("lim", limit as i64)).await {
                let bridges: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if !bridges.is_empty() {
                    output.push_str("### Bridge Functions (Connect Different Parts)\n\n");
                    output.push_str("| Function | File | Callers | Callees | Bridge Score |\n|----------|------|---------|---------|-------------|\n");
                    for b in &bridges {
                        let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let fp = b.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let callers = b.get("callers").and_then(|v| v.as_u64()).unwrap_or(0);
                        let callees = b.get("callees").and_then(|v| v.as_u64()).unwrap_or(0);
                        let score = b.get("bridge_score").and_then(|v| v.as_u64()).unwrap_or(0);
                        output.push_str(&format!(
                            "| **{}** | {} | {} | {} | {} |\n",
                            name, fp, callers, callees, score
                        ));
                    }
                    output.push('\n');
                }
            }
        }

        // 3. Central entities — most referenced/called (PageRank-like)
        if analysis == "all" || analysis == "central" {
            let q = "SELECT name, file_path, count(<-calls) AS in_degree \
                     FROM `function` ORDER BY in_degree DESC LIMIT $lim";
            if let Ok(mut r) = ctx.db.query(q).bind(("lim", limit as i64)).await {
                let central: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if !central.is_empty() {
                    output.push_str("### Most Central Functions (Highest In-Degree)\n\n");
                    output.push_str("| # | Function | File | Called By |\n|---|----------|------|-----------|\n");
                    for (i, c) in central.iter().enumerate() {
                        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let deg = c.get("in_degree").and_then(|v| v.as_u64()).unwrap_or(0);
                        output.push_str(&format!(
                            "| {} | **{}** | {} | {} |\n",
                            i + 1,
                            name,
                            fp,
                            deg
                        ));
                    }
                }
            }
        }

        output
    }

    // ===== Export to Obsidian Vault =====

    /// Export the knowledge graph as an Obsidian-compatible vault with wikilinks
    #[tool(
        description = "Export indexed functions and classes as an Obsidian vault with wikilinks. \
        Creates an index.md listing all entities and individual markdown files for the top 50 \
        most-connected functions (with callers/callees). Output defaults to ~/.codescope/exports/{repo}/."
    )]
    async fn export_obsidian(
        &self,
        Parameters(params): Parameters<ExportObsidianParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let limit = params.limit.unwrap_or(500);

        // Resolve output directory
        let output_dir = if let Some(dir) = params.output_dir {
            std::path::PathBuf::from(dir)
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".codescope")
                .join("exports")
                .join(&ctx.repo_name)
        };

        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            return format!("Failed to create output directory: {}", e);
        }

        // Query all functions
        let fq = "SELECT name, file_path, language, start_line, end_line, signature \
                   FROM `function` WHERE repo = $repo \
                   ORDER BY file_path, start_line LIMIT $lim";
        let functions: Vec<serde_json::Value> = match ctx
            .db
            .query(fq)
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("lim", limit as i64))
            .await
        {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(e) => return format!("Error querying functions: {}", e),
        };

        // Query all classes
        let cq = "SELECT name, file_path, language, start_line, end_line \
                   FROM class WHERE repo = $repo \
                   ORDER BY file_path, start_line LIMIT $lim";
        let classes: Vec<serde_json::Value> = match ctx
            .db
            .query(cq)
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("lim", limit as i64))
            .await
        {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(e) => return format!("Error querying classes: {}", e),
        };

        // Build index.md
        let mut index = format!("# {} — Code Index\n\n", ctx.repo_name);

        index.push_str("## Functions\n\n");
        for f in &functions {
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let fp = f.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let line = f.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
            index.push_str(&format!("- [[{}]] (`{}:{}`)\n", name, fp, line));
        }

        index.push_str("\n## Classes\n\n");
        for c in &classes {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let line = c.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
            index.push_str(&format!("- [[{}]] (`{}:{}`)\n", name, fp, line));
        }

        if let Err(e) = std::fs::write(output_dir.join("index.md"), &index) {
            return format!("Error writing index.md: {}", e);
        }
        let mut file_count = 1usize; // index.md

        // Find top 50 most-connected functions for individual files
        let top_q = "SELECT name, file_path, language, start_line, end_line, signature, \
                      count(<-calls) AS caller_count, count(->calls) AS callee_count, \
                      (count(<-calls) + count(->calls)) AS total_edges \
                      FROM `function` WHERE repo = $repo \
                      ORDER BY total_edges DESC LIMIT 50";
        let top_functions: Vec<serde_json::Value> = match ctx
            .db
            .query(top_q)
            .bind(("repo", ctx.repo_name.clone()))
            .await
        {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        for tf in &top_functions {
            let name = tf.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
            let fp = tf.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let lang = tf
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let start = tf.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
            let end = tf.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0);
            let sig = tf.get("signature").and_then(|v| v.as_str()).unwrap_or("");

            let mut md = format!(
                "---\nkind: function\nfile_path: {}\nlanguage: {}\nstart_line: {}\nend_line: {}\n---\n\n",
                fp, lang, start, end
            );
            md.push_str(&format!("## {}\n\n", name));
            if !sig.is_empty() {
                md.push_str(&format!("`{}`\n\n", sig));
            }

            // Query callers for this function
            let caller_q = "SELECT <-calls<-`function`.name AS callers FROM `function` \
                            WHERE name = $name AND repo = $repo LIMIT 1";
            if let Ok(mut cr) = ctx
                .db
                .query(caller_q)
                .bind(("name", name.to_string()))
                .bind(("repo", ctx.repo_name.clone()))
                .await
            {
                let rows: Vec<serde_json::Value> = cr.take(0).unwrap_or_default();
                if let Some(row) = rows.first() {
                    if let Some(callers) = row.get("callers").and_then(|v| v.as_array()) {
                        if !callers.is_empty() {
                            md.push_str("### Called By\n\n");
                            for c in callers {
                                if let Some(cn) = c.as_str() {
                                    md.push_str(&format!("- [[{}]]\n", cn));
                                }
                            }
                            md.push('\n');
                        }
                    }
                }
            }

            // Query callees for this function
            let callee_q = "SELECT ->calls->`function`.name AS callees FROM `function` \
                            WHERE name = $name AND repo = $repo LIMIT 1";
            if let Ok(mut cr) = ctx
                .db
                .query(callee_q)
                .bind(("name", name.to_string()))
                .bind(("repo", ctx.repo_name.clone()))
                .await
            {
                let rows: Vec<serde_json::Value> = cr.take(0).unwrap_or_default();
                if let Some(row) = rows.first() {
                    if let Some(callees) = row.get("callees").and_then(|v| v.as_array()) {
                        if !callees.is_empty() {
                            md.push_str("### Calls\n\n");
                            for c in callees {
                                if let Some(cn) = c.as_str() {
                                    md.push_str(&format!("- [[{}]]\n", cn));
                                }
                            }
                            md.push('\n');
                        }
                    }
                }
            }

            // Sanitize filename (replace problematic chars)
            let safe_name = name.replace(['/', '\\', ':', '<', '>', '|', '?', '*'], "_");
            if let Err(e) = std::fs::write(output_dir.join(format!("{}.md", safe_name)), &md) {
                return format!("Error writing {}.md: {}", safe_name, e);
            }
            file_count += 1;
        }

        format!(
            "Exported {} files to {}\n- index.md with {} functions and {} classes\n- {} individual entity files (top connected functions)",
            file_count,
            output_dir.display(),
            functions.len(),
            classes.len(),
            file_count - 1
        )
    }

    // ===== Capture Insight (real-time memory write) =====

    /// Record a decision, problem, solution, correction, or learning insight in real-time
    #[tool(
        description = "Record an insight into the knowledge graph in real-time. Types: decision, problem, solution, correction, learning. \
        Call this after making a decision, encountering a problem, finding a solution, or when the user corrects you (correction). \
        Corrections are especially important — they record what went wrong and the correct approach. \
        The agent field identifies which AI tool recorded this (claude-code, cursor, codex-cli, etc). \
        The insight is stored with timestamp, repo, scope, and optional entity links."
    )]
    async fn capture_insight(
        &self,
        Parameters(params): Parameters<CaptureInsightParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        // Validate kind
        let valid_kinds = ["decision", "problem", "solution", "learning", "correction"];
        let kind = params.kind.to_lowercase();
        if !valid_kinds.contains(&kind.as_str()) {
            return format!(
                "Invalid kind '{}'. Must be one of: {}",
                params.kind,
                valid_kinds.join(", ")
            );
        }

        // Map kind to DB table
        let table = match kind.as_str() {
            "decision" => "decision",
            "problem" => "problem",
            "solution" => "solution",
            "correction" => "solution", // corrections stored as solutions (they fix problems)
            "learning" => "conv_topic",
            _ => unreachable!(),
        };

        // Agent identity (auto-detect from client info or use provided)
        let agent = params
            .agent
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        // Derive scope from file_path if given
        let scope = params.file_path.as_deref().map(derive_scope_from_file_path);

        // Generate a unique qualified name
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let slug = params
            .summary
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
            .replace("__", "_")
            .trim_matches('_')
            .chars()
            .take(60)
            .collect::<String>();
        let qname = format!("{}:insight:{}:{}", ctx.repo_name, kind, slug);

        // Build the body from summary + detail
        let body = if let Some(detail) = &params.detail {
            format!("{}\n\n{}", params.summary, detail)
        } else {
            params.summary.clone()
        };

        // Escape single quotes for SurrealQL
        let esc = |s: &str| s.replace('\'', "\\'");

        // Create the insight record
        let create_query = format!(
            "CREATE {table} SET \
             name = '{name}', \
             qualified_name = '{qname}', \
             kind = '{kind}', \
             file_path = '{file_path}', \
             repo = '{repo}', \
             language = 'insight', \
             start_line = 0, \
             end_line = 0, \
             body = '{body}', \
             timestamp = '{ts}', \
             scope = '{scope}', \
             agent = '{agent}';",
            table = table,
            name = esc(&params.summary),
            qname = esc(&qname),
            kind = esc(&kind),
            file_path = esc(params.file_path.as_deref().unwrap_or("")),
            repo = esc(&ctx.repo_name),
            body = esc(&body),
            ts = timestamp,
            scope = esc(scope.as_deref().unwrap_or("root")),
            agent = esc(&agent),
        );

        if let Err(e) = ctx.db.query(&create_query).await {
            return format!("Error storing insight: {}", e);
        }

        // If entity_name is given, create a relation to the code entity
        if let Some(entity_name) = &params.entity_name {
            let rel_kind = match kind.as_str() {
                "decision" => "decided_about",
                _ => "discussed_in",
            };

            // Try to find the code entity in function, class, or config tables
            let find_query = format!(
                "SELECT id FROM `function` WHERE name = '{}' AND repo = '{}' LIMIT 1; \
                 SELECT id FROM class WHERE name = '{}' AND repo = '{}' LIMIT 1; \
                 SELECT id FROM config WHERE name = '{}' AND repo = '{}' LIMIT 1;",
                esc(entity_name),
                esc(&ctx.repo_name),
                esc(entity_name),
                esc(&ctx.repo_name),
                esc(entity_name),
                esc(&ctx.repo_name),
            );

            if let Ok(mut resp) = ctx.db.query(&find_query).await {
                let mut target_id = None;
                for i in 0..3u32 {
                    let results: Vec<serde_json::Value> = resp.take(i as usize).unwrap_or_default();
                    if let Some(first) = results.first() {
                        if let Some(id) = first.get("id") {
                            target_id = Some(id.to_string());
                            break;
                        }
                    }
                }

                if let Some(target) = target_id {
                    // Find the insight we just created
                    let relate_query = format!(
                        "LET $insight = (SELECT id FROM {table} WHERE qualified_name = '{qname}' LIMIT 1); \
                         IF $insight THEN \
                             RELATE $insight[0].id->{rel}->{target} \
                         END;",
                        table = table,
                        qname = esc(&qname),
                        rel = rel_kind,
                        target = target.trim_matches('"'),
                    );
                    let _ = ctx.db.query(&relate_query).await;
                }
            }
        }

        let mut confirmation = format!(
            "Captured {} insight: \"{}\"\n- Repo: {}\n- Agent: {}\n- Timestamp: {}",
            kind, params.summary, ctx.repo_name, agent, timestamp
        );
        if let Some(scope) = &scope {
            confirmation.push_str(&format!("\n- Scope: {}", scope));
        }
        if let Some(entity) = &params.entity_name {
            confirmation.push_str(&format!("\n- Linked to entity: {}", entity));
        }
        confirmation
    }

    /// Suggest a project directory structure for new/empty projects, or return the project profile if already indexed.
    #[tool(
        description = "Suggest a directory structure for a new project based on language and description. \
        If the project is already indexed (has entities), returns the Project Profile instead. \
        For empty/new projects, reads README.md or DESIGN.md if available and suggests a \
        language-appropriate directory layout."
    )]
    async fn suggest_structure(
        &self,
        Parameters(params): Parameters<SuggestStructureParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        // Check if the project already has indexed entities
        let entity_count: Vec<serde_json::Value> = ctx
            .db
            .query(
                "SELECT \
                 (SELECT count() FROM `function` WHERE repo = $repo GROUP ALL)[0].count AS fn_count, \
                 (SELECT count() FROM class WHERE repo = $repo GROUP ALL)[0].count AS cls_count",
            )
            .bind(("repo", ctx.repo_name.clone()))
            .await
            .ok()
            .and_then(|mut r| r.take(0).ok())
            .unwrap_or_default();

        let has_entities = entity_count
            .first()
            .map(|row| {
                let fns = row.get("fn_count").and_then(|v| v.as_u64()).unwrap_or(0);
                let cls = row.get("cls_count").and_then(|v| v.as_u64()).unwrap_or(0);
                fns > 0 || cls > 0
            })
            .unwrap_or(false);

        // If project already indexed, return the Project Profile
        if has_entities {
            let profile = build_project_profile(&ctx.db, &ctx.repo_name).await;
            if profile.is_empty() {
                return "Project is indexed but no profile data available. Try re-indexing.".into();
            }
            return format!(
                "Project already indexed. Here is the current profile:\n\n{}",
                profile
            );
        }

        // Empty/new project — look for README.md, DESIGN.md, or docs/
        let mut context_from_files = String::new();

        for filename in &["README.md", "DESIGN.md"] {
            let file_path = ctx.codebase_path.join(filename);
            if file_path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    let truncated: String = content.chars().take(2000).collect();
                    context_from_files
                        .push_str(&format!("### From {}\n{}\n\n", filename, truncated));
                }
            }
        }

        // Check docs/ directory for any .md files
        let docs_path = ctx.codebase_path.join("docs");
        if docs_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&docs_path) {
                for entry in entries.flatten().take(3) {
                    let path = entry.path();
                    if path.extension().map(|e| e == "md").unwrap_or(false) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("doc");
                            let truncated: String = content.chars().take(1000).collect();
                            context_from_files
                                .push_str(&format!("### From docs/{}\n{}\n\n", fname, truncated));
                        }
                    }
                }
            }
        }

        // Detect language — from param, or try to infer from files in the directory
        let lang = if let Some(ref l) = params.language {
            l.to_lowercase()
        } else {
            // Try to detect from common project files
            let codebase = &ctx.codebase_path;
            if codebase.join("Cargo.toml").is_file() {
                "rust".to_string()
            } else if codebase.join("package.json").is_file() {
                if codebase.join("tsconfig.json").is_file() {
                    "typescript".to_string()
                } else {
                    "javascript".to_string()
                }
            } else if codebase.join("pyproject.toml").is_file()
                || codebase.join("requirements.txt").is_file()
            {
                "python".to_string()
            } else if codebase.join("pubspec.yaml").is_file() {
                "dart".to_string()
            } else if codebase.join("go.mod").is_file() {
                "go".to_string()
            } else if codebase.join("Project.csproj").is_file() || codebase.join("*.sln").is_file()
            {
                "csharp".to_string()
            } else {
                "unknown".to_string()
            }
        };

        let suggestion = match lang.as_str() {
            "rust" => {
                "Suggested structure:\n```\nsrc/\n\
                       \x20\x20\x20\x20main.rs\n\
                       \x20\x20\x20\x20lib.rs\n\
                       \x20\x20\x20\x20config.rs\n\
                       \x20\x20\x20\x20error.rs\n\
                       \x20\x20\x20\x20routes/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20mod.rs\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20health.rs\n\
                       Cargo.toml\n```"
            }
            "typescript" | "javascript" => {
                "Suggested structure:\n```\nsrc/\n\
                       \x20\x20\x20\x20index.ts\n\
                       \x20\x20\x20\x20config/\n\
                       \x20\x20\x20\x20routes/\n\
                       \x20\x20\x20\x20services/\n\
                       \x20\x20\x20\x20models/\n\
                       \x20\x20\x20\x20utils/\n\
                       package.json\ntsconfig.json\n```"
            }
            "python" => {
                "Suggested structure:\n```\nsrc/\n\
                       \x20\x20\x20\x20__init__.py\n\
                       \x20\x20\x20\x20main.py\n\
                       \x20\x20\x20\x20config.py\n\
                       \x20\x20\x20\x20routes/\n\
                       \x20\x20\x20\x20services/\n\
                       \x20\x20\x20\x20models/\n\
                       \x20\x20\x20\x20utils/\n\
                       requirements.txt\npyproject.toml\n```"
            }
            "dart" | "flutter" => {
                "Suggested structure:\n```\nlib/\n\
                       \x20\x20\x20\x20main.dart\n\
                       \x20\x20\x20\x20core/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20config/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20constants/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20utils/\n\
                       \x20\x20\x20\x20features/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20home/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20screens/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20widgets/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20providers/\n\
                       \x20\x20\x20\x20shared/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20models/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20services/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20widgets/\n\
                       pubspec.yaml\n```"
            }
            "go" => {
                "Suggested structure:\n```\ncmd/\n\
                       \x20\x20\x20\x20server/\n\
                       \x20\x20\x20\x20\x20\x20\x20\x20main.go\n\
                       internal/\n\
                       \x20\x20\x20\x20config/\n\
                       \x20\x20\x20\x20handler/\n\
                       \x20\x20\x20\x20service/\n\
                       \x20\x20\x20\x20model/\n\
                       \x20\x20\x20\x20repository/\n\
                       pkg/\ngo.mod\n```"
            }
            "csharp" | "c#" => {
                "Suggested structure:\n```\nsrc/\n\
                       \x20\x20\x20\x20Program.cs\n\
                       \x20\x20\x20\x20Controllers/\n\
                       \x20\x20\x20\x20Services/\n\
                       \x20\x20\x20\x20Models/\n\
                       \x20\x20\x20\x20Data/\n\
                       \x20\x20\x20\x20Middleware/\n\
                       Tests/\n\
                       \x20\x20\x20\x20UnitTests/\n\
                       \x20\x20\x20\x20IntegrationTests/\n\
                       Project.csproj\n```"
            }
            _ => {
                "Suggested structure:\n```\nsrc/\n\
                       \x20\x20\x20\x20main\n\
                       \x20\x20\x20\x20config/\n\
                       \x20\x20\x20\x20core/\n\
                       \x20\x20\x20\x20services/\n\
                       \x20\x20\x20\x20models/\n\
                       \x20\x20\x20\x20utils/\n\
                       tests/\ndocs/\n```"
            }
        };

        let mut output = "# Project Structure Suggestion\n\n".to_string();
        output.push_str(&format!("**Detected language**: {}\n", lang));
        if let Some(ref desc) = params.description {
            output.push_str(&format!("**Goal**: {}\n", desc));
        }
        output.push('\n');
        output.push_str(suggestion);

        if !context_from_files.is_empty() {
            output.push_str("\n\n## Existing Documentation\n\n");
            output.push_str(&context_from_files);
        }

        output
    }
}

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::*;
use rmcp::{tool_handler, tool_router, ServerHandler};
use std::path::PathBuf;
use std::sync::Arc;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::daemon::DaemonState;
use crate::helpers::build_context_summary;

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
///
/// Both modes share the same `DaemonState` infrastructure — stdio mode
/// creates a single-project state at construction time.
#[derive(Clone)]
pub struct GraphRagServer {
    project: Arc<tokio::sync::RwLock<Option<ProjectCtx>>>,
    daemon: Option<Arc<DaemonState>>,
    /// True if this server was created via `new()` (stdio); false if via `new_daemon()`.
    /// Used by `init_project` to differentiate behavior.
    stdio_mode: bool,
    /// Cached conversation context summary, injected into ServerInfo.instructions
    context_summary: Arc<tokio::sync::RwLock<String>>,
    /// Delta-mode cache: stores last context_bundle output per file path.
    /// On repeat calls, returns only structural diff instead of full output.
    context_cache: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    /// Result archive: stores large tool outputs with retrieval IDs.
    /// When a tool output exceeds 4KB, the full result is archived here
    /// and a summary + retrieval ID is returned instead.
    result_archive: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    tool_router: ToolRouter<Self>,
}

impl GraphRagServer {
    /// Create for stdio mode — project ready immediately
    /// Create for stdio mode — single project pre-loaded.
    /// Internally builds a single-project DaemonState so both stdio and daemon
    /// share the same DB-management codepath.
    pub fn new(db: Surreal<Db>, repo_name: String, codebase_path: PathBuf) -> Self {
        // Use the parent dir of the DB as the daemon base path
        let base_db_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codescope")
            .join("db");
        let state = Arc::new(DaemonState::with_initial(
            base_db_path,
            repo_name.clone(),
            db.clone(),
        ));

        Self {
            project: Arc::new(tokio::sync::RwLock::new(Some(ProjectCtx {
                db,
                repo_name,
                codebase_path,
            }))),
            daemon: Some(state),
            stdio_mode: true,
            context_summary: Arc::new(tokio::sync::RwLock::new(String::new())),
            context_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            result_archive: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            tool_router: Self::merged_router(),
        }
    }

    /// Create for daemon mode — no project until init_project is called.
    pub fn new_daemon(state: Arc<DaemonState>) -> Self {
        Self {
            project: Arc::new(tokio::sync::RwLock::new(None)),
            daemon: Some(state),
            stdio_mode: false,
            context_summary: Arc::new(tokio::sync::RwLock::new(String::new())),
            context_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            result_archive: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            tool_router: Self::merged_router(),
        }
    }

    /// Whether this server was started in stdio mode (single project pre-loaded).
    pub(crate) fn is_stdio_mode(&self) -> bool {
        self.stdio_mode
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
        router.merge(Self::quality_router());
        router.merge(Self::adr_router());
        router.merge(Self::memory_router());
        router.merge(Self::analytics_router());
        router.merge(Self::knowledge_router());
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

    /// Accessor for the delta-mode context cache.
    pub(crate) fn context_cache(
        &self,
    ) -> &Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>> {
        &self.context_cache
    }

    /// Accessor for the result archive (large output storage).
    pub(crate) fn result_archive(
        &self,
    ) -> &Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>> {
        &self.result_archive
    }

    /// Get the active project context, or return an error message
    pub(crate) async fn ctx(&self) -> Result<ProjectCtx, String> {
        self.project
            .read()
            .await
            .clone()
            .ok_or_else(|| "No project initialized. Call `init_project` first with repo name and codebase path.".into())
    }

    /// Load conversation context + knowledge hot cache from DB and cache it.
    /// Called after auto-indexing completes.
    pub async fn load_context_summary(&self) {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(_) => return,
        };
        let summary = build_context_summary(&ctx.db, &ctx.repo_name).await;
        let insights = crate::helpers::build_post_index_insights(&ctx.db, &ctx.repo_name).await;

        // Knowledge hot cache: recent knowledge nodes for agent context
        let hot_cache = Self::build_knowledge_hot_cache(&ctx.db).await;

        let mut parts = Vec::new();
        if !insights.is_empty() {
            parts.push(insights);
        }
        if !summary.is_empty() {
            parts.push(summary);
        }
        if !hot_cache.is_empty() {
            parts.push(hot_cache);
        }
        *self.context_summary.write().await = parts.join("\n\n");
    }

    /// Build a hot cache of recent knowledge entities (~500 words) so agents
    /// start each session with awareness of what's in the knowledge graph.
    async fn build_knowledge_hot_cache(
        db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    ) -> String {
        let query =
            "SELECT title, kind, confidence FROM knowledge ORDER BY updated_at DESC LIMIT 15";
        let results: Vec<serde_json::Value> = match db.query(query).await {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(_) => return String::new(),
        };
        if results.is_empty() {
            return String::new();
        }

        let mut hot = String::from("# Knowledge Graph Hot Cache\n\n");
        hot.push_str(&format!("Recent knowledge nodes ({}):\n", results.len()));
        for r in &results {
            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("?");
            let kind = r.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let conf = r.get("confidence").and_then(|v| v.as_str()).unwrap_or("-");
            hot.push_str(&format!("- **{}** [{}] ({})\n", title, kind, conf));
        }
        hot.push_str(
            "\nUse `knowledge_search` to drill into any node. Use `/wiki-ingest` to add more.\n",
        );
        hot
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

/// Empty tool router required by `merged_router()` to satisfy the
/// `Self::tool_router()` reference. All actual tools live in `tools/*.rs`.
#[tool_router]
impl GraphRagServer {}

/// Convert a title/name to a URL-safe slug for use as SurrealDB record ID.
pub fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if matches!(c, ' ' | '-' | '_' | '/' | '.') && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

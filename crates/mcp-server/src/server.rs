use codescope_core::DbHandle;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::*;
use rmcp::{tool_handler, tool_router, ServerHandler};
use std::path::PathBuf;
use std::sync::Arc;

use crate::daemon::DaemonState;
use crate::helpers::build_context_summary;
use crate::index_state::IndexState;

/// Active project context — DB handle + metadata
#[derive(Clone)]
pub struct ProjectCtx {
    pub db: DbHandle,
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
    /// Path-routed mode: a repo name the session is pre-bound to. Set when
    /// the daemon's axum layer mounts the HTTP service at `/mcp/{repo}`.
    /// `ctx()` lazily resolves it to a full [`ProjectCtx`] on first tool
    /// call — we can't block on `daemon.get_db()` inside the synchronous
    /// rmcp factory closure. Cleared by `init_project` to keep
    /// switch-project semantics on an already-pinned session.
    pending_repo: Arc<tokio::sync::RwLock<Option<String>>>,
    /// Cached conversation context summary, injected into ServerInfo.instructions
    context_summary: Arc<tokio::sync::RwLock<String>>,
    /// Delta-mode cache: stores last context_bundle output per file path.
    /// On repeat calls, returns only structural diff instead of full output.
    context_cache: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    /// Result archive: stores large tool outputs with retrieval IDs.
    /// When a tool output exceeds 4KB, the full result is archived here
    /// and a summary + retrieval ID is returned instead.
    result_archive: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    /// Indexing state for readiness gating. Tool handlers consult this
    /// BEFORE running their DB queries — if the index is mid-build or
    /// has failed, the handler returns a structured JSON response
    /// instead of an empty result array.
    index_state: IndexState,
    tool_router: ToolRouter<Self>,
}

impl GraphRagServer {
    /// Create for stdio mode — project ready immediately
    /// Create for stdio mode — single project pre-loaded.
    /// Internally builds a single-project DaemonState so both stdio and daemon
    /// share the same DB-management codepath.
    pub fn new(db: DbHandle, repo_name: String, codebase_path: PathBuf) -> Self {
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
            pending_repo: Arc::new(tokio::sync::RwLock::new(None)),
            context_summary: Arc::new(tokio::sync::RwLock::new(String::new())),
            context_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            result_archive: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            index_state: IndexState::new(),
            tool_router: Self::merged_router(),
        }
    }

    /// Create for daemon mode — no project until init_project is called.
    pub fn new_daemon(state: Arc<DaemonState>) -> Self {
        Self {
            project: Arc::new(tokio::sync::RwLock::new(None)),
            daemon: Some(state),
            stdio_mode: false,
            pending_repo: Arc::new(tokio::sync::RwLock::new(None)),
            context_summary: Arc::new(tokio::sync::RwLock::new(String::new())),
            context_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            result_archive: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            index_state: IndexState::new(),
            tool_router: Self::merged_router(),
        }
    }

    /// Create for daemon mode, pre-bound to a repo via path routing
    /// (`/mcp/{repo}`). No DB open yet — the first tool call resolves it
    /// through [`DaemonState::get_db`] via [`Self::ctx`]. This keeps the
    /// rmcp session factory synchronous while still giving each mounted
    /// route a stable repo identity without requiring `init_project`.
    pub fn new_daemon_for_repo(state: Arc<DaemonState>, repo: String) -> Self {
        Self {
            project: Arc::new(tokio::sync::RwLock::new(None)),
            daemon: Some(state),
            stdio_mode: false,
            pending_repo: Arc::new(tokio::sync::RwLock::new(Some(repo))),
            context_summary: Arc::new(tokio::sync::RwLock::new(String::new())),
            context_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            result_archive: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            index_state: IndexState::new(),
            tool_router: Self::merged_router(),
        }
    }

    /// Shared indexing state — used by the pipeline to write progress
    /// and by tool handlers to read it for the readiness gate.
    pub fn index_state(&self) -> &IndexState {
        &self.index_state
    }

    /// Readiness-gate helper for tool handlers. If indexing is in
    /// progress or has failed, returns a structured response string.
    /// Otherwise returns `None` and the caller proceeds normally.
    ///
    /// Usage at the top of a handler:
    /// ```ignore
    /// if let Some(resp) = self.index_gate().await { return resp; }
    /// ```
    pub(crate) async fn index_gate(&self) -> Option<String> {
        self.index_state.gate().await
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

    /// Accessor for the pending-repo RwLock — used by admin tool
    /// sub-module to clear the path-routed default on explicit
    /// `init_project`.
    pub(crate) fn pending_repo_lock(&self) -> &Arc<tokio::sync::RwLock<Option<String>>> {
        &self.pending_repo
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

    /// Get the active project context, or return an error message.
    ///
    /// Errors come back as the R2 structured JSON shape; tool handlers
    /// forward the string verbatim, so Claude Code sees
    /// `{ok:false, error:{code, message, hint}}` and can act on
    /// `error.hint`.
    pub(crate) async fn ctx(&self) -> Result<ProjectCtx, String> {
        // Every tool call reaches here on its first awaited step,
        // so instrumenting once here catches 99% of the surface
        // without touching 52 individual tool handlers. The bump
        // is a single relaxed atomic add — effectively free.
        codescope_core::gain::record_call();
        if let Some(ctx) = self.project.read().await.clone() {
            // Insight event — fire-and-forget, no await.
            codescope_core::insight::record_event(&ctx.repo_name);
            return Ok(ctx);
        }
        let pending = self.pending_repo.read().await.clone();
        if let Some(repo) = pending {
            let daemon = self.daemon.clone().ok_or_else(|| {
                crate::error::tool_error(
                    crate::error::code::INTERNAL,
                    "Daemon state not available for pending repo resolve.",
                    None,
                )
            })?;
            let db = daemon.get_db(&repo).await.map_err(|e| {
                crate::error::tool_error(
                    crate::error::code::DB_UNREACHABLE,
                    &format!("Failed to open DB for pending repo '{repo}': {e}"),
                    Some("Is the surreal server up? Run `codescope start`."),
                )
            })?;
            let ctx = ProjectCtx {
                db,
                repo_name: repo.clone(),
                // Path-routed sessions don't know a filesystem codebase;
                // use CWD as a best-effort for tools that need a path.
                codebase_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            };
            codescope_core::insight::record_event(&ctx.repo_name);
            *self.project.write().await = Some(ctx.clone());
            return Ok(ctx);
        }
        Err(crate::error::tool_error(
            crate::error::code::NO_PROJECT,
            "No project initialized.",
            Some("Call `init_project` with { repo, codebase_path }, or use the `/mcp/{repo}` endpoint."),
        ))
    }

    /// Get the active project context *gated by indexing state*. If the
    /// background indexer is still running or has failed, returns the
    /// structured status JSON in the Err branch — existing handlers
    /// already forward the Err payload as the tool response, so this
    /// keeps the readiness gate a one-line change in each handler
    /// (`self.ctx()` → `self.gated_ctx()`).
    ///
    /// Admin tools that MUST remain callable during indexing (e.g.
    /// `index_status`, `project`, `index_codebase`) should keep using
    /// plain `ctx()`.
    pub(crate) async fn gated_ctx(&self) -> Result<ProjectCtx, String> {
        if let Some(gate_response) = self.index_gate().await {
            return Err(gate_response);
        }
        self.ctx().await
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
    async fn build_knowledge_hot_cache(db: &DbHandle) -> String {
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

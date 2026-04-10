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
        router.merge(Self::quality_router());
        router.merge(Self::adr_router());
        router.merge(Self::memory_router());
        router.merge(Self::analytics_router());
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

/// Empty tool router required by `merged_router()` to satisfy the
/// `Self::tool_router()` reference. All actual tools live in `tools/*.rs`.
#[tool_router]
impl GraphRagServer {}

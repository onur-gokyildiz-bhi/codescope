use rmcp::model::*;
use rmcp::{ServerHandler, tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use codescope_core::graph::query::GraphQuery;

use crate::daemon::DaemonState;

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
}

// Parameter structs for MCP tools

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchParams {
    /// The search query (function/class name or pattern)
    pub query: String,
    /// Maximum number of results (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FileEntitiesParams {
    /// Path to the file to inspect
    pub file_path: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindCallersParams {
    /// Name of the function to find callers for
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindCalleesParams {
    /// Name of the function to find callees for
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HttpCallParams {
    /// Filter by HTTP method (GET, POST, PUT, DELETE, PATCH). If not specified, returns all HTTP calls.
    pub method: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RawQueryParams {
    /// SurrealQL query to execute
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexParams {
    /// Path to index (relative to codebase root)
    pub path: Option<String>,
    /// Clear existing data before indexing
    pub clean: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ImpactAnalysisParams {
    /// Name of the function to analyze impact for
    pub function_name: String,
    /// Depth of the call graph to traverse (default: 3)
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NaturalLanguageQueryParams {
    /// Natural language question about the codebase
    pub question: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SyncHistoryParams {
    /// Path to the git repository
    pub git_path: Option<String>,
    /// Number of recent commits to sync (default: 200)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HotspotParams {
    /// Minimum risk score threshold (default: 0)
    pub min_score: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChurnParams {
    /// Number of top churned files to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CouplingParams {
    /// Number of top coupled file pairs to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DiffReviewParams {
    /// Git ref to diff against (e.g., "main", "HEAD~3", commit hash)
    pub base_ref: String,
    /// Optional head ref (default: HEAD)
    pub head_ref: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct InitProjectParams {
    /// Repository/project name (used for DB isolation)
    pub repo: String,
    /// Path to the codebase directory
    pub path: String,
    /// Auto-index the codebase after initialization
    pub auto_index: Option<bool>,
}

// === Obsidian-like exploration tools ===

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExploreParams {
    /// Entity name to explore (function, class, config key, file path, etc.)
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ContextBundleParams {
    /// File path to get full context for
    pub file_path: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RelatedParams {
    /// Keyword to search across all entity types (code, config, docs, packages)
    pub keyword: String,
    /// Maximum results per type (default: 10)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BacklinksParams {
    /// Entity name to find backlinks for
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexConversationsParams {
    /// Path to Claude projects directory (auto-detects if not provided)
    pub project_dir: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConversationSearchParams {
    /// Search query — entity name, topic keyword, or concept
    pub query: String,
    /// Filter by type: "decision", "problem", "solution", "topic", or "all" (default)
    pub entity_type: Option<String>,
    /// Maximum results (default: 20)
    pub limit: Option<usize>,
}

// === Semantic search tools ===

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConversationTimelineParams {
    /// Entity name (function, class, file) to search conversation history for
    pub entity_name: String,
    /// Number of days to look back (default: 30)
    pub days_back: Option<u32>,
    /// Maximum results (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EmbedParams {
    /// Embedding provider: "fastembed" (default, local), "ollama", or "openai"
    pub provider: Option<String>,
    /// Batch size for embedding generation (default: 100)
    pub batch_size: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SemanticSearchParams {
    /// Natural language query to search for semantically similar code
    pub query: String,
    /// Maximum results (default: 10)
    pub limit: Option<usize>,
    /// Embedding provider: "fastembed" (default), "ollama", or "openai"
    pub provider: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RenameSymbolParams {
    /// Name of the symbol (function/class) to find all references for
    pub symbol_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SafeDeleteParams {
    /// Name of the symbol to check for safe deletion
    pub symbol_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeadCodeParams {
    /// Minimum function size in lines to include (default: 3, filters out trivial getters)
    pub min_lines: Option<u32>,
    /// Maximum results (default: 50)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TeamPatternsParams {
    /// Focus area: "imports", "naming", "structure", or "all" (default)
    pub focus: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditPreflightParams {
    /// File path being edited
    pub file_path: String,
    /// Name of the function/class being added or modified
    pub entity_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ManageAdrParams {
    /// Action: "list", "create", "get"
    pub action: String,
    /// ADR title (for create)
    pub title: Option<String>,
    /// ADR body/decision text (for create)
    pub body: Option<String>,
    /// ADR ID (for get)
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CommunityDetectionParams {
    /// Analysis type: "clusters", "bridges", "central", or "all" (default)
    pub analysis: Option<String>,
    /// Maximum results per analysis (default: 20)
    pub limit: Option<usize>,
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
        }
    }

    /// Create for daemon mode — no project until init_project is called
    pub fn new_daemon(state: Arc<DaemonState>) -> Self {
        Self {
            project: Arc::new(tokio::sync::RwLock::new(None)),
            daemon: Some(state),
            context_summary: Arc::new(tokio::sync::RwLock::new(String::new())),
        }
    }

    /// Get the active project context, or return an error message
    async fn ctx(&self) -> Result<ProjectCtx, String> {
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
        *self.context_summary.write().await = summary;
    }
}

#[tool(tool_box)]
impl ServerHandler for GraphRagServer {
    fn get_info(&self) -> ServerInfo {
        let base_instructions = "Code Graph RAG — Intelligent code knowledge graph. \
             Search, analyze, and query your codebase using a graph database. \
             Supports semantic search, call graph analysis, impact analysis, \
             conversation history, and Obsidian-like knowledge navigation.";

        // Try to include cached conversation context (non-blocking)
        let context = self.context_summary.try_read()
            .map(|c| c.clone())
            .unwrap_or_default();

        let instructions = if context.is_empty() {
            base_instructions.to_string()
        } else {
            format!("{}\n\n{}", base_instructions, context)
        };

        ServerInfo {
            instructions: Some(instructions.into()),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                ..Default::default()
            },
            ..ServerInfo::default()
        }
    }
}

#[tool(tool_box)]
impl GraphRagServer {
    /// Initialize a project for this session (daemon mode). Opens the DB and optionally indexes the codebase.
    #[tool(description = "Initialize a project for this session. Required in daemon mode before using other tools. Pass the repo name and codebase path.")]
    async fn init_project(&self, #[tool(aggr)] params: InitProjectParams) -> String {
        let daemon = match &self.daemon {
            Some(d) => d.clone(),
            None => {
                // Stdio mode — already initialized
                return "Project already initialized (stdio mode).".into();
            }
        };

        let db = match daemon.get_db(&params.repo).await {
            Ok(db) => db,
            Err(e) => return format!("Failed to open DB for '{}': {}", params.repo, e),
        };

        let codebase_path = PathBuf::from(&params.path);
        let repo_name = params.repo.clone();

        // Set the active project for this connection
        *self.project.write().await = Some(ProjectCtx {
            db: db.clone(),
            repo_name: repo_name.clone(),
            codebase_path: codebase_path.clone(),
        });

        // Auto-index in background with parallel parsing
        if params.auto_index.unwrap_or(false) {
            let index_repo = repo_name.clone();
            let index_path = codebase_path.clone();
            tokio::spawn(async move {
                tracing::info!("Background indexing {}...", index_path.display());
                let builder = codescope_core::graph::builder::GraphBuilder::new(db);

                // Parse files in parallel (CPU-bound, rayon thread pool)
                let parse_path = index_path.clone();
                let parse_repo = index_repo.clone();
                let results = tokio::task::spawn_blocking(move || {
                    use rayon::prelude::*;
                    let parser = codescope_core::parser::CodeParser::new();
                    let walker = ignore::WalkBuilder::new(&parse_path)
                        .hidden(true)
                        .git_ignore(true)
                        .build();

                    let files: Vec<std::path::PathBuf> = walker
                        .flatten()
                        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
                        .filter(|e| {
                            let fp = e.path();
                            let ext = fp.extension().and_then(|e| e.to_str()).unwrap_or("");
                            let fname = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            (parser.supports_extension(ext) || parser.supports_filename(fname))
                                && !codescope_core::parser::should_skip_file(fp)
                        })
                        .map(|e| e.into_path())
                        .collect();

                    tracing::info!("Found {} files to parse", files.len());

                    files
                        .par_iter()
                        .filter_map(|file_path| {
                            let rel_path = file_path
                                .strip_prefix(&parse_path)
                                .unwrap_or(file_path)
                                .to_string_lossy()
                                .to_string()
                                .replace('\\', "/");
                            let content = std::fs::read_to_string(file_path).ok()?;
                            parser
                                .parse_source(std::path::Path::new(&rel_path), &content, &parse_repo)
                                .ok()
                        })
                        .collect::<Vec<_>>()
                })
                .await
                .unwrap_or_default();

                // Batch insert results
                let mut file_count = 0;
                for (entities, relations) in results {
                    let _ = builder.insert_entities(&entities).await;
                    let _ = builder.insert_relations(&relations).await;
                    file_count += 1;
                }

                tracing::info!("Background indexing complete: {} files", file_count);
            });
        }

        format!("Project '{}' initialized at {}. DB ready.", repo_name, codebase_path.display())
    }

    /// List all projects currently open in the daemon
    #[tool(description = "List all projects currently open in the daemon. Only available in daemon mode.")]
    async fn list_projects(&self) -> String {
        match &self.daemon {
            Some(d) => {
                let repos = d.list_repos().await;
                if repos.is_empty() {
                    "No projects open. Call init_project first.".into()
                } else {
                    format!("Open projects: {}", repos.join(", "))
                }
            }
            None => {
                let ctx = self.project.read().await;
                match &*ctx {
                    Some(c) => format!("Stdio mode — project: {}", c.repo_name),
                    None => "No project initialized.".into(),
                }
            }
        }
    }

    /// Search for functions by name or pattern in the code graph
    #[tool(description = "Search for functions by name or pattern. Returns matching functions with file paths and line numbers.")]
    async fn search_functions(&self, #[tool(aggr)] params: SearchParams) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = params.limit.unwrap_or(20);
        let gq = GraphQuery::new(ctx.db);

        match gq.search_functions(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No functions found matching '{}'", params.query);
                }
                let mut output = format!("Found {} functions matching '{}':\n\n", results.len().min(limit), params.query);
                for (i, r) in results.iter().enumerate().take(limit) {
                    output.push_str(&format!(
                        "{}. **{}** ({}:{})\n",
                        i + 1,
                        r.name.as_deref().unwrap_or("?"),
                        r.file_path.as_deref().unwrap_or("?"),
                        r.start_line.unwrap_or(0),
                    ));
                    if let Some(sig) = &r.signature {
                        output.push_str(&format!("   `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Error searching: {}", e),
        }
    }

    /// Find a function by exact name
    #[tool(description = "Find a function by exact name. Returns detailed info including signature, file path, and line numbers.")]
    async fn find_function(&self, #[tool(aggr)] params: SearchParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_function(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No function found with name '{}'", params.query);
                }
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "Error formatting results".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// List all code entities (functions, classes) in a specific file
    #[tool(description = "List all functions and classes in a file. Provides an overview of the file's structure.")]
    async fn file_entities(&self, #[tool(aggr)] params: FileEntitiesParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.file_entities(&params.file_path).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No entities found in '{}'", params.file_path);
                }
                let mut output = format!("Entities in {}:\n\n", params.file_path);
                for r in &results {
                    output.push_str(&format!(
                        "- **{}** (L{}-{})\n",
                        r.name.as_deref().unwrap_or("?"),
                        r.start_line.unwrap_or(0),
                        r.end_line.unwrap_or(0),
                    ));
                    if let Some(sig) = &r.signature {
                        output.push_str(&format!("  `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find all functions that call the specified function (callers / incoming calls)
    #[tool(description = "Find all functions that call the specified function. Useful for understanding who depends on a function.")]
    async fn find_callers(&self, #[tool(aggr)] params: FindCallersParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_callers(&params.function_name).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No callers found for '{}'", params.function_name);
                }
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find all functions called by the specified function (callees / outgoing calls)
    #[tool(description = "Find all functions called by the specified function. Useful for understanding a function's dependencies.")]
    async fn find_callees(&self, #[tool(aggr)] params: FindCalleesParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_callees(&params.function_name).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No callees found for '{}'", params.function_name);
                }
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Get statistics about the indexed code graph
    #[tool(description = "Get statistics about the code graph: number of files, functions, classes, and relationships indexed.")]
    async fn graph_stats(&self) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.stats().await {
            Ok(stats) => {
                serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "Error formatting stats".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Execute a raw SurrealQL query against the code graph
    #[tool(description = "Execute a raw SurrealQL query against the code graph database. Use for advanced queries like graph traversals.")]
    async fn raw_query(&self, #[tool(aggr)] params: RawQueryParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.raw_query(&params.query).await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Query error: {}", e),
        }
    }

    /// Index or re-index the codebase into the graph database
    #[tool(description = "Index the codebase into the knowledge graph. Parses source files and extracts entities and relationships.")]
    async fn index_codebase(&self, #[tool(aggr)] params: IndexParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let target_path = match &params.path {
            Some(p) => ctx.codebase_path.join(p),
            None => ctx.codebase_path.clone(),
        };

        let parser = codescope_core::parser::CodeParser::new();
        let builder = codescope_core::graph::builder::GraphBuilder::new(ctx.db.clone());
        let incremental = codescope_core::graph::incremental::IncrementalIndexer::new(ctx.db.clone());

        let clean = params.clean.unwrap_or(false);
        if clean {
            if let Err(e) = builder.clear_repo(&ctx.repo_name).await {
                return format!("Error clearing repo: {}", e);
            }
        }

        // Load existing hashes in bulk for incremental comparison
        let existing_hashes = if !clean {
            incremental.load_file_hashes(&ctx.repo_name).await.unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        let walker = ignore::WalkBuilder::new(&target_path)
            .hidden(true)
            .git_ignore(true)
            .build();

        let mut files_indexed = 0;
        let mut files_skipped = 0;
        let mut entities = 0;
        let mut relations = 0;
        let mut errors = Vec::new();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }
            let file_path = entry.path();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let filename = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !parser.supports_extension(ext) && !parser.supports_filename(filename) {
                continue;
            }
            if codescope_core::parser::should_skip_file(file_path) {
                continue;
            }

            // Read file content for incremental hash check
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let rel_path = file_path
                .strip_prefix(&target_path)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string()
                .replace('\\', "/");

            // Skip unchanged files (unless clean rebuild)
            if !clean {
                let current_hash = codescope_core::graph::incremental::hash_content(&content);
                if existing_hashes.get(&rel_path).map(|h| h.as_str()) == Some(&current_hash) {
                    files_skipped += 1;
                    continue;
                }
                // File changed — delete old entities before re-inserting
                let _ = builder.delete_file_entities(&rel_path, &ctx.repo_name).await;
            }

            match parser.parse_source(std::path::Path::new(&rel_path), &content, &ctx.repo_name) {
                Ok((ents, rels)) => {
                    entities += ents.len();
                    relations += rels.len();
                    let _ = builder.insert_entities(&ents).await;
                    let _ = builder.insert_relations(&rels).await;
                    files_indexed += 1;
                }
                Err(e) => {
                    errors.push(format!("{}: {}", file_path.display(), e));
                }
            }
        }

        // Clean up entities from deleted files
        let deleted = if !clean {
            incremental.cleanup_deleted_files(&target_path, &ctx.repo_name).await.unwrap_or(0)
        } else {
            0
        };

        let mut output = format!(
            "Indexing complete!\n- Files indexed: {}\n- Files unchanged (skipped): {}\n- Entities: {}\n- Relations: {}",
            files_indexed, files_skipped, entities, relations
        );
        if deleted > 0 {
            output.push_str(&format!("\n- Deleted files cleaned: {}", deleted));
        }
        if !errors.is_empty() {
            output.push_str(&format!("\n- Errors: {}", errors.len()));
        }
        output
    }

    /// Analyze the impact of changing a function — what else could be affected
    #[tool(description = "Analyze the impact of changing a function. Shows the transitive call graph to understand what would be affected by a change.")]
    async fn impact_analysis(&self, #[tool(aggr)] params: ImpactAnalysisParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let _depth = params.depth.unwrap_or(3);

        // Direct edge traversal — much faster than subquery on large graphs (21K+ edges)
        let query = format!(
            "SELECT name, qualified_name, file_path, start_line FROM `function` WHERE name = $name;\
             SELECT in.name AS name, in.file_path AS file_path \
               FROM calls WHERE out.name = $name AND in.name != NONE;\
             SELECT in.name AS name, in.file_path AS file_path \
               FROM calls WHERE out.name IN \
                 (SELECT VALUE in.name FROM calls WHERE out.name = $name AND in.name != NONE) \
                 AND in.name != NONE AND in.name != $name;"
        );

        let name = params.function_name.clone();
        match ctx.db.query(query).bind(("name", name)).await {
            Ok(mut response) => {
                let func_info: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                let direct: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
                let indirect: Vec<serde_json::Value> = response.take(2).unwrap_or_default();

                let mut output = format!("## Impact Analysis: {}\n\n", params.function_name);

                if let Some(info) = func_info.first() {
                    output.push_str(&format!("**Location:** {}:{}\n\n",
                        info.get("file_path").and_then(|v| v.as_str()).unwrap_or("?"),
                        info.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0),
                    ));
                }

                output.push_str("### Direct Callers\n");
                if direct.is_empty() {
                    output.push_str("None found\n");
                } else {
                    output.push_str(&serde_json::to_string_pretty(&direct).unwrap_or_default());
                }

                output.push_str("\n\n### Indirect Callers (2 hops)\n");
                if indirect.is_empty() {
                    output.push_str("None found\n");
                } else {
                    output.push_str(&serde_json::to_string_pretty(&indirect).unwrap_or_default());
                }

                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    // ===== HTTP Cross-Service Linking =====

    /// Find HTTP client calls in the codebase
    #[tool(description = "Find all HTTP client calls (reqwest, fetch, axios, requests) in the codebase. Optionally filter by HTTP method (GET, POST, PUT, DELETE, PATCH). Shows which functions make HTTP requests and to which endpoints.")]
    async fn find_http_calls(&self, #[tool(aggr)] params: HttpCallParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_http_calls(params.method.as_deref()).await {
            Ok(results) => {
                if results.is_empty() {
                    return "No HTTP client calls found in the codebase.".into();
                }
                let mut output = format!("Found {} HTTP client calls:\n\n", results.len());
                for (i, r) in results.iter().enumerate() {
                    let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let file = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    let line = r.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                    output.push_str(&format!("{}. **{}** ({}:{})\n", i + 1, name, file, line));
                }
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find which functions call a specific HTTP endpoint
    #[tool(description = "Find all code functions that call a specific HTTP endpoint by URL pattern. Example: '/users' finds all code that makes HTTP requests to any /users endpoint. Shows the calling function, HTTP method, and location.")]
    async fn find_endpoint_callers(&self, #[tool(aggr)] params: SearchParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_endpoint_callers(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No functions found calling endpoint matching '{}'", params.query);
                }
                let mut output = format!("Found {} callers of endpoint '{}':\n\n", results.len(), params.query);
                for (i, r) in results.iter().enumerate() {
                    let caller = r.get("caller_name").and_then(|v| v.as_str()).unwrap_or("?");
                    let file = r.get("caller_file").and_then(|v| v.as_str()).unwrap_or("?");
                    let method = r.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                    let http_call = r.get("http_call").and_then(|v| v.as_str()).unwrap_or("?");
                    output.push_str(&format!(
                        "{}. **{}** ({}) calls {} {}\n",
                        i + 1, caller, file, method, http_call,
                    ));
                }
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    // ===== Symbol-Level Operations =====

    /// Find all references to a symbol for rename planning
    #[tool(description = "Find all references to a symbol (function/class) across the codebase. Shows definitions, call sites, and imports. Use this to plan a rename — it shows every location that would need to change.")]
    async fn rename_symbol(&self, #[tool(aggr)] params: RenameSymbolParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.find_all_references(&params.symbol_name).await {
            Ok(result) => {
                let total = result.get("total_references").and_then(|v| v.as_u64()).unwrap_or(0);
                if total == 0 {
                    return format!("No references found for symbol '{}'", params.symbol_name);
                }
                let mut output = format!("**Symbol: {}** — {} references found\n\n", params.symbol_name, total);

                if let Some(refs) = result.get("references").and_then(|v| v.as_array()) {
                    for r in refs {
                        let ref_type = r.get("ref_type").and_then(|v| v.as_str()).unwrap_or("?");
                        let file = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let line = r.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                        let name = r.get("name").or_else(|| r.get("caller_name"))
                            .and_then(|v| v.as_str()).unwrap_or("?");
                        output.push_str(&format!("- [{}] **{}** at {}:{}\n", ref_type, name, file, line));
                    }
                }

                output.push_str(&format!("\nTo rename '{}', all {} locations above need updating.", params.symbol_name, total));
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find unused symbols (functions with zero references)
    #[tool(description = "Find unused symbols — functions that are never called by any other function. Filters out known entry points (main, test functions, handlers, constructors). Useful for codebase cleanup and reducing maintenance burden.")]
    async fn find_unused(&self, #[tool(aggr)] params: DeadCodeParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);
        let min_lines = params.min_lines.unwrap_or(3);

        match gq.find_unused_symbols(min_lines).await {
            Ok(results) => {
                if results.is_empty() {
                    return "No unused symbols found (or all are entry points/trivial).".into();
                }
                let limit = params.limit.unwrap_or(50);
                let mut output = format!("Found {} potentially unused symbols (min {} lines):\n\n", results.len().min(limit), min_lines);
                for (i, r) in results.iter().enumerate().take(limit) {
                    let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let file = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    let lines = r.get("line_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let sig = r.get("signature").and_then(|v| v.as_str()).unwrap_or("");
                    output.push_str(&format!("{}. **{}** ({}, {} lines)\n", i + 1, name, file, lines));
                    if !sig.is_empty() {
                        output.push_str(&format!("   `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Check if a symbol can be safely deleted
    #[tool(description = "Check if a symbol (function/class) can be safely deleted. Returns whether it has zero callers and zero importers. If not safe, shows what still references it.")]
    async fn safe_delete(&self, #[tool(aggr)] params: SafeDeleteParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.safe_delete_check(&params.symbol_name).await {
            Ok(result) => {
                let safe = result.get("safe_to_delete").and_then(|v| v.as_bool()).unwrap_or(false);
                let callers = result.get("caller_count").and_then(|v| v.as_u64()).unwrap_or(0);
                let imports = result.get("import_count").and_then(|v| v.as_u64()).unwrap_or(0);

                if safe {
                    let mut output = format!("**{}** can be safely deleted. No callers or imports reference it.\n\n", params.symbol_name);
                    if let Some(defs) = result.get("definitions").and_then(|v| v.as_array()) {
                        output.push_str("Definitions to remove:\n");
                        for d in defs {
                            let file = d.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                            let line = d.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                            output.push_str(&format!("- {}:{}\n", file, line));
                        }
                    }
                    output
                } else {
                    format!(
                        "**{}** is NOT safe to delete.\n\n\
                         - {} callers still reference it\n\
                         - {} imports mention it\n\n\
                         Use `rename_symbol` to see all references.",
                        params.symbol_name, callers, imports
                    )
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// List all supported programming languages
    #[tool(description = "List all programming languages supported by the code graph parser.")]
    async fn supported_languages(&self) -> String {
        let parser = codescope_core::parser::CodeParser::new();
        let languages = parser.supported_languages();
        format!("Supported languages: {}", languages.join(", "))
    }

    /// Sync git commit history into the graph database for temporal analysis
    #[tool(description = "Sync git commit history into the graph database. Enables temporal queries like hotspot detection, change coupling, and code evolution tracking.")]
    async fn sync_git_history(&self, #[tool(aggr)] params: SyncHistoryParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let git_path = params.git_path
            .map(|p| ctx.codebase_path.join(p))
            .unwrap_or_else(|| ctx.codebase_path.clone());
        let limit = params.limit.unwrap_or(200);

        let commits = match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.recent_commits(limit)
        }).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => return format!("Error reading git history: {}", e),
            Err(e) => return format!("Task error: {}", e),
        };

        let sync = codescope_core::temporal::TemporalGraphSync::new(ctx.db);
        match sync.sync_commit_data(&commits, &ctx.repo_name).await {
            Ok(count) => format!("Synced {} commits into the graph database", count),
            Err(e) => format!("Error syncing commits: {}", e),
        }
    }

    /// Detect code hotspots — files/functions with high complexity AND high churn
    #[tool(description = "Detect code hotspots: functions with high complexity and high change frequency. These are high-risk areas that may need refactoring.")]
    async fn hotspot_detection(&self, #[tool(aggr)] params: HotspotParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let sync = codescope_core::temporal::TemporalGraphSync::new(ctx.db);
        match sync.calculate_hotspots(&ctx.repo_name).await {
            Ok(hotspots) => {
                if hotspots.is_empty() {
                    return "No hotspots found. Make sure to sync git history first with sync_git_history.".into();
                }
                let min_score = params.min_score.unwrap_or(0);
                let filtered: Vec<_> = hotspots.iter()
                    .filter(|h| h.risk_score.unwrap_or(0) >= min_score)
                    .collect();

                let mut output = format!("## Code Hotspots ({})\n\n", filtered.len());
                output.push_str("| Function | File | Size | Churn | Risk Score |\n");
                output.push_str("|----------|------|------|-------|------------|\n");
                for h in &filtered {
                    output.push_str(&format!(
                        "| {} | {} | {} | {} | {} |\n",
                        h.name.as_deref().unwrap_or("?"),
                        h.file_path.as_deref().unwrap_or("?"),
                        h.size.unwrap_or(0),
                        h.churn.unwrap_or(0),
                        h.risk_score.unwrap_or(0),
                    ));
                }
                output
            }
            Err(e) => format!("Error calculating hotspots: {}", e),
        }
    }

    /// Get file churn — most frequently changed files in git history
    #[tool(description = "Get the most frequently changed files in git history. High-churn files may indicate instability or active development areas.")]
    async fn file_churn(&self, #[tool(aggr)] params: ChurnParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let limit = params.limit.unwrap_or(20);
        let git_path = ctx.codebase_path.clone();

        match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.file_churn(limit)
        }).await {
            Ok(Ok(churn)) => {
                let mut output = "## File Churn (Most Changed Files)\n\n".to_string();
                output.push_str("| Changes | File |\n|---------|------|\n");
                for (file, count) in &churn {
                    output.push_str(&format!("| {} | {} |\n", count, file));
                }
                output
            }
            Ok(Err(e)) => format!("Error: {}", e),
            Err(e) => format!("Task error: {}", e),
        }
    }

    /// Get change coupling — files that are frequently changed together
    #[tool(description = "Find files that are frequently changed together in commits. High coupling suggests hidden dependencies or that files should be colocated.")]
    async fn change_coupling(&self, #[tool(aggr)] params: CouplingParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let limit = params.limit.unwrap_or(20);
        let git_path = ctx.codebase_path.clone();

        match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.change_coupling(limit)
        }).await {
            Ok(Ok(coupling)) => {
                let mut output = "## Change Coupling (Files Changed Together)\n\n".to_string();
                output.push_str("| Count | File A | File B |\n|-------|--------|--------|\n");
                for (a, b, count) in &coupling {
                    output.push_str(&format!("| {} | {} | {} |\n", count, a, b));
                }
                output
            }
            Ok(Err(e)) => format!("Error: {}", e),
            Err(e) => format!("Task error: {}", e),
        }
    }

    /// Review a git diff with graph context — analyze which functions and relationships are affected
    #[tool(description = "Review a git diff with graph context. Shows which functions, classes, and call relationships are affected by changes between two git refs.")]
    async fn review_diff(&self, #[tool(aggr)] params: DiffReviewParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let git_path = ctx.codebase_path.clone();
        let base_ref = params.base_ref.clone();
        let head_ref_str = params.head_ref.clone().unwrap_or_else(|| "HEAD".to_string());

        // Get changed files in blocking task
        let changed_files = match tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<(String, String)>> {
            let repo = git2::Repository::open(&git_path)?;
            let base = repo.revparse_single(&base_ref)?;
            let head = repo.revparse_single(&head_ref_str)?;
            let base_tree = base.peel_to_tree()?;
            let head_tree = head.peel_to_tree()?;
            let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;

            let mut files = Vec::new();
            diff.foreach(
                &mut |delta, _| {
                    let path = delta.new_file().path()
                        .or_else(|| delta.old_file().path())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let status = match delta.status() {
                        git2::Delta::Added => "added",
                        git2::Delta::Deleted => "deleted",
                        git2::Delta::Modified => "modified",
                        git2::Delta::Renamed => "renamed",
                        _ => "other",
                    };
                    files.push((path, status.to_string()));
                    true
                },
                None, None, None,
            )?;
            Ok(files)
        }).await {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => return format!("Error computing diff: {}", e),
            Err(e) => return format!("Task error: {}", e),
        };

        let gq = GraphQuery::new(ctx.db);
        let head_display = params.head_ref.as_deref().unwrap_or("HEAD");

        let mut output = format!(
            "## Diff Review: {} → {}\n\n**{} files changed**\n\n",
            params.base_ref, head_display, changed_files.len()
        );

        // Batch query: get ALL entities for ALL changed files in one DB call (not N+1)
        if !changed_files.is_empty() {
            let file_list = changed_files.iter()
                .map(|(fp, _)| format!("'{}'", fp.replace('\'', "\\'")))
                .collect::<Vec<_>>()
                .join(", ");
            let batch_query = format!(
                "SELECT name, file_path, start_line, end_line FROM `function` WHERE file_path IN [{}]; \
                 SELECT name, file_path, start_line, end_line FROM class WHERE file_path IN [{}];",
                file_list, file_list
            );

            let mut entities_by_file: std::collections::HashMap<String, Vec<(String, u32, u32)>> =
                std::collections::HashMap::with_capacity(changed_files.len());

            if let Ok(batch_result) = gq.raw_query(&batch_query).await {
                if let Some(arr) = batch_result.as_array() {
                    for stmt_result in arr {
                        if let Some(rows) = stmt_result.as_array() {
                            for row in rows {
                                let fp = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                                let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let sl = row.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                let el = row.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                entities_by_file.entry(fp.to_string()).or_default()
                                    .push((name.to_string(), sl, el));
                            }
                        }
                    }
                }
            }

            for (file_path, status) in &changed_files {
                output.push_str(&format!("### {} ({})\n", file_path, status));
                if let Some(entities) = entities_by_file.get(file_path.as_str()) {
                    for (name, sl, el) in entities {
                        output.push_str(&format!("  - **{}** (L{}-{})\n", name, sl, el));
                    }
                } else {
                    output.push_str("  (no indexed entities)\n");
                }
            }
        }

        output.push_str(&format!("\n---\n**Summary:** {} files affected.\n", changed_files.len()));
        output
    }

    /// Get contributor expertise map — who knows which parts of the codebase
    #[tool(description = "Get a contributor expertise map showing who has the most knowledge about which files. Useful for finding the right reviewer for a change.")]
    async fn contributor_map(&self) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let git_path = ctx.codebase_path.clone();

        match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.contributor_map()
        }).await {
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

    /// Suggest reviewers for changed files based on git history
    #[tool(description = "Suggest code reviewers for a set of changed files based on who has the most expertise with those files.")]
    async fn suggest_reviewers(&self, #[tool(aggr)] params: DiffReviewParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let git_path = ctx.codebase_path.clone();
        let base_ref = params.base_ref.clone();
        let head_ref_str = params.head_ref.clone().unwrap_or_else(|| "HEAD".to_string());

        // All git2 work in blocking task
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<String>, std::collections::HashMap<String, Vec<(String, usize)>>)> {
            let repo = git2::Repository::open(&git_path)?;
            let base = repo.revparse_single(&base_ref)?;
            let head = repo.revparse_single(&head_ref_str)?;
            let base_tree = base.peel_to_tree()?;
            let head_tree = head.peel_to_tree()?;
            let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;

            let mut changed_files = Vec::new();
            diff.foreach(
                &mut |delta, _| {
                    if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                        changed_files.push(path.to_string_lossy().to_string());
                    }
                    true
                },
                None, None, None,
            )?;

            let analyzer = codescope_core::temporal::GitAnalyzer::open(&repo.path().parent().unwrap_or(repo.path()))?;
            let contributor_map = analyzer.contributor_map()?;

            Ok((changed_files, contributor_map))
        }).await;

        let (changed_files, contributor_map) = match result {
            Ok(Ok((cf, cm))) => (cf, cm),
            Ok(Err(e)) => return format!("Error: {}", e),
            Err(e) => return format!("Task error: {}", e),
        };

        let mut reviewer_scores: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (author, files) in &contributor_map {
            for (file, count) in files {
                if changed_files.iter().any(|cf| file.contains(cf) || cf.contains(file)) {
                    *reviewer_scores.entry(author.clone()).or_insert(0) += count;
                }
            }
        }

        let mut reviewers: Vec<_> = reviewer_scores.into_iter().collect();
        reviewers.sort_by(|a, b| b.1.cmp(&a.1));

        let head_display = params.head_ref.as_deref().unwrap_or("HEAD");
        let mut output = format!(
            "## Suggested Reviewers for {} → {}\n\n**{} files changed**\n\n",
            params.base_ref, head_display, changed_files.len()
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

    /// Translate a natural language question to a SurrealQL query and execute it
    #[tool(description = "Ask a question about the codebase in natural language. Translates to a graph query and returns results. Examples: 'what functions are in main.rs?', 'find all structs', 'show call graph for parse_file'")]
    async fn ask(&self, #[tool(aggr)] params: NaturalLanguageQueryParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let question = params.question.to_lowercase();

        // Sanitize user input for safe SurrealQL interpolation
        let sanitize = |s: &str| -> String {
            s.replace('\'', "")
             .replace('\\', "")
             .replace(';', "")
             .replace("--", "")
        };

        // Pattern matching for common questions — all parameterized where possible
        let (surql, binds): (String, Vec<(&str, String)>) =
            if question.contains("how many") && question.contains("file") {
                ("SELECT count() FROM file GROUP ALL".into(), vec![])
            } else if question.contains("how many") && question.contains("function") {
                ("SELECT count() FROM `function` GROUP ALL".into(), vec![])
            } else if question.contains("how many") && question.contains("class") {
                ("SELECT count() FROM class GROUP ALL".into(), vec![])
            } else if question.contains("all function") || question.contains("list function") {
                ("SELECT name, file_path, start_line FROM `function` ORDER BY name LIMIT 50".into(), vec![])
            } else if question.contains("all class") || question.contains("list class") || question.contains("all struct") || question.contains("list struct") {
                ("SELECT name, kind, file_path, start_line FROM class ORDER BY name LIMIT 50".into(), vec![])
            } else if question.contains("all file") || question.contains("list file") {
                ("SELECT path, language, line_count FROM file ORDER BY path LIMIT 50".into(), vec![])
            } else if question.contains("call graph") || question.contains("calls") {
                let words: Vec<&str> = question.split_whitespace().collect();
                if let Some(idx) = words.iter().position(|w| *w == "for" || *w == "of") {
                    let func_name = sanitize(&words[idx + 1..].join(" ").trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_string());
                    ("SELECT ->calls->`function`.name AS calls FROM `function` WHERE name = $name".into(),
                     vec![("name", func_name)])
                } else {
                    ("SELECT *, ->calls->`function`.name AS calls FROM `function` WHERE array::len(->calls) > 0 LIMIT 20".into(), vec![])
                }
            } else if question.contains("in file") || question.contains("in ") && question.contains(".rs") || question.contains(".ts") || question.contains(".py") {
                let path = sanitize(&extract_path_from_question(&question));
                ("SELECT name, qualified_name, start_line, end_line FROM `function` WHERE file_path CONTAINS $path \
                  UNION \
                  SELECT name, qualified_name, start_line, end_line FROM class WHERE file_path CONTAINS $path".into(),
                 vec![("path", path)])
            } else if question.contains("largest") || question.contains("biggest") || question.contains("longest") {
                ("SELECT name, file_path, start_line, end_line, (end_line - start_line) AS size FROM `function` ORDER BY size DESC LIMIT 10".into(), vec![])
            } else if question.contains("import") {
                ("SELECT name, file_path FROM import_decl ORDER BY file_path LIMIT 50".into(), vec![])
            } else {
                let search_term = question.split_whitespace()
                    .filter(|w| w.len() > 3 && !["what", "where", "which", "find", "show", "list", "does", "that", "this", "from", "with"].contains(w))
                    .next()
                    .unwrap_or(&question);
                let safe_term = sanitize(search_term);
                ("SELECT name, file_path, start_line, signature FROM `function` WHERE name ~ $term LIMIT 20".into(),
                 vec![("term", safe_term)])
            };

        // Build parameterized query
        let mut query = ctx.db.query(&surql);
        for (key, val) in &binds {
            query = query.bind((*key, val.clone()));
        }

        match query.await {
            Ok(mut response) => {
                let result: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                let mut output = format!("**Query:** `{}`\n\n**Results:**\n", surql);
                output.push_str(&serde_json::to_string_pretty(&result).unwrap_or_default());
                output
            }
            Err(e) => format!("Error executing query: {}\n\nQuery was: {}", e, surql),
        }
    }

    // ===== Obsidian-like Context Exploration Tools =====

    /// Explore an entity's full graph neighborhood — like Obsidian's local graph view
    #[tool(description = "Explore the full neighborhood of any entity (function, class, config, doc, package, file). \
        Shows all connections: callers, callees, sibling functions, containing file, related configs/docs. \
        Use this to deeply understand how any piece of code or config fits into the system.")]
    async fn explore(&self, #[tool(aggr)] params: ExploreParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
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

                // For file-type results, show full context
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
    #[tool(description = "Get complete context for a file: all functions (with external callers), classes, imports, configs, docs, and packages. \
        Shows cross-file connections. Use this to understand a file's role in the system before reading or modifying it.")]
    async fn context_bundle(&self, #[tool(aggr)] params: ContextBundleParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.file_context(&params.file_path).await {
            Ok(result) => {
                let mut output = format!("## Context: {}\n\n", params.file_path);

                // File info
                if let Some(file) = result.get("file") {
                    let lang = file.get("language").and_then(|v| v.as_str()).unwrap_or("?");
                    let lines = file.get("line_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    output.push_str(&format!("**Language:** {} | **Lines:** {}\n\n", lang, lines));
                }

                // Functions with cross-file callers
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
                        // Show external callers (cross-file links!)
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

                // Classes
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

                // Imports
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

                // Configs, Docs, Packages, Infra
                for (key, label) in [("configs", "Config"), ("docs", "Documentation"), ("packages", "Packages"), ("infra", "Infrastructure")] {
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

                output
            }
            Err(e) => format!("Error getting context for '{}': {}", params.file_path, e),
        }
    }

    /// Search across ALL entity types — universal knowledge graph search
    #[tool(description = "Search across the entire knowledge graph: code, configs, docs, packages, infrastructure. \
        Unlike search_functions which only searches functions, this searches everything. \
        Use this when you need to find all mentions of a concept across code AND non-code files.")]
    async fn related(&self, #[tool(aggr)] params: RelatedParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);
        let limit = params.limit.unwrap_or(10);

        match gq.cross_search(&params.keyword, limit).await {
            Ok(result) => {
                let total = result.get("total_results").and_then(|v| v.as_u64()).unwrap_or(0);
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
    #[tool(description = "Find all backlinks to an entity: who calls it, who imports it, what file contains it, what depends on it. \
        Like Obsidian's backlinks panel — shows everything that points TO this entity. \
        Use this to understand the impact of changing something.")]
    async fn backlinks(&self, #[tool(aggr)] params: BacklinksParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let gq = GraphQuery::new(ctx.db);

        match gq.backlinks(&params.name).await {
            Ok(result) => {
                let total = result.get("total_backlinks").and_then(|v| v.as_u64()).unwrap_or(0);
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

    // ===== Conversation Memory Tools =====

    /// Index Claude Code conversation transcripts into the knowledge graph
    #[tool(description = "Index Claude Code conversation history into the knowledge graph. \
        Extracts decisions, problems, solutions, and discussion topics from JSONL transcripts. \
        Links them to code entities (functions, classes, files) mentioned in conversations. \
        After indexing, use conversation_search to query past decisions and problem-solving history.")]
    async fn index_conversations(&self, #[tool(aggr)] params: IndexConversationsParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };

        // Auto-detect Claude projects directory
        let project_dir = if let Some(dir) = params.project_dir {
            std::path::PathBuf::from(dir)
        } else {
            let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
            let claude_projects = home.join(".claude").join("projects");

            // Find project dir matching codebase path
            let codebase_str = ctx.codebase_path.to_string_lossy()
                .replace(['/', '\\', ':'], "-")
                .replace("--", "-");

            match std::fs::read_dir(&claude_projects) {
                Ok(entries) => {
                    let mut found = None;
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        // Match project directory by checking if it contains path components
                        if name.contains(&ctx.repo_name) || codebase_str.contains(&name) || name.contains("graph-rag") {
                            found = Some(entry.path());
                            break;
                        }
                    }
                    found.unwrap_or(claude_projects)
                }
                Err(_) => claude_projects,
            }
        };

        // Find JSONL files
        let jsonl_files: Vec<std::path::PathBuf> = match std::fs::read_dir(&project_dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| {
                    e.path().extension()
                        .map(|ext| ext == "jsonl")
                        .unwrap_or(false)
                })
                .map(|e| e.path())
                .collect(),
            Err(e) => return format!("Cannot read project dir '{}': {}", project_dir.display(), e),
        };

        if jsonl_files.is_empty() {
            return format!("No JSONL conversation files found in {}", project_dir.display());
        }

        // Load known entities for code linking
        let known_entities = load_known_entities(&ctx.db).await;
        let builder = codescope_core::graph::builder::GraphBuilder::new(ctx.db.clone());

        let mut total_result = codescope_core::conversation::ConvIndexResult::default();

        for jsonl_path in &jsonl_files {
            let jsonl_name = jsonl_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.jsonl")
                .to_string();

            // Incremental: check if this file is already indexed with same hash
            if let Ok(existing) = check_conversation_hash(&ctx.db, &jsonl_name).await {
                if let Some(stored_hash) = existing {
                    // Compute current file hash
                    if let Ok(content) = std::fs::read(jsonl_path) {
                        use sha2::{Digest, Sha256};
                        let current_hash = hex::encode(Sha256::digest(&content));
                        if stored_hash == current_hash {
                            total_result.skipped += 1;
                            continue;
                        }
                    }
                }
            }

            match codescope_core::conversation::parse_conversation(jsonl_path, &ctx.repo_name, &known_entities) {
                Ok((entities, relations, result)) => {
                    let _ = builder.insert_entities(&entities).await;
                    let _ = builder.insert_relations(&relations).await;
                    total_result.sessions_indexed += result.sessions_indexed;
                    total_result.decisions += result.decisions;
                    total_result.problems += result.problems;
                    total_result.solutions += result.solutions;
                    total_result.topics += result.topics;
                    total_result.code_links += result.code_links;
                }
                Err(e) => {
                    tracing::warn!("Failed to parse {}: {}", jsonl_path.display(), e);
                }
            }
        }

        // Index memory files in the project directory
        let memory_dir = project_dir.join("memory");
        let mut memory_count = 0;
        if memory_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&memory_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "md").unwrap_or(false) {
                        match codescope_core::conversation::parse_memory_file(&path, &ctx.repo_name, &known_entities) {
                            Ok((entities, relations)) => {
                                let _ = builder.insert_entities(&entities).await;
                                let _ = builder.insert_relations(&relations).await;
                                memory_count += 1;
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse memory file {}: {}", path.display(), e);
                            }
                        }
                    }
                }
            }
        }

        // Cross-session topic linking
        let cross_links = link_cross_session_topics(&ctx.db, &ctx.repo_name).await;

        format!(
            "## Conversation Indexing Complete\n\n\
             - Sessions indexed: {}\n\
             - Skipped (unchanged): {}\n\
             - Decisions: {}\n\
             - Problems: {}\n\
             - Solutions: {}\n\
             - Topics: {}\n\
             - Code links: {}\n\
             - Memory files: {}\n\
             - Cross-session links: {}\n\
             - Source: {}",
            total_result.sessions_indexed,
            total_result.skipped,
            total_result.decisions,
            total_result.problems,
            total_result.solutions,
            total_result.topics,
            total_result.code_links,
            memory_count,
            cross_links,
            project_dir.display(),
        )
    }

    /// Search conversation history — find past decisions, problems, and solutions
    #[tool(description = "Search conversation history for decisions, problems, solutions, and discussion topics. \
        Finds what was discussed about specific code entities, what decisions were made, and how problems were solved. \
        Like Obsidian search across your AI conversation notes. Index conversations first with index_conversations.")]
    async fn conversation_search(&self, #[tool(aggr)] params: ConversationSearchParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let limit = params.limit.unwrap_or(20) as u32;
        let filter_type = params.entity_type.as_deref().unwrap_or("all");

        // Build type filter
        let tables: Vec<&str> = match filter_type {
            "decision" => vec!["decision"],
            "problem" => vec!["problem"],
            "solution" => vec!["solution"],
            "topic" => vec!["conv_topic"],
            _ => vec!["decision", "problem", "solution", "conv_topic"],
        };

        let mut all_results = Vec::new();

        for table in &tables {
            let query = format!(
                "SELECT name, kind, body, file_path, start_line, '{}' AS type \
                 FROM {} WHERE string::contains(string::lowercase(name), string::lowercase($kw)) \
                 OR string::contains(string::lowercase(body), string::lowercase($kw)) \
                 LIMIT $lim;",
                table, table
            );

            match ctx.db.query(&query)
                .bind(("kw", params.query.clone()))
                .bind(("lim", limit))
                .await
            {
                Ok(mut response) => {
                    let results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                    all_results.extend(results);
                }
                Err(_) => {}
            }
        }

        // Also search via code entity links (discussed_in, decided_about)
        if filter_type == "all" || filter_type == "decision" {
            let link_query = format!(
                "SELECT <-decided_about<-decision.{{name, body}} AS linked_decisions \
                 FROM `function` WHERE name = $kw LIMIT 1;"
            );
            if let Ok(mut resp) = ctx.db.query(&link_query).bind(("kw", params.query.clone())).await {
                let linked: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
                if !linked.is_empty() {
                    all_results.push(serde_json::json!({
                        "type": "linked_decisions",
                        "for_entity": params.query,
                        "data": linked
                    }));
                }
            }
        }

        if all_results.is_empty() {
            return format!("No conversation history found for '{}'. Run index_conversations first.", params.query);
        }

        let mut output = format!("## Conversation History: '{}'\n\n", params.query);

        for item in &all_results {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let body = item.get("body").and_then(|v| v.as_str()).unwrap_or("");

            let icon = match item_type {
                "decision" => "**[DECISION]**",
                "problem" => "**[PROBLEM]**",
                "solution" => "**[SOLUTION]**",
                "conv_topic" => "**[TOPIC]**",
                "linked_decisions" => "**[LINKED]**",
                _ => "**[?]**",
            };

            output.push_str(&format!("{} {}\n", icon, name));
            if !body.is_empty() && body.len() > 10 {
                let preview = if body.len() > 200 { &body[..200] } else { body };
                output.push_str(&format!("  > {}\n", preview));
            }
            output.push('\n');
        }

        output
    }

    // ===== Temporal Conversation Query =====

    /// Search conversation history by time — find what was discussed about an entity recently
    #[tool(description = "Search conversation history over time for a specific code entity. \
        Shows decisions, problems, and solutions related to a function/class/file, ordered by time. \
        Use to answer 'what did we discuss about X last week?' or 'when was this function last changed?'.")]
    async fn conversation_timeline(&self, #[tool(aggr)] params: ConversationTimelineParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let limit = params.limit.unwrap_or(20) as u32;
        let _days_back = params.days_back.unwrap_or(30);
        let name = params.entity_name.clone();

        // Search across all conversation entity types for mentions of this entity
        let tables = ["decision", "problem", "solution", "conv_topic"];
        let mut all_results: Vec<serde_json::Value> = Vec::new();

        for table in &tables {
            let query = format!(
                "SELECT name, body, timestamp, kind, '{}' AS type \
                 FROM {} WHERE body CONTAINS $name \
                 ORDER BY timestamp DESC LIMIT $lim",
                table, table
            );
            if let Ok(mut resp) = ctx.db.query(&query)
                .bind(("name", name.clone()))
                .bind(("lim", limit))
                .await
            {
                let results: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
                all_results.extend(results);
            }
        }

        // Also check discussed_in relations
        let link_query = "SELECT <-discussed_in<-decision.{name, body, timestamp} AS decisions, \
                           <-discussed_in<-problem.{name, body, timestamp} AS problems, \
                           <-discussed_in<-solution.{name, body, timestamp} AS solutions \
                           FROM `function` WHERE name = $name LIMIT 1;";
        if let Ok(mut resp) = ctx.db.query(link_query).bind(("name", name.clone())).await {
            let linked: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
            if !linked.is_empty() {
                all_results.push(serde_json::json!({
                    "type": "linked",
                    "for_entity": name,
                    "data": linked
                }));
            }
        }

        if all_results.is_empty() {
            return format!("No conversation history found for '{}'. Run index_conversations first.", name);
        }

        let mut output = format!("## Timeline: '{}'\n\n", name);

        for item in &all_results {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            let item_name = item.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let timestamp = item.get("timestamp").and_then(|v| v.as_str()).unwrap_or("?");
            let body = item.get("body").and_then(|v| v.as_str()).unwrap_or("");

            let icon = match item_type {
                "decision" => "[DECISION]",
                "problem" => "[PROBLEM]",
                "solution" => "[SOLUTION]",
                "conv_topic" => "[TOPIC]",
                "linked" => "[LINKED]",
                _ => "[?]",
            };

            output.push_str(&format!("**{}** {} ({})\n", icon, item_name, timestamp));
            if !body.is_empty() && body.len() > 10 {
                let preview = if body.len() > 200 { &body[..200] } else { body };
                output.push_str(&format!("  > {}\n", preview));
            }
            output.push('\n');
        }

        output
    }

    // ===== Semantic Search Tools =====

    /// Generate embeddings for all functions in the graph
    #[tool(description = "Generate vector embeddings for all functions that don't have them yet. \
        Uses local FastEmbed by default (no external service needed). \
        Required before using semantic_search. Providers: 'fastembed' (local, default), 'ollama', 'openai'.")]
    async fn embed_functions(&self, #[tool(aggr)] params: EmbedParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let batch_size = params.batch_size.unwrap_or(100);
        let provider_name = params.provider.as_deref().unwrap_or("fastembed");

        let provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider_name {
            "ollama" => {
                Box::new(codescope_core::embeddings::OllamaProvider::new(
                    Some("http://localhost:11434".into()),
                    Some("nomic-embed-text".into()),
                ))
            }
            "openai" => {
                let api_key = match std::env::var("OPENAI_API_KEY") {
                    Ok(k) => k,
                    Err(_) => return "OPENAI_API_KEY environment variable not set.".into(),
                };
                Box::new(codescope_core::embeddings::OpenAIProvider::new(api_key, None))
            }
            _ => {
                // FastEmbed — local, no external service
                match codescope_core::embeddings::FastEmbedProvider::new() {
                    Ok(p) => Box::new(p),
                    Err(e) => return format!("Error creating FastEmbed provider: {}", e),
                }
            }
        };

        let pipeline = codescope_core::embeddings::EmbeddingPipeline::new(ctx.db, provider);

        // First embed new functions
        match pipeline.embed_functions(batch_size).await {
            Ok(result) => {
                // Also backfill BQ for any pre-existing embeddings without binary vectors
                let backfilled = pipeline.backfill_binary_quantization().await.unwrap_or(0);
                let dims = pipeline.dimensions();
                let bq_bytes = (dims + 7) / 8;

                format!(
                    "## Embedding Complete\n\n\
                     - **Embedded:** {} functions ({} dimensions)\n\
                     - **Binary Quantized:** {} (BQ backfilled: {})\n\
                     - **Memory per vector:** f32 = {} bytes, BQ = {} bytes (**{}x smaller**)\n\
                     - **Provider:** {}\n\
                     - **Search mode:** Two-stage (Hamming pre-filter → Cosine rerank)",
                    result.embedded, dims,
                    result.binary_quantized, backfilled,
                    dims * 4, bq_bytes, (dims * 4) / bq_bytes,
                    pipeline.provider_name()
                )
            }
            Err(e) => format!("Error embedding functions: {}", e),
        }
    }

    /// Search for semantically similar code using vector embeddings
    #[tool(description = "Search for code by meaning, not just name. Finds semantically similar functions \
        using vector embeddings. Run embed_functions first to generate embeddings. \
        Example: 'parse configuration file' finds all config-parsing functions regardless of naming.")]
    async fn semantic_search(&self, #[tool(aggr)] params: SemanticSearchParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let limit = params.limit.unwrap_or(10);
        let provider_name = params.provider.as_deref().unwrap_or("fastembed");

        let provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider_name {
            "ollama" => {
                Box::new(codescope_core::embeddings::OllamaProvider::new(
                    Some("http://localhost:11434".into()),
                    Some("nomic-embed-text".into()),
                ))
            }
            "openai" => {
                let api_key = match std::env::var("OPENAI_API_KEY") {
                    Ok(k) => k,
                    Err(_) => return "OPENAI_API_KEY environment variable not set.".into(),
                };
                Box::new(codescope_core::embeddings::OpenAIProvider::new(api_key, None))
            }
            _ => {
                match codescope_core::embeddings::FastEmbedProvider::new() {
                    Ok(p) => Box::new(p),
                    Err(e) => return format!("Error creating FastEmbed provider: {}", e),
                }
            }
        };

        let pipeline = codescope_core::embeddings::EmbeddingPipeline::new(ctx.db, provider);

        match pipeline.semantic_search(&params.query, limit).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!(
                        "No semantic matches for '{}'. Run embed_functions first to generate embeddings.",
                        params.query
                    );
                }
                let has_bq = results.first().and_then(|r| r.hamming_distance).is_some();
                let mode = if has_bq { "BQ + Cosine (two-stage)" } else { "Cosine only" };
                let mut output = format!("## Semantic Search: '{}'\n**Mode:** {}\n\n", params.query, mode);
                for (i, r) in results.iter().enumerate() {
                    let score = r.score.map(|s| format!("{:.3}", s)).unwrap_or_else(|| "?".into());
                    let hamming = r.hamming_distance.map(|h| format!(" (hamming: {})", h)).unwrap_or_default();
                    output.push_str(&format!(
                        "{}. **{}** ({}) — cosine: {}{}\n",
                        i + 1,
                        r.name,
                        r.file_path,
                        score,
                        hamming,
                    ));
                    if let Some(sig) = &r.signature {
                        output.push_str(&format!("   `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Semantic search error: {}", e),
        }
    }

    // ===== Code Quality Tools =====

    /// Find potentially dead code — functions with zero callers
    #[tool(description = "Find dead code: functions that are never called by any other function. \
        Filters out known entry points (main, test functions, handlers, constructors). \
        Useful for codebase cleanup and reducing maintenance burden.")]
    async fn find_dead_code(&self, #[tool(aggr)] params: DeadCodeParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let min_lines = params.min_lines.unwrap_or(3);
        let limit = params.limit.unwrap_or(50);

        // Find functions with zero incoming calls, excluding entry points
        let query = format!(
            "SELECT name, file_path, start_line, end_line, \
                    (end_line - start_line) AS size \
             FROM `function` \
             WHERE count(<-calls) = 0 \
               AND (end_line - start_line) >= {} \
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
             ORDER BY size DESC \
             LIMIT {}",
            min_lines, limit
        );

        match ctx.db.query(&query).await {
            Ok(mut response) => {
                let results: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                if results.is_empty() {
                    return "No dead code found (all functions have callers or are entry points).".into();
                }

                let mut output = format!("## Dead Code: {} potentially unused functions\n\n", results.len());
                output.push_str("| # | Function | File | Lines | Size |\n");
                output.push_str("|---|----------|------|-------|------|\n");

                for (i, r) in results.iter().enumerate() {
                    let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let fp = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    let start = r.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                    let size = r.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                    output.push_str(&format!(
                        "| {} | **{}** | {} | L{} | {} lines |\n",
                        i + 1, name, fp, start, size
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

    // ===== Team Intelligence Tools =====

    /// Detect team coding patterns from the codebase
    #[tool(description = "Detect team coding patterns: naming conventions, import styles, file structure patterns, \
        and common architectural patterns. Analyzes the actual codebase to learn how the team codes. \
        Use this to understand conventions before writing new code.")]
    async fn team_patterns(&self, #[tool(aggr)] params: TeamPatternsParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let focus = params.focus.as_deref().unwrap_or("all");
        let mut output = "## Team Coding Patterns\n\n".to_string();

        // 1. Naming conventions
        if focus == "all" || focus == "naming" {
            let naming_q = "SELECT name, language, file_path FROM `function` LIMIT 200";
            if let Ok(mut r) = ctx.db.query(naming_q).await {
                let fns: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                let mut snake = 0; let mut camel = 0; let mut pascal = 0;
                for f in &fns {
                    let n = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if n.contains('_') { snake += 1; }
                    else if n.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) { camel += 1; }
                    else { pascal += 1; }
                }
                let total = snake + camel + pascal;
                if total > 0 {
                    output.push_str("### Naming Conventions\n");
                    output.push_str(&format!("- snake_case: {}% ({}/{})\n", snake*100/total, snake, total));
                    output.push_str(&format!("- camelCase: {}% ({}/{})\n", camel*100/total, camel, total));
                    output.push_str(&format!("- PascalCase: {}% ({}/{})\n\n", pascal*100/total, pascal, total));
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
                    let mut patterns: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                    for imp in &imports {
                        let body = imp.get("body").and_then(|v| v.as_str()).unwrap_or("");
                        let pattern = if body.contains("from ") { "ES module (from)" }
                            else if body.contains("require(") { "CommonJS (require)" }
                            else if body.contains("use ") { "Rust (use)" }
                            else if body.contains("import ") { "import statement" }
                            else { "other" };
                        *patterns.entry(pattern.to_string()).or_insert(0) += 1;
                    }
                    let mut sorted: Vec<_> = patterns.into_iter().collect();
                    sorted.sort_by(|a,b| b.1.cmp(&a.1));
                    for (p,c) in &sorted { output.push_str(&format!("- {}: {} occurrences\n", p, c)); }
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
                    output.push_str(&format!("### Function Size\n- Average: {:.0} lines\n- Largest: {} lines\n\n", avg, max));
                }
            }

            // Top-level directory structure
            let dir_q = "SELECT file_path FROM file LIMIT 500";
            if let Ok(mut r) = ctx.db.query(dir_q).await {
                let files: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                let mut dirs: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                for f in &files {
                    let fp = f.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some(first) = fp.split('/').next() {
                        *dirs.entry(first.to_string()).or_insert(0) += 1;
                    }
                }
                let mut sorted: Vec<_> = dirs.into_iter().collect();
                sorted.sort_by(|a,b| b.1.cmp(&a.1));
                output.push_str("### Project Structure (top-level)\n");
                for (d,c) in sorted.iter().take(10) { output.push_str(&format!("- {}/  ({} files)\n", d, c)); }
            }
        }

        output
    }

    /// Pre-flight check before editing a file — validates against team patterns
    #[tool(description = "Check if a planned edit aligns with team coding patterns. \
        Call before writing code to avoid introducing inconsistencies. \
        Returns warnings if naming, structure, or style deviates from the codebase norm.")]
    async fn edit_preflight(&self, #[tool(aggr)] params: EditPreflightParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
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
                lang = f.get("language").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            }
        }

        let name = &params.entity_name;
        let has_underscore = name.contains('_');
        let starts_lower = name.chars().next().map(|c| c.is_lowercase()).unwrap_or(true);

        // Naming check
        match lang.as_str() {
            "rust" | "python" | "ruby" | "elixir" => {
                if !has_underscore && name.len() > 3 && starts_lower {
                    warnings.push(format!("Naming: '{}' uses camelCase but {} convention is snake_case", name, lang));
                }
            }
            "typescript" | "javascript" | "java" | "dart" | "kotlin" | "go" => {
                if has_underscore && starts_lower {
                    warnings.push(format!("Naming: '{}' uses snake_case but {} convention is camelCase", name, lang));
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
            let sibling_names: Vec<&str> = siblings.iter()
                .filter_map(|s| s.get("name").and_then(|v| v.as_str()))
                .collect();

            if !sibling_names.is_empty() {
                let snake_count = sibling_names.iter().filter(|n| n.contains('_')).count();
                let ratio = snake_count as f32 / sibling_names.len() as f32;

                if ratio > 0.7 && !has_underscore && name.len() > 3 {
                    warnings.push(format!("Style: {}% of siblings use snake_case, but '{}' doesn't", (ratio*100.0) as u32, name));
                } else if ratio < 0.3 && has_underscore {
                    warnings.push(format!("Style: {}% of siblings use camelCase, but '{}' uses snake_case", ((1.0-ratio)*100.0) as u32, name));
                }

                info.push(format!("File has {} existing functions", sibling_names.len()));
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
                    warnings.push(format!("File size: {} lines — consider splitting into smaller modules", lines));
                }
                info.push(format!("File is {} lines", lines));
            }
        }

        let mut output = format!("## Edit Preflight: {} in {}\n\n", params.entity_name, params.file_path);
        output.push_str(&format!("**Language:** {}\n\n", lang));

        if warnings.is_empty() {
            output.push_str("**All checks passed.** Edit aligns with team patterns.\n\n");
        } else {
            output.push_str(&format!("**{} warnings:**\n", warnings.len()));
            for w in &warnings { output.push_str(&format!("- {} {}\n", "!!!", w)); }
            output.push('\n');
        }

        if !info.is_empty() {
            output.push_str("**Context:**\n");
            for i in &info { output.push_str(&format!("- {}\n", i)); }
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
    async fn manage_adr(&self, #[tool(aggr)] params: ManageAdrParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };

        match params.action.as_str() {
            "list" => {
                let q = "SELECT name, body, timestamp, qualified_name FROM decision WHERE repo = $repo ORDER BY timestamp DESC LIMIT 50";
                match ctx.db.query(q).bind(("repo", ctx.repo_name.clone())).await {
                    Ok(mut r) => {
                        let decisions: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if decisions.is_empty() {
                            return "No ADRs found. Decisions are auto-extracted from conversations, or create one with action='create'.".into();
                        }
                        let mut output = format!("## Architecture Decision Records ({} total)\n\n", decisions.len());
                        for (i, d) in decisions.iter().enumerate() {
                            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let ts = d.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
                            let body = d.get("body").and_then(|v| v.as_str()).unwrap_or("");
                            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
                            output.push_str(&format!("### ADR-{:03}: {}\n", i+1, name));
                            if !date.is_empty() { output.push_str(&format!("*Date: {}*\n\n", date)); }
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
                let qname = format!("{}:adr:{}", ctx.repo_name, title.to_lowercase().replace(' ', "_").chars().take(60).collect::<String>());
                let ts = chrono::Utc::now().to_rfc3339();

                let q = "UPSERT decision SET name = $name, qualified_name = $qname, \
                         body = $body, repo = $repo, language = 'adr', \
                         file_path = 'adr', start_line = 0, end_line = 0, \
                         timestamp = $ts";
                match ctx.db.query(q)
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
                let q = "SELECT * FROM decision WHERE name CONTAINS $search AND repo = $repo LIMIT 1";
                match ctx.db.query(q).bind(("search", id.to_string())).bind(("repo", ctx.repo_name.clone())).await {
                    Ok(mut r) => {
                        let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        if let Some(d) = results.first() {
                            serde_json::to_string_pretty(d).unwrap_or_else(|_| "Error formatting".into())
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
    #[tool(description = "Save a persistent memory note that survives across sessions and is shared between all agents \
        (Claude Code, Cursor, etc.) connected to this project. Use for recording: preferences, conventions, \
        important context, architectural notes, TODOs. Memories are searchable via conversation_search.")]
    async fn memory_save(&self, #[tool(aggr)] params: NaturalLanguageQueryParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let text = &params.question;
        let ts = chrono::Utc::now().to_rfc3339();
        let slug = text.to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
            .chars().take(60).collect::<String>();
        let qname = format!("{}:memory:{}", ctx.repo_name, slug);

        let q = "UPSERT conv_topic SET name = $name, qualified_name = $qname, \
                 body = $body, repo = $repo, language = 'memory', kind = 'shared_memory', \
                 file_path = 'memory', start_line = 0, end_line = 0, timestamp = $ts";
        match ctx.db.query(q)
            .bind(("name", text.chars().take(100).collect::<String>()))
            .bind(("qname", qname))
            .bind(("body", text.to_string()))
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("ts", ts))
            .await
        {
            Ok(_) => format!("Memory saved. Accessible by all agents connected to '{}'.", ctx.repo_name),
            Err(e) => format!("Error saving memory: {}", e),
        }
    }

    /// Search shared memories and conversation history
    #[tool(description = "Search shared memories, decisions, and conversation history across all sessions. \
        Returns memories saved by any agent, plus auto-extracted decisions/problems/solutions.")]
    async fn memory_search(&self, #[tool(aggr)] params: SearchParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
        let limit = params.limit.unwrap_or(20) as i64;
        let safe = params.query.replace('\'', "");

        let q = "SELECT name, body, kind, timestamp, 'memory' AS source FROM conv_topic \
                 WHERE repo = $repo AND (name ~ $search OR body ~ $search) \
                 UNION \
                 SELECT name, body, 'decision' AS kind, timestamp, 'conversation' AS source FROM decision \
                 WHERE repo = $repo AND (name ~ $search OR body ~ $search) \
                 UNION \
                 SELECT name, body, 'problem' AS kind, timestamp, 'conversation' AS source FROM problem \
                 WHERE repo = $repo AND (name ~ $search OR body ~ $search) \
                 UNION \
                 SELECT name, body, 'solution' AS kind, timestamp, 'conversation' AS source FROM solution \
                 WHERE repo = $repo AND (name ~ $search OR body ~ $search) \
                 ORDER BY timestamp DESC LIMIT $lim";

        match ctx.db.query(q)
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("search", safe))
            .bind(("lim", limit))
            .await
        {
            Ok(mut r) => {
                let results: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if results.is_empty() {
                    return format!("No memories found for '{}'", params.query);
                }
                let mut output = format!("## Memory Search: '{}' ({} results)\n\n", params.query, results.len());
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

    // ===== Graph Analytics =====

    /// Detect code communities and architectural boundaries
    #[tool(description = "Detect code communities, bridge modules, and central nodes in the codebase graph. \
        'clusters' — find groups of tightly-connected files, \
        'bridges' — find modules that connect separate clusters (high betweenness), \
        'central' — find the most connected/important entities (PageRank-like), \
        'all' — run all analyses.")]
    async fn community_detection(&self, #[tool(aggr)] params: CommunityDetectionParams) -> String {
        let ctx = match self.ctx().await { Ok(c) => c, Err(e) => return e };
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
                        output.push_str(&format!("| {} | {} | {} | {} |\n", fp, out_c, in_c, total));
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
                        output.push_str(&format!("| **{}** | {} | {} | {} | {} |\n", name, fp, callers, callees, score));
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
                        output.push_str(&format!("| {} | **{}** | {} | {} |\n", i+1, name, fp, deg));
                    }
                }
            }
        }

        output
    }
}

/// Load known entity names from the graph for conversation-to-code linking.
/// Queries all 11 entity tables to maximize linking coverage.
async fn load_known_entities(db: &surrealdb::Surreal<surrealdb::engine::local::Db>) -> Vec<String> {
    let query = "SELECT name, qualified_name FROM `function`; \
                 SELECT name, qualified_name FROM class; \
                 SELECT path AS name, path AS qualified_name FROM file; \
                 SELECT name, qualified_name FROM module; \
                 SELECT name, qualified_name FROM variable; \
                 SELECT name, qualified_name FROM import_decl; \
                 SELECT name, qualified_name FROM config; \
                 SELECT name, qualified_name FROM doc; \
                 SELECT name, qualified_name FROM api; \
                 SELECT name, qualified_name FROM infra; \
                 SELECT name, qualified_name FROM package;";

    let table_names = [
        "function", "class", "file", "module", "variable",
        "import_decl", "config", "doc", "api", "infra", "package",
    ];

    match db.query(query).await {
        Ok(mut response) => {
            let mut entities = Vec::new();

            for (table_idx, table_name) in table_names.iter().enumerate() {
                let results: Vec<serde_json::Value> = response.take(table_idx).unwrap_or_default();
                for r in results {
                    if let (Some(name), Some(qname)) = (
                        r.get("name").and_then(|v| v.as_str()),
                        r.get("qualified_name").and_then(|v| v.as_str()),
                    ) {
                        entities.push(format!("{}:{}:{}", table_name, name, qname));
                    }
                }
            }

            entities
        }
        Err(_) => Vec::new(),
    }
}

fn extract_path_from_question(question: &str) -> String {
    for word in question.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '\\' && c != '.' && c != '_' && c != '-');
        if clean.contains('.') && (clean.contains('/') || clean.contains('\\') || clean.ends_with(".rs") || clean.ends_with(".ts") || clean.ends_with(".py") || clean.ends_with(".go") || clean.ends_with(".java") || clean.ends_with(".js")) {
            return clean.to_string();
        }
    }
    question.to_string()
}

/// Check if a conversation file is already indexed by comparing stored hash
async fn check_conversation_hash(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    file_name: &str,
) -> anyhow::Result<Option<String>> {
    #[derive(serde::Deserialize)]
    struct HashRecord {
        hash: Option<String>,
    }
    let results: Vec<HashRecord> = db
        .query("SELECT hash FROM conversation WHERE file_path = $name LIMIT 1")
        .bind(("name", file_name.to_string()))
        .await?
        .take(0)?;
    Ok(results.first().and_then(|r| r.hash.clone()))
}

/// Find the Claude projects directory matching a codebase path
pub fn find_claude_project_dir(codebase_path: &std::path::Path, repo_name: &str) -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let claude_projects = home.join(".claude").join("projects");

    let codebase_str = codebase_path.to_string_lossy()
        .replace(['/', '\\', ':'], "-")
        .replace("--", "-");

    match std::fs::read_dir(&claude_projects) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains(repo_name) || codebase_str.contains(&name) {
                    return entry.path();
                }
            }
            claude_projects
        }
        Err(_) => claude_projects,
    }
}

/// Build a concise conversation context summary from indexed conversations.
/// This gets injected into ServerInfo.instructions so Claude sees it automatically.
async fn build_context_summary(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    repo: &str,
) -> String {
    let mut sections = Vec::new();

    // Recent decisions (last 10)
    let decisions: Vec<serde_json::Value> = db
        .query("SELECT name, body, timestamp FROM decision WHERE repo = $repo ORDER BY timestamp DESC LIMIT 10")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !decisions.is_empty() {
        let mut s = "## Recent Decisions\n".to_string();
        for d in &decisions {
            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let ts = d.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
            s.push_str(&format!("- {}: {}\n", date, name));
        }
        sections.push(s);
    }

    // Recent problems (last 5 unsolved)
    let problems: Vec<serde_json::Value> = db
        .query("SELECT name, timestamp FROM problem WHERE repo = $repo ORDER BY timestamp DESC LIMIT 5")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !problems.is_empty() {
        let mut s = "## Recent Problems\n".to_string();
        for p in &problems {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let ts = p.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
            s.push_str(&format!("- {}: {}\n", date, name));
        }
        sections.push(s);
    }

    // Recent solutions (last 5)
    let solutions: Vec<serde_json::Value> = db
        .query("SELECT name, timestamp FROM solution WHERE repo = $repo ORDER BY timestamp DESC LIMIT 5")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !solutions.is_empty() {
        let mut s = "## Recent Solutions\n".to_string();
        for sol in &solutions {
            let name = sol.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let ts = sol.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
            s.push_str(&format!("- {}: {}\n", date, name));
        }
        sections.push(s);
    }

    // Session count
    let stats: Vec<serde_json::Value> = db
        .query("SELECT count() FROM conversation WHERE repo = $repo GROUP ALL")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    let session_count = stats.first()
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if session_count > 0 {
        sections.push(format!("*{} conversation sessions indexed for this project.*", session_count));
    }

    if sections.is_empty() {
        String::new()
    } else {
        format!("# Conversation Context\n\n{}", sections.join("\n"))
    }
}

/// Generate CONTEXT.md in the project's .claude directory.
/// Claude reads this automatically at session start.
pub async fn generate_context_md(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    repo: &str,
    codebase_path: &std::path::Path,
) {
    let summary = build_context_summary(db, repo).await;
    if summary.is_empty() {
        return;
    }

    let context_path = codebase_path.join(".claude").join("CONTEXT.md");
    if let Some(parent) = context_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let content = format!(
        "<!-- Auto-generated by Codescope. Do not edit manually. -->\n\
         <!-- Updated: {} -->\n\n\
         {}\n\n\
         > Use `conversation_search` for deeper queries, `explore` for entity graph navigation.\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
        summary,
    );

    match std::fs::write(&context_path, &content) {
        Ok(_) => tracing::info!("Generated CONTEXT.md at {}", context_path.display()),
        Err(e) => tracing::warn!("Failed to write CONTEXT.md: {}", e),
    }
}

/// Create cross-session topic links: sessions discussing the same code entity get co_discusses edges
async fn link_cross_session_topics(
    db: &surrealdb::Surreal<surrealdb::engine::local::Db>,
    _repo: &str,
) -> usize {
    // Find code entities discussed in multiple sessions
    let query = "SELECT out AS entity, array::group(in) AS sessions \
                 FROM discussed_in \
                 GROUP BY out \
                 HAVING count() > 1 \
                 LIMIT 50;";

    let results: Vec<serde_json::Value> = match db.query(query).await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(_) => return 0,
    };

    let mut link_count = 0;
    for row in &results {
        let sessions = match row.get("sessions").and_then(|v| v.as_array()) {
            Some(s) => s,
            None => continue,
        };
        // Create pairwise co_discusses links (capped at 10 sessions per entity)
        let capped: Vec<_> = sessions.iter().take(10).collect();
        for i in 0..capped.len() {
            for j in (i + 1)..capped.len() {
                let from_id = capped[i].as_str().unwrap_or("");
                let to_id = capped[j].as_str().unwrap_or("");
                if !from_id.is_empty() && !to_id.is_empty() {
                    let q = format!(
                        "LET $existing = (SELECT * FROM co_discusses WHERE in = {} AND out = {} LIMIT 1); \
                         IF !$existing THEN \
                             RELATE {}->co_discusses->{} \
                         END;",
                        from_id, to_id, from_id, to_id
                    );
                    if db.query(&q).await.is_ok() {
                        link_count += 1;
                    }
                }
            }
        }
    }

    link_count
}

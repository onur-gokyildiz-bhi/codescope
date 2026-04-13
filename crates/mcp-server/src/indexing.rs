//! Indexing pipeline orchestrator extracted from `lib.rs::run_stdio`.
//!
//! Each phase is a separate `async fn` so failures in one phase no longer
//! silently abort the rest. Phases are still ordered, but errors are logged
//! and execution continues to the next phase whenever it's safe.
//!
//! Pipeline overview:
//!
//! ```text
//!   parse_files       → Phase 1 (CPU-bound, rayon)
//!   insert_results    → Phase 2 (DB writes)
//!   resolve_calls     → Phase 2.5
//!   resolve_virtual   → Phase 2.6
//!   sync_git          → Phase 2.7
//!   index_conversations → Phase 3
//!   load_context      → Phase 4
//!   auto_embed        → Phase 4.5
//!   start_watcher     → Phase 5
//!   spawn_health      → Phase 6
//!   spawn_periodic    → Phase 7
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

use crate::collect_jsonl_files;
use crate::helpers;
use crate::server::GraphRagServer;
use crate::watcher;
use codescope_core::graph::builder::GraphBuilder;

/// Orchestrates background indexing for `run_stdio`. Owns shared state.
pub struct IndexingPipeline {
    pub db: Surreal<Db>,
    pub repo: String,
    pub path: PathBuf,
    pub mcp: GraphRagServer,
}

type ParseResult = (
    Vec<codescope_core::CodeEntity>,
    Vec<codescope_core::CodeRelation>,
);

impl IndexingPipeline {
    pub fn new(db: Surreal<Db>, repo: String, path: PathBuf, mcp: GraphRagServer) -> Arc<Self> {
        Arc::new(Self {
            db,
            repo,
            path,
            mcp,
        })
    }

    /// Run all phases sequentially. Each phase logs its own errors.
    pub async fn run_full(self: Arc<Self>) {
        tracing::info!("Background indexing {}...", self.path.display());
        let builder = GraphBuilder::new(self.db.clone());

        // Phase 0: clean stale entities to prevent abs-path / rel-path duplicates.
        // UPSERT keys on qualified_name which includes file_path — if the same
        // file was previously indexed with a different path form (absolute vs
        // relative, \\?\ prefix vs not), the old record stays alongside the new
        // one. Wiping code entities before re-indexing is the simplest fix.
        self.phase0_clean_stale().await;

        // Phase 1: parse files in parallel (CPU-bound)
        let parse_results = self.phase1_parse_files().await;

        // Phase 2: batch insert
        let file_count = self.phase2_insert_results(&builder, parse_results).await;
        tracing::info!("Background indexing complete: {} files", file_count);

        // Phase 2.5: cross-file call targets
        self.phase25_resolve_calls(&builder).await;

        // Phase 2.6: virtual dispatch
        self.phase26_resolve_virtual(&builder).await;

        // Phase 2.7: git history
        self.phase27_sync_git().await;

        // Phase 3: conversations + memory
        self.phase3_index_conversations(&builder).await;

        // Phase 4: context summary
        self.phase4_load_context().await;

        // Phase 4.5: auto-embed
        self.phase45_auto_embed().await;

        // Phase 5: file watcher
        self.phase5_start_watcher();

        // Phase 6: health check
        self.clone().phase6_spawn_health();

        // Phase 7: periodic conv re-index
        self.clone().phase7_spawn_periodic();
    }

    // ─── Phase 0: clean stale entities ────────────────────────────

    async fn phase0_clean_stale(&self) {
        // Delete all code entities and edges, then re-insert fresh.
        // This prevents duplicates from path normalization differences
        // between CLI init and MCP auto-index.
        //
        // We preserve: conversations, memory, skills (non-code data).
        let tables = [
            "`function`",
            "class",
            "import_decl",
            "file",
            "config",
            "doc",
            "package",
            "infra",
            "calls",
            "contains",
            "imports",
            "inherits",
        ];
        for table in &tables {
            if let Err(e) = self.db.query(format!("DELETE {table}")).await {
                tracing::warn!("Clean {table}: {e}");
            }
        }
        tracing::info!("Cleaned stale entities for fresh re-index");
    }

    // ─── Phase 1: file collection + parallel parsing ─────────────

    async fn phase1_parse_files(&self) -> Vec<ParseResult> {
        let parse_path = self.path.clone();
        // Normalize the base path: canonicalize + strip \\?\ prefix (Windows).
        // This must match what init.rs does, otherwise the same file gets
        // different qualified_names from CLI init vs MCP auto-index, creating
        // duplicate entities in the graph.
        let parse_path = parse_path.canonicalize().unwrap_or(parse_path);
        let parse_path = {
            let s = parse_path.to_string_lossy();
            if let Some(stripped) = s.strip_prefix(r"\\?\") {
                PathBuf::from(stripped)
            } else {
                parse_path
            }
        };
        let parse_repo = self.repo.clone();
        tokio::task::spawn_blocking(move || {
            use rayon::prelude::*;
            let parser = codescope_core::parser::CodeParser::new();
            let walker = ignore::WalkBuilder::new(&parse_path)
                .hidden(true)
                .git_ignore(true)
                .build();

            let files: Vec<PathBuf> = walker
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
        .unwrap_or_default()
    }

    // ─── Phase 2: batch insert ───────────────────────────────────

    async fn phase2_insert_results(
        &self,
        builder: &GraphBuilder,
        results: Vec<ParseResult>,
    ) -> usize {
        let mut file_count = 0;
        for (entities, relations) in results {
            if let Err(e) = builder.insert_entities(&entities).await {
                tracing::warn!("Entity insert failed: {e}");
            }
            if let Err(e) = builder.insert_relations(&relations).await {
                tracing::warn!("Relation insert failed: {e}");
            }
            file_count += 1;
        }
        file_count
    }

    // ─── Phase 2.5: cross-file call resolution ───────────────────

    async fn phase25_resolve_calls(&self, builder: &GraphBuilder) {
        match builder.resolve_call_targets(&self.repo).await {
            Ok(resolved) if resolved > 0 => {
                tracing::info!("Resolved {} cross-file call targets", resolved);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("Call target resolution failed: {}", e),
        }
    }

    // ─── Phase 2.6: virtual dispatch ─────────────────────────────

    async fn phase26_resolve_virtual(&self, builder: &GraphBuilder) {
        match builder.resolve_virtual_dispatch(&self.repo).await {
            Ok(resolved) if resolved > 0 => {
                tracing::info!("Resolved {} virtual dispatch edges", resolved);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("Virtual dispatch resolution failed: {}", e),
        }
    }

    // ─── Phase 2.7: git history sync ─────────────────────────────

    async fn phase27_sync_git(&self) {
        let git_path = self.path.clone();
        let commits = tokio::task::spawn_blocking(move || {
            codescope_core::temporal::GitAnalyzer::open(&git_path)
                .and_then(|a| a.recent_commits(500))
        })
        .await
        .unwrap_or_else(|_| Ok(vec![]));

        match commits {
            Ok(ref c) if !c.is_empty() => {
                let git_sync = codescope_core::temporal::TemporalGraphSync::new(self.db.clone());
                match git_sync.sync_commit_data(c, &self.repo).await {
                    Ok(n) if n > 0 => {
                        tracing::info!("Synced {} git commits", n);
                    }
                    Ok(_) => {}
                    Err(e) => tracing::debug!("Git sync failed: {}", e),
                }
            }
            Err(e) => tracing::debug!("Git history skipped: {}", e),
            _ => {}
        }
    }

    // ─── Phase 3: conversations + memory files ───────────────────

    async fn phase3_index_conversations(&self, builder: &GraphBuilder) {
        let project_dir = helpers::find_claude_project_dir(&self.path, &self.repo);
        tracing::info!("Auto-indexing conversations from {}", project_dir.display());

        let known_entities: Vec<String> = Vec::new();
        let mut jsonl_files = Vec::new();
        collect_jsonl_files(&project_dir, &mut jsonl_files);

        let mut conv_count = 0;
        for jsonl_path in &jsonl_files {
            match codescope_core::conversation::parse_conversation(
                jsonl_path,
                &self.repo,
                &known_entities,
            ) {
                Ok((entities, relations, _)) => {
                    if let Err(e) = builder.insert_entities(&entities).await {
                        tracing::warn!("Entity insert failed: {e}");
                    }
                    if let Err(e) = builder.insert_relations(&relations).await {
                        tracing::warn!("Relation insert failed: {e}");
                    }
                    conv_count += 1;
                }
                Err(e) => {
                    tracing::debug!("Conversation parse error {}: {}", jsonl_path.display(), e);
                }
            }
        }

        // Memory files
        let memory_dir = project_dir.join("memory");
        let mut mem_count = 0;
        if memory_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&memory_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "md").unwrap_or(false) {
                        if let Ok((ents, rels)) = codescope_core::conversation::parse_memory_file(
                            &path,
                            &self.repo,
                            &known_entities,
                        ) {
                            if let Err(e) = builder.insert_entities(&ents).await {
                                tracing::warn!("Entity insert failed: {e}");
                            }
                            if let Err(e) = builder.insert_relations(&rels).await {
                                tracing::warn!("Relation insert failed: {e}");
                            }
                            mem_count += 1;
                        }
                    }
                }
            }
        }

        tracing::info!(
            "Conversation indexing: {} sessions, {} memory files",
            conv_count,
            mem_count
        );
    }

    // ─── Phase 4: load context summary ───────────────────────────

    async fn phase4_load_context(&self) {
        helpers::generate_context_md(&self.db, &self.repo, &self.path).await;
        self.mcp.load_context_summary().await;
        tracing::info!("Context summary loaded into MCP server instructions");
    }

    // ─── Phase 4.5: auto-embed functions ─────────────────────────

    async fn phase45_auto_embed(&self) {
        match codescope_core::embeddings::FastEmbedProvider::new() {
            Ok(provider) => {
                let pipeline = codescope_core::embeddings::EmbeddingPipeline::new(
                    self.db.clone(),
                    Box::new(provider),
                );
                match pipeline.embed_functions(500).await {
                    Ok(result) => {
                        if result.embedded > 0 {
                            tracing::info!(
                                "Auto-embedded {} functions ({} BQ)",
                                result.embedded,
                                result.binary_quantized
                            );
                        }
                    }
                    Err(e) => tracing::debug!("Auto-embed skipped: {}", e),
                }
            }
            Err(e) => tracing::debug!("FastEmbed not available: {}", e),
        }
    }

    // ─── Phase 5: file watcher ───────────────────────────────────

    fn phase5_start_watcher(&self) {
        match watcher::start_watcher(&self.path) {
            Ok(rx) => {
                watcher::spawn_reindex_task(
                    rx,
                    self.db.clone(),
                    self.repo.clone(),
                    self.path.clone(),
                );
                tracing::info!("File watcher active — changes will auto-reindex");
            }
            Err(e) => {
                tracing::warn!("File watcher failed to start: {}", e);
            }
        }
    }

    // ─── Phase 6: periodic DB health check ───────────────────────

    fn phase6_spawn_health(self: Arc<Self>) {
        let health_db = self.db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let gq = codescope_core::graph::query::GraphQuery::new(health_db.clone());
                if let Err(e) = gq.health_check().await {
                    tracing::error!("DB health check failed: {}", e);
                }
            }
        });
    }

    // ─── Phase 7: periodic conversation re-index ─────────────────

    fn phase7_spawn_periodic(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            interval.tick().await; // skip first immediate tick
            loop {
                interval.tick().await;
                tracing::debug!("Periodic conversation re-index...");

                let project_dir = helpers::find_claude_project_dir(&self.path, &self.repo);
                let builder = GraphBuilder::new(self.db.clone());
                let known: Vec<String> = Vec::new();

                let mut jsonl_files = Vec::new();
                collect_jsonl_files(&project_dir, &mut jsonl_files);

                let mut new_count = 0;
                for jsonl_path in &jsonl_files {
                    let fname = jsonl_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    if let Ok(Some(_)) = helpers::check_conversation_hash(&self.db, fname).await {
                        continue; // already indexed
                    }

                    if let Ok((entities, relations, _)) =
                        codescope_core::conversation::parse_conversation(
                            jsonl_path, &self.repo, &known,
                        )
                    {
                        if let Err(e) = builder.insert_entities(&entities).await {
                            tracing::warn!("Entity insert failed: {e}");
                        }
                        if let Err(e) = builder.insert_relations(&relations).await {
                            tracing::warn!("Relation insert failed: {e}");
                        }
                        new_count += 1;
                    }
                }

                if new_count > 0 {
                    tracing::info!("Periodic index: {} new conversations", new_count);
                    helpers::generate_context_md(&self.db, &self.repo, &self.path).await;
                    self.mcp.load_context_summary().await;
                }
            }
        });
    }
}

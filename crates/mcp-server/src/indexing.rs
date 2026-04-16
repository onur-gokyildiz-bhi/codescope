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
use crate::index_state::IndexState;
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
        let state = self.mcp.index_state().clone();
        state.start().await;

        let builder = GraphBuilder::new(self.db.clone());

        // Phase 1 FIRST: parse into an in-memory staging set BEFORE we
        // touch the DB. If parsing produces zero results (e.g. directory
        // permission denied, unsupported project), we abort WITHOUT
        // wiping the existing graph — the user's prior index stays
        // intact and `index_status` surfaces the reason.
        let parse_results = self.phase1_parse_files(&state).await;

        if parse_results.is_empty() {
            // No files parsed successfully. Don't clobber the DB.
            let reason =
                "phase1 produced zero parse results — check that the path is a supported codebase"
                    .to_string();
            state.mark_failed(reason.clone()).await;
            tracing::error!("{} — skipping DB wipe, graph left unchanged", reason);
            return;
        }

        // Phase 0: clean stale entities to prevent abs-path / rel-path duplicates.
        // UPSERT keys on qualified_name which includes file_path — if the same
        // file was previously indexed with a different path form (absolute vs
        // relative, \\?\ prefix vs not), the old record stays alongside the new
        // one. Wiping code entities before re-indexing is the simplest fix.
        //
        // NOTE: This is staging-swap-lite — we parsed into `parse_results`
        // FIRST (above), so a phase1 failure never leaves the DB empty.
        // A true transactional swap would require SurrealDB table-rename
        // support, which isn't available in the current version.
        if let Err(e) = self.phase0_clean_stale().await {
            state
                .mark_failed(format!("phase0 (clean stale) failed: {e}"))
                .await;
            tracing::error!("Phase 0 failed — graph may be inconsistent: {e}");
            return;
        }

        // Phase 2: batch insert
        let file_count = self
            .phase2_insert_results(&builder, parse_results, &state)
            .await;
        tracing::info!("Background indexing complete: {} files", file_count);

        if file_count == 0 {
            // DB was wiped in phase0 but phase2 inserted nothing.
            // This is the "silent empty DB" failure mode the user
            // reported — surface it explicitly.
            state
                .mark_failed("phase2 inserted zero files after phase0 wiped the DB — the graph is empty; re-run with --auto-index or call index_codebase")
                .await;
            return;
        }

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

        // Phase 5: file watcher (code changes)
        self.phase5_start_watcher();

        // Phase 5.5: knowledge source watcher (.raw/ directory)
        self.phase55_watch_knowledge_sources();

        // Phase 6: health check
        self.clone().phase6_spawn_health();

        // Phase 7: periodic conv re-index
        self.clone().phase7_spawn_periodic();

        // All foreground phases done. Mark index Ready so the gate stops
        // short-circuiting tool calls. Background phases (health check,
        // periodic re-index) continue running.
        state.mark_ready().await;
        tracing::info!("Index state: Ready");
    }

    // ─── Phase 0: clean stale entities ────────────────────────────

    /// Delete all code entities and edges before re-inserting fresh.
    /// Returns an error only if EVERY delete fails (DB connection is gone)
    /// — individual table deletes can fail cheaply (table doesn't exist yet)
    /// and that's OK.
    async fn phase0_clean_stale(&self) -> Result<(), String> {
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
        let mut all_failed = true;
        let mut last_err: Option<String> = None;
        for table in &tables {
            match self.db.query(format!("DELETE {table}")).await {
                Ok(_) => {
                    all_failed = false;
                }
                Err(e) => {
                    tracing::warn!("Clean {table}: {e}");
                    last_err = Some(e.to_string());
                }
            }
        }
        if all_failed {
            return Err(last_err.unwrap_or_else(|| "every DELETE failed".into()));
        }
        tracing::info!("Cleaned stale entities for fresh re-index");
        Ok(())
    }

    // ─── Phase 1: file collection + parallel parsing ─────────────

    /// Parse all supported files under `self.path` in parallel and return
    /// `(entities, relations)` per successful file. Per-file read or parse
    /// errors are pushed onto `IndexState` (logged + counted) instead of
    /// being silently dropped with `.ok()`.
    async fn phase1_parse_files(&self, state: &IndexState) -> Vec<ParseResult> {
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

        // Collect per-file errors in the worker thread, then replay them
        // onto the async `IndexState` after the join. This avoids pulling
        // a tokio runtime handle into the rayon closure.
        type FileErr = (PathBuf, String);
        let join_result = tokio::task::spawn_blocking(move || {
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

            let total = files.len();
            tracing::info!("Found {} files to parse", total);

            // Each worker returns Either(ok_result, err_record) via a
            // 2-tuple Vec of enums. We just flatten at the end.
            enum Outcome {
                Ok(ParseResult),
                Skipped(PathBuf, String),
                Err(FileErr),
            }

            let outcomes: Vec<Outcome> = files
                .par_iter()
                .map(|file_path| {
                    let rel_path = file_path
                        .strip_prefix(&parse_path)
                        .unwrap_or(file_path)
                        .to_string_lossy()
                        .to_string()
                        .replace('\\', "/");
                    let content = match std::fs::read_to_string(file_path) {
                        Ok(c) => c,
                        Err(e) => {
                            return Outcome::Skipped(file_path.clone(), format!("read: {e}"));
                        }
                    };
                    match parser.parse_source(
                        std::path::Path::new(&rel_path),
                        &content,
                        &parse_repo,
                    ) {
                        Ok(res) => Outcome::Ok(res),
                        Err(e) => Outcome::Err((file_path.clone(), e.to_string())),
                    }
                })
                .collect();

            let mut ok_results = Vec::with_capacity(outcomes.len());
            let mut errors: Vec<FileErr> = Vec::new();
            let mut skipped: Vec<(PathBuf, String)> = Vec::new();
            for o in outcomes {
                match o {
                    Outcome::Ok(r) => ok_results.push(r),
                    Outcome::Err(e) => errors.push(e),
                    Outcome::Skipped(p, m) => skipped.push((p, m)),
                }
            }
            (total, ok_results, errors, skipped)
        })
        .await;

        let (total, ok_results, errors, skipped) = match join_result {
            Ok(tup) => tup,
            Err(join_err) => {
                // spawn_blocking itself failed (panic or cancellation).
                // Surface this via state — previously `.unwrap_or_default()`
                // silently swallowed it.
                state
                    .push_error(
                        PathBuf::from("<phase1 worker>"),
                        format!("spawn_blocking join error: {join_err}"),
                    )
                    .await;
                return Vec::new();
            }
        };

        state.set_total(total).await;
        for (path, msg) in &skipped {
            // Unreadable files — count separately from parse errors so
            // `index_status` can distinguish "permission denied" from
            // "tree-sitter rejected the input".
            tracing::debug!("Skipped {}: {}", path.display(), msg);
            state.inc_skipped().await;
        }
        for (path, err) in errors {
            state.push_error(path, err).await;
        }
        ok_results
    }

    // ─── Phase 2: batch insert ───────────────────────────────────

    async fn phase2_insert_results(
        &self,
        builder: &GraphBuilder,
        results: Vec<ParseResult>,
        state: &IndexState,
    ) -> usize {
        let mut file_count = 0;
        for (entities, relations) in results {
            if let Err(e) = builder.insert_entities(&entities).await {
                tracing::warn!("Entity insert failed: {e}");
                state
                    .push_error(PathBuf::from("<phase2 entities>"), e.to_string())
                    .await;
            }
            if let Err(e) = builder.insert_relations(&relations).await {
                tracing::warn!("Relation insert failed: {e}");
                state
                    .push_error(PathBuf::from("<phase2 relations>"), e.to_string())
                    .await;
            }
            file_count += 1;
            state.inc_done().await;
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

    // ─── Phase 5.5: knowledge source watcher ──────────────────────

    fn phase55_watch_knowledge_sources(&self) {
        let raw_dir = self.path.join(".raw");
        if !raw_dir.exists() {
            tracing::debug!("No .raw/ directory — skipping knowledge source watcher");
            return;
        }

        match watcher::start_watcher(&raw_dir) {
            Ok(rx) => {
                let db = self.db.clone();
                let repo = self.repo.clone();
                tokio::spawn(async move {
                    let mut debounce_rx = rx;
                    while let Some(_changed_files) = debounce_rx.recv().await {
                        tracing::info!("Knowledge source change detected in .raw/ — notify agent to /wiki-ingest");
                        // Log the event so the agent picks it up on next query
                        let _ = db
                            .query(format!(
                                "UPSERT knowledge:raw_change_pending SET \
                                 title = 'Pending source changes in .raw/', \
                                 content = 'New or modified files detected in .raw/ directory. Run /wiki-ingest to process them.', \
                                 kind = 'notification', \
                                 repo = '{}', \
                                 updated_at = time::now()",
                                repo
                            ))
                            .await;
                    }
                });
                tracing::info!("Knowledge source watcher active on .raw/");
            }
            Err(e) => {
                tracing::debug!("Knowledge source watcher skipped: {}", e);
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

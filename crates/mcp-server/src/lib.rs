pub mod daemon;
pub mod helpers;
pub mod params;
pub mod server;
pub mod watcher;

pub use server::GraphRagServer;

use anyhow::Result;
use std::path::PathBuf;

use rmcp::ServiceExt;

/// Run MCP server in stdio mode — single project, one process.
/// This is the main entry point used by both the standalone binary and the unified CLI.
pub async fn run_stdio(path: PathBuf, repo: Option<String>, auto_index: bool) -> Result<()> {
    // Debug log to file (always, for troubleshooting MCP startup)
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file = log_dir.join("mcp-debug.log");
    let _ = std::fs::write(&log_file, format!(
        "[{}] Starting codescope-mcp\n  path: {:?}\n  repo: {:?}\n  auto_index: {}\n  cwd: {:?}\n  pid: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        path, repo, auto_index,
        std::env::current_dir().ok(),
        std::process::id(),
    ));

    let repo_name = repo.unwrap_or_else(|| {
        path.canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".into())
    });

    let db_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db")
        .join(&repo_name);

    // Append to debug log
    let _ = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_file)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "  repo_name: {}\n  db_path: {:?}", repo_name, db_path)
        });

    tracing::info!(
        "Stdio mode: repo '{}', db: {}",
        repo_name,
        db_path.display()
    );

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = match surrealdb::Surreal::new::<surrealdb::engine::local::SurrealKv>(
        db_path.to_string_lossy().as_ref(),
    )
    .await
    {
        Ok(db) => db,
        Err(e) => {
            let _ = std::fs::OpenOptions::new()
                .append(true)
                .open(&log_file)
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "  DB ERROR: {}", e)
                });
            return Err(e.into());
        }
    };
    db.use_ns("codescope").use_db(&repo_name).await?;
    codescope_core::graph::schema::init_schema(&db).await?;

    let _ = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_file)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "  DB connected, MCP serving...")
        });

    // Create MCP server BEFORE spawning background tasks so we can share context_summary
    let mcp_server = server::GraphRagServer::new(db.clone(), repo_name.clone(), path.clone());

    // Background auto-index with parallel parsing
    if auto_index {
        let index_db = db.clone();
        let index_path = path.clone();
        let index_repo = repo_name.clone();
        let mcp_handle = mcp_server.clone();
        tokio::spawn(async move {
            tracing::info!("Background indexing {}...", index_path.display());
            let builder = codescope_core::graph::builder::GraphBuilder::new(index_db.clone());

            // Phase 1: Collect + parse files in parallel (CPU-bound, rayon thread pool)
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

            // Phase 2: Batch insert results (async DB operations)
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

            tracing::info!("Background indexing complete: {} files", file_count);

            // Phase 2.5: Resolve cross-file call targets
            match builder.resolve_call_targets(&index_repo).await {
                Ok(resolved) if resolved > 0 => {
                    tracing::info!("Resolved {} cross-file call targets", resolved);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("Call target resolution failed: {}", e),
            }

            // Phase 2.6: Resolve virtual dispatch for OOP languages (C#, Java)
            match builder.resolve_virtual_dispatch(&index_repo).await {
                Ok(resolved) if resolved > 0 => {
                    tracing::info!("Resolved {} virtual dispatch edges", resolved);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("Virtual dispatch resolution failed: {}", e),
            }

            // Phase 2.7: Sync git history for hotspot/churn analysis
            {
                let git_path = index_path.clone();
                let commits = tokio::task::spawn_blocking(move || {
                    codescope_core::temporal::GitAnalyzer::open(&git_path)
                        .and_then(|a| a.recent_commits(500))
                })
                .await
                .unwrap_or_else(|_| Ok(vec![]));

                match commits {
                    Ok(ref c) if !c.is_empty() => {
                        let git_sync =
                            codescope_core::temporal::TemporalGraphSync::new(index_db.clone());
                        match git_sync.sync_commit_data(c, &index_repo).await {
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

            // Phase 3: Auto-index conversations + memory files
            let project_dir = helpers::find_claude_project_dir(&index_path, &index_repo);
            tracing::info!("Auto-indexing conversations from {}", project_dir.display());

            let known_entities: Vec<String> = Vec::new();

            let mut jsonl_files = Vec::new();
            collect_jsonl_files(&project_dir, &mut jsonl_files);

            let mut conv_count = 0;
            for jsonl_path in &jsonl_files {
                match codescope_core::conversation::parse_conversation(
                    jsonl_path,
                    &index_repo,
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

            // Index memory files
            let memory_dir = project_dir.join("memory");
            let mut mem_count = 0;
            if memory_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&memory_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map(|e| e == "md").unwrap_or(false) {
                            if let Ok((ents, rels)) =
                                codescope_core::conversation::parse_memory_file(
                                    &path,
                                    &index_repo,
                                    &known_entities,
                                )
                            {
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

            // Phase 4: Generate CONTEXT.md + load context summary into MCP server
            helpers::generate_context_md(&index_db, &index_repo, &index_path).await;
            mcp_handle.load_context_summary().await;

            tracing::info!("Context summary loaded into MCP server instructions");

            // Phase 4.5: Auto-embed functions for semantic search
            match codescope_core::embeddings::FastEmbedProvider::new() {
                Ok(provider) => {
                    let pipeline = codescope_core::embeddings::EmbeddingPipeline::new(
                        index_db.clone(),
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

            // Phase 5: Start file watcher for live re-indexing
            match watcher::start_watcher(&index_path) {
                Ok(rx) => {
                    watcher::spawn_reindex_task(
                        rx,
                        index_db.clone(),
                        index_repo.clone(),
                        index_path.clone(),
                    );
                    tracing::info!("File watcher active — changes will auto-reindex");
                }
                Err(e) => {
                    tracing::warn!("File watcher failed to start: {}", e);
                }
            }

            // Phase 6: Periodic conversation re-indexing (every 5 minutes)
            let conv_db = index_db.clone();
            let conv_repo = index_repo.clone();
            let conv_path = index_path.clone();
            let conv_mcp = mcp_handle.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                interval.tick().await; // skip first immediate tick
                loop {
                    interval.tick().await;
                    tracing::debug!("Periodic conversation re-index...");

                    let project_dir = helpers::find_claude_project_dir(&conv_path, &conv_repo);
                    let builder =
                        codescope_core::graph::builder::GraphBuilder::new(conv_db.clone());
                    let known: Vec<String> = Vec::new();

                    let mut jsonl_files = Vec::new();
                    collect_jsonl_files(&project_dir, &mut jsonl_files);

                    let mut new_count = 0;
                    for jsonl_path in &jsonl_files {
                        // Check hash to skip already-indexed files
                        let fname = jsonl_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("");
                        if let Ok(Some(_)) = helpers::check_conversation_hash(&conv_db, fname).await
                        {
                            continue; // already indexed
                        }

                        if let Ok((entities, relations, _)) =
                            codescope_core::conversation::parse_conversation(
                                jsonl_path, &conv_repo, &known,
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
                        helpers::generate_context_md(&conv_db, &conv_repo, &conv_path).await;
                        conv_mcp.load_context_summary().await;
                    }
                }
            });
        });
    }

    let service = mcp_server.serve(rmcp::transport::stdio()).await?;
    tracing::info!("MCP server running on stdio");
    service.waiting().await?;

    Ok(())
}

/// Recursively collect all .jsonl files in a directory
pub fn collect_jsonl_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_jsonl_files(&path, out);
            } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                out.push(path);
            }
        }
    }
}

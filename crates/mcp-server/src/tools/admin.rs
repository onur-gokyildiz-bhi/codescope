//! Admin tools: init_project, list_projects, index_codebase.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use std::path::PathBuf;

use crate::params::*;
use crate::server::{GraphRagServer, ProjectCtx};

#[tool_router(router = admin_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Initialize a project for this session (daemon mode). Opens the DB and optionally indexes the codebase.
    #[tool(
        description = "Initialize a project for this session. Required in daemon mode before using other tools. Pass the repo name and codebase path."
    )]
    async fn init_project(&self, Parameters(params): Parameters<InitProjectParams>) -> String {
        if self.is_stdio_mode() {
            return "Project already initialized (stdio mode).".into();
        }
        let daemon = match self.daemon() {
            Some(d) => d.clone(),
            None => {
                return "Daemon state not available.".into();
            }
        };

        let db = match daemon.get_db(&params.repo).await {
            Ok(db) => db,
            Err(e) => return format!("Failed to open DB for '{}': {}", params.repo, e),
        };

        let codebase_path = PathBuf::from(&params.path);
        let repo_name = params.repo.clone();

        *self.project_lock().write().await = Some(ProjectCtx {
            db: db.clone(),
            repo_name: repo_name.clone(),
            codebase_path: codebase_path.clone(),
        });

        if params.auto_index.unwrap_or(false) {
            let index_repo = repo_name.clone();
            let index_path = codebase_path.clone();
            tokio::spawn(async move {
                tracing::info!("Background indexing {}...", index_path.display());
                let builder = codescope_core::graph::builder::GraphBuilder::new(db);

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
                                .parse_source(
                                    std::path::Path::new(&rel_path),
                                    &content,
                                    &parse_repo,
                                )
                                .ok()
                        })
                        .collect::<Vec<_>>()
                })
                .await
                .unwrap_or_default();

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
            });
        }

        format!(
            "Project '{}' initialized at {}. DB ready.",
            repo_name,
            codebase_path.display()
        )
    }

    /// List all projects currently open in the daemon
    #[tool(
        description = "List all projects currently open in the daemon. Only available in daemon mode."
    )]
    async fn list_projects(&self) -> String {
        if self.is_stdio_mode() {
            let ctx = self.project_lock().read().await;
            return match &*ctx {
                Some(c) => format!("Stdio mode — project: {}", c.repo_name),
                None => "No project initialized.".into(),
            };
        }
        match self.daemon() {
            Some(d) => {
                let repos = d.active_repos().await;
                if repos.is_empty() {
                    "No projects open. Call init_project first.".into()
                } else {
                    format!("Open projects: {}", repos.join(", "))
                }
            }
            None => "Daemon state not available.".into(),
        }
    }

    /// Index or re-index the codebase into the graph database
    #[tool(
        description = "Index the codebase into the knowledge graph. Parses source files and extracts entities and relationships."
    )]
    async fn index_codebase(&self, Parameters(params): Parameters<IndexParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let target_path = match &params.path {
            Some(p) => ctx.codebase_path.join(p),
            None => ctx.codebase_path.clone(),
        };

        let parser = codescope_core::parser::CodeParser::new();
        let builder = codescope_core::graph::builder::GraphBuilder::new(ctx.db.clone());
        let incremental =
            codescope_core::graph::incremental::IncrementalIndexer::new(ctx.db.clone());

        let clean = params.clean.unwrap_or(false);
        if clean {
            if let Err(e) = builder.clear_repo(&ctx.repo_name).await {
                return format!("Error clearing repo: {}", e);
            }
        }

        let existing_hashes = if !clean {
            incremental
                .load_file_hashes(&ctx.repo_name)
                .await
                .unwrap_or_default()
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

            if !clean {
                let current_hash = codescope_core::graph::incremental::hash_content(&content);
                if existing_hashes.get(&rel_path).map(|h| h.as_str()) == Some(&current_hash) {
                    files_skipped += 1;
                    continue;
                }
                let _ = builder
                    .delete_file_entities(&rel_path, &ctx.repo_name)
                    .await;
            }

            match parser.parse_source(std::path::Path::new(&rel_path), &content, &ctx.repo_name) {
                Ok((ents, rels)) => {
                    entities += ents.len();
                    relations += rels.len();
                    if let Err(e) = builder.insert_entities(&ents).await {
                        tracing::warn!("Entity insert failed: {e}");
                    }
                    if let Err(e) = builder.insert_relations(&rels).await {
                        tracing::warn!("Relation insert failed: {e}");
                    }
                    files_indexed += 1;
                }
                Err(e) => {
                    errors.push(format!("{}: {}", file_path.display(), e));
                }
            }
        }

        let deleted = if !clean {
            incremental
                .cleanup_deleted_files(&target_path, &ctx.repo_name)
                .await
                .unwrap_or(0)
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
}

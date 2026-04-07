//! File watcher — monitors codebase for changes and triggers incremental re-index.
//! Uses `notify` with debouncing (2s) to avoid rapid-fire re-indexes.

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Start watching a directory for file changes.
/// Returns a channel receiver that emits batches of changed file paths.
pub fn start_watcher(codebase_path: &Path) -> anyhow::Result<mpsc::Receiver<Vec<PathBuf>>> {
    let (tx, rx) = mpsc::channel(50); // Bounded channel — backpressure at 50 pending batches
    let watch_path = codebase_path.to_path_buf();

    std::thread::spawn(move || {
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();

        let mut debouncer = match new_debouncer(std::time::Duration::from_secs(2), notify_tx) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = debouncer
            .watcher()
            .watch(&watch_path, notify::RecursiveMode::Recursive)
        {
            warn!("Failed to watch {}: {}", watch_path.display(), e);
            return;
        }

        info!("File watcher started for {}", watch_path.display());

        loop {
            match notify_rx.recv() {
                Ok(Ok(events)) => {
                    let changed: Vec<PathBuf> = events
                        .iter()
                        .filter(|e| matches!(e.kind, DebouncedEventKind::Any))
                        .filter(|e| is_indexable_file(&e.path)) // Filter BEFORE clone
                        .map(|e| e.path.clone())
                        .collect();

                    if !changed.is_empty() {
                        debug!("File watcher: {} files changed", changed.len());
                        if tx.blocking_send(changed).is_err() {
                            break; // receiver dropped
                        }
                    }
                }
                Ok(Err(e)) => {
                    debug!("Watch error: {:?}", e);
                }
                Err(_) => break, // channel closed
            }
        }

        info!("File watcher stopped");
    });

    Ok(rx)
}

/// Spawn a background task that listens for file changes and re-indexes them.
pub fn spawn_reindex_task(
    mut rx: mpsc::Receiver<Vec<PathBuf>>,
    db: surrealdb::Surreal<surrealdb::engine::local::Db>,
    repo_name: String,
    codebase_path: PathBuf,
) {
    tokio::spawn(async move {
        let parser = codescope_core::parser::CodeParser::new();
        let builder = codescope_core::graph::builder::GraphBuilder::new(db.clone());
        let incremental = codescope_core::graph::incremental::IncrementalIndexer::new(db.clone());

        // Load hashes ONCE at startup — update in-memory cache after each re-index
        let mut cached_hashes = incremental
            .load_file_hashes(&repo_name)
            .await
            .unwrap_or_default();

        while let Some(changed_files) = rx.recv().await {
            let mut indexed = 0;
            let mut skipped = 0;
            let mut all_entities = Vec::with_capacity(changed_files.len() * 10);
            let mut all_relations = Vec::with_capacity(changed_files.len() * 5);

            for file_path in &changed_files {
                let content = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let rel_path = file_path
                    .strip_prefix(&codebase_path)
                    .unwrap_or(file_path)
                    .to_string_lossy()
                    .to_string()
                    .replace('\\', "/");

                // Check cached hash — no DB round-trip
                let current_hash = codescope_core::graph::incremental::hash_content(&content);
                if cached_hashes.get(&rel_path).map(|h| h.as_str()) == Some(&current_hash) {
                    skipped += 1;
                    continue;
                }

                // Delete old entities
                if let Err(e) = builder.delete_file_entities(&rel_path, &repo_name).await {
                    tracing::warn!("Delete entities failed: {e}");
                }

                match parser.parse_source(std::path::Path::new(&rel_path), &content, &repo_name) {
                    Ok((entities, relations)) => {
                        all_entities.extend(entities);
                        all_relations.extend(relations);
                        // Update in-memory cache
                        cached_hashes.insert(rel_path, current_hash);
                        indexed += 1;
                    }
                    Err(e) => {
                        debug!("Re-index error {}: {}", rel_path, e);
                    }
                }
            }

            // Batch insert all parsed entities/relations at once
            if !all_entities.is_empty() {
                if let Err(e) = builder.insert_entities(&all_entities).await {
                    tracing::warn!("Entity insert failed: {e}");
                }
                if let Err(e) = builder.insert_relations(&all_relations).await {
                    tracing::warn!("Relation insert failed: {e}");
                }
            }

            if indexed > 0 {
                info!(
                    "File watcher re-indexed {} files ({} unchanged)",
                    indexed, skipped
                );
            }
        }
    });
}

/// Check if a file should trigger re-indexing
fn is_indexable_file(path: &Path) -> bool {
    // Skip hidden, build artifacts, lock files — check components, not string contains
    for component in path.components() {
        let s = component.as_os_str().to_string_lossy();
        if s == ".git"
            || s == "target"
            || s == "node_modules"
            || s == ".next"
            || s == "build"
            || s == "dist"
            || s == "__pycache__"
        {
            return false;
        }
    }

    // Check supported extensions using static set (no parser allocation)
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Fast extension check — covers 99% of cases
    static INDEXABLE_EXTS: &[&str] = &[
        "rs",
        "ts",
        "tsx",
        "js",
        "jsx",
        "py",
        "go",
        "java",
        "rb",
        "c",
        "cpp",
        "h",
        "hpp",
        "cs",
        "swift",
        "kt",
        "scala",
        "vue",
        "svelte",
        "css",
        "scss",
        "html",
        "json",
        "yaml",
        "yml",
        "toml",
        "xml",
        "md",
        "dockerfile",
        "tf",
        "hcl",
        "sql",
        "graphql",
        "proto",
        "sh",
        "bash",
        "zsh",
        "ps1",
        "bat",
    ];
    static INDEXABLE_NAMES: &[&str] = &[
        "Dockerfile",
        "Makefile",
        "Cargo.toml",
        "package.json",
        "tsconfig.json",
        "docker-compose.yml",
        "docker-compose.yaml",
        ".env",
        ".env.example",
    ];

    let ext_lower = ext.to_lowercase();
    (INDEXABLE_EXTS.contains(&ext_lower.as_str()) || INDEXABLE_NAMES.contains(&fname))
        && !codescope_core::parser::should_skip_file(path)
}

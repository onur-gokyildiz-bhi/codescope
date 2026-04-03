//! File watcher — monitors codebase for changes and triggers incremental re-index.
//! Uses `notify` with debouncing (2s) to avoid rapid-fire re-indexes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::mpsc;
use tracing::{info, debug, warn};

/// Start watching a directory for file changes.
/// Returns a channel receiver that emits batches of changed file paths.
pub fn start_watcher(
    codebase_path: &Path,
) -> anyhow::Result<mpsc::UnboundedReceiver<Vec<PathBuf>>> {
    let (tx, rx) = mpsc::unbounded_channel();
    let watch_path = codebase_path.to_path_buf();

    std::thread::spawn(move || {
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();

        let mut debouncer = match new_debouncer(
            std::time::Duration::from_secs(2),
            notify_tx,
        ) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = debouncer.watcher().watch(
            &watch_path,
            notify::RecursiveMode::Recursive,
        ) {
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
                        .map(|e| e.path.clone())
                        .filter(|p| is_indexable_file(p))
                        .collect();

                    if !changed.is_empty() {
                        debug!("File watcher: {} files changed", changed.len());
                        if tx.send(changed).is_err() {
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
    mut rx: mpsc::UnboundedReceiver<Vec<PathBuf>>,
    db: surrealdb::Surreal<surrealdb::engine::local::Db>,
    repo_name: String,
    codebase_path: PathBuf,
) {
    tokio::spawn(async move {
        let parser = codescope_core::parser::CodeParser::new();
        let builder = codescope_core::graph::builder::GraphBuilder::new(db.clone());
        let incremental = codescope_core::graph::incremental::IncrementalIndexer::new(db.clone());

        while let Some(changed_files) = rx.recv().await {
            let mut indexed = 0;
            let mut skipped = 0;

            // Load existing hashes for comparison
            let existing_hashes = incremental
                .load_file_hashes(&repo_name)
                .await
                .unwrap_or_default();

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

                // Check if actually changed
                let current_hash = codescope_core::graph::incremental::hash_content(&content);
                if existing_hashes.get(&rel_path).map(|h| h.as_str()) == Some(&current_hash) {
                    skipped += 1;
                    continue;
                }

                // Delete old entities, re-parse, insert
                let _ = builder.delete_file_entities(&rel_path, &repo_name).await;

                match parser.parse_source(std::path::Path::new(&rel_path), &content, &repo_name) {
                    Ok((entities, relations)) => {
                        let _ = builder.insert_entities(&entities).await;
                        let _ = builder.insert_relations(&relations).await;
                        indexed += 1;
                    }
                    Err(e) => {
                        debug!("Re-index error {}: {}", rel_path, e);
                    }
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
    // Skip hidden, build artifacts, lock files
    let path_str = path.to_string_lossy();
    if path_str.contains("/.git/") || path_str.contains("\\.git\\")
        || path_str.contains("/target/") || path_str.contains("\\target\\")
        || path_str.contains("/node_modules/") || path_str.contains("\\node_modules\\")
        || path_str.contains("/.next/") || path_str.contains("\\.next\\")
    {
        return false;
    }

    // Check supported extensions
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let parser = codescope_core::parser::CodeParser::new();
    (parser.supports_extension(ext) || parser.supports_filename(fname))
        && !codescope_core::parser::should_skip_file(path)
}

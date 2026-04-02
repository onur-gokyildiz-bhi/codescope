use anyhow::Result;
use sha2::{Digest, Sha256};
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use std::path::Path;
use tracing::{debug, info};

/// Handles incremental indexing — only re-parse files that changed
pub struct IncrementalIndexer {
    db: Surreal<Db>,
}

impl IncrementalIndexer {
    pub fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// Check if a file needs re-indexing by comparing content hashes
    pub async fn needs_reindex(&self, file_path: &str, content: &str) -> Result<bool> {
        let current_hash = hash_content(content);
        let path = file_path.to_string();

        let existing: Vec<FileHashRecord> = self
            .db
            .query(
                "SELECT hash FROM file WHERE path = $path LIMIT 1".to_string(),
            )
            .bind(("path", path))
            .await?
            .take(0)?;

        match existing.first() {
            Some(record) => Ok(record.hash.as_deref() != Some(&current_hash)),
            None => Ok(true), // New file, needs indexing
        }
    }

    /// Get list of files that have changed since last index
    pub async fn changed_files(
        &self,
        base_path: &Path,
        repo_name: &str,
        extensions: &[&str],
    ) -> Result<Vec<std::path::PathBuf>> {
        let mut changed = Vec::new();

        let walker = ignore::WalkBuilder::new(base_path)
            .hidden(true)
            .git_ignore(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }

            let file_path = entry.path();
            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            if !extensions.contains(&ext) {
                continue;
            }

            let rel_path = file_path
                .strip_prefix(base_path)
                .unwrap_or(file_path)
                .to_string_lossy()
                .replace('\\', "/");

            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            if self.needs_reindex(&rel_path, &content).await? {
                changed.push(file_path.to_path_buf());
            }
        }

        info!(
            "Incremental check: {} files changed out of total scanned",
            changed.len()
        );

        Ok(changed)
    }

    /// Remove entities from files that no longer exist on disk
    pub async fn cleanup_deleted_files(
        &self,
        base_path: &Path,
        repo_name: &str,
    ) -> Result<usize> {
        let repo = repo_name.to_string();

        let indexed_files: Vec<FilePathRecord> = self
            .db
            .query("SELECT path FROM file WHERE repo = $repo".to_string())
            .bind(("repo", repo.clone()))
            .await?
            .take(0)?;

        let mut deleted = 0;
        for record in &indexed_files {
            let full_path = base_path.join(&record.path);
            if !full_path.exists() {
                debug!("Cleaning up deleted file: {}", record.path);
                let path = record.path.clone();

                // Delete the file and all its contained entities
                let _ = self.db
                    .query(
                        "DELETE FROM `function` WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM class WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM import_decl WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM file WHERE path = $path AND repo = $repo;"
                            .to_string(),
                    )
                    .bind(("path", path))
                    .bind(("repo", repo.clone()))
                    .await;

                deleted += 1;
            }
        }

        if deleted > 0 {
            info!("Cleaned up {} deleted files", deleted);
        }

        Ok(deleted)
    }
}

#[derive(Debug, serde::Deserialize)]
struct FileHashRecord {
    hash: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct FilePathRecord {
    path: String,
}

fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

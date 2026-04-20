use crate::DbHandle;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;
use surrealdb::types::SurrealValue;
use tracing::{debug, info};

/// Handles incremental indexing — only re-parse files that changed
pub struct IncrementalIndexer {
    db: DbHandle,
}

impl IncrementalIndexer {
    pub fn new(db: DbHandle) -> Self {
        Self { db }
    }

    /// Check if a file needs re-indexing by comparing content hashes
    pub async fn needs_reindex(&self, file_path: &str, content: &str) -> Result<bool> {
        let current_hash = hash_content(content);
        let path = file_path.to_string();

        let existing: Vec<FileHashRecord> = self
            .db
            .query("SELECT hash FROM file WHERE path = $path LIMIT 1".to_string())
            .bind(("path", path))
            .await?
            .take(0)?;

        match existing.first() {
            Some(record) => Ok(record.hash.as_deref() != Some(&current_hash)),
            None => Ok(true), // New file, needs indexing
        }
    }

    /// Remove entities from files that no longer exist on disk
    pub async fn cleanup_deleted_files(&self, base_path: &Path, repo_name: &str) -> Result<usize> {
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

                // Delete from ALL tables (including content parser tables)
                let _ = self
                    .db
                    .query(
                        "DELETE FROM `function` WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM class WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM import_decl WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM module WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM variable WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM config WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM doc WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM api WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM db_entity WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM infra WHERE file_path = $path AND repo = $repo; \
                         DELETE FROM package WHERE file_path = $path AND repo = $repo; \
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

    /// Bulk-load all file hashes for a repo (single query instead of N+1).
    /// Returns a map of relative_path -> content_hash for fast local comparison.
    pub async fn load_file_hashes(
        &self,
        repo_name: &str,
    ) -> Result<std::collections::HashMap<String, String>> {
        let repo = repo_name.to_string();
        let records: Vec<FileHashPathRecord> = self
            .db
            .query("SELECT path, hash FROM file WHERE repo = $repo")
            .bind(("repo", repo))
            .await?
            .take(0)?;

        Ok(records
            .into_iter()
            .filter_map(|r| r.hash.map(|h| (r.path, h)))
            .collect())
    }
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct FileHashRecord {
    hash: Option<String>,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct FilePathRecord {
    path: String,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct FileHashPathRecord {
    path: String,
    hash: Option<String>,
}

pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

use anyhow::Result;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use tracing::{debug, info, warn};

use super::git::{GitAnalyzer, CommitInfo, ChangeType};

/// Syncs git history into the SurrealDB graph
pub struct TemporalGraphSync {
    db: Surreal<Db>,
}

impl TemporalGraphSync {
    pub fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// Initialize temporal schema (commit table + modified_in edges)
    pub async fn init_schema(&self) -> Result<()> {
        self.db
            .query(
                "
                DEFINE TABLE IF NOT EXISTS commit SCHEMAFULL;
                DEFINE FIELD IF NOT EXISTS hash ON commit TYPE string;
                DEFINE FIELD IF NOT EXISTS author ON commit TYPE string;
                DEFINE FIELD IF NOT EXISTS message ON commit TYPE string;
                DEFINE FIELD IF NOT EXISTS timestamp ON commit TYPE int;
                DEFINE FIELD IF NOT EXISTS files_changed ON commit TYPE int;
                DEFINE FIELD IF NOT EXISTS repo ON commit TYPE string;
                DEFINE INDEX IF NOT EXISTS commit_hash ON commit FIELDS hash UNIQUE;
                DEFINE INDEX IF NOT EXISTS commit_ts ON commit FIELDS timestamp;
                DEFINE INDEX IF NOT EXISTS commit_author ON commit FIELDS author;
                "
                    .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Sync recent commits from a git repo into the graph
    pub async fn sync_commits(
        &self,
        analyzer: &GitAnalyzer,
        repo_name: &str,
        limit: usize,
    ) -> Result<usize> {
        let commits = analyzer.recent_commits(limit)?;
        self.sync_commit_data(&commits, repo_name).await
    }

    /// Sync pre-fetched commits into the graph (thread-safe, no git2 dependency)
    pub async fn sync_commit_data(
        &self,
        commits: &[CommitInfo],
        repo_name: &str,
    ) -> Result<usize> {
        self.init_schema().await?;

        info!("Syncing {} commits for repo '{}'...", commits.len(), repo_name);

        let mut count = 0;
        for commit in commits {
            // Create commit node
            let hash = commit.hash.clone();
            let author = commit.author.clone();
            let message = commit.message.lines().next().unwrap_or("").to_string();
            let timestamp = commit.timestamp;
            let files_changed = commit.files_changed.len() as i64;
            let repo = repo_name.to_string();

            let result = self.db
                .query(
                    "CREATE commit SET hash = $hash, author = $author, message = $msg, \
                     timestamp = $ts, files_changed = $fc, repo = $repo"
                        .to_string(),
                )
                .bind(("hash", hash.clone()))
                .bind(("author", author))
                .bind(("msg", message))
                .bind(("ts", timestamp))
                .bind(("fc", files_changed))
                .bind(("repo", repo))
                .await;

            if let Err(e) = result {
                // Might be duplicate (UNIQUE constraint)
                debug!("Commit {} already exists or error: {}", &hash[..8], e);
                continue;
            }

            // Create modified_in edges: file -> modified_in -> commit
            for file_change in &commit.files_changed {
                let file_path = file_change.path.clone();
                let change_type = match file_change.change_type {
                    ChangeType::Added => "added".to_string(),
                    ChangeType::Modified => "modified".to_string(),
                    ChangeType::Deleted => "deleted".to_string(),
                    ChangeType::Renamed => "renamed".to_string(),
                };
                let commit_hash = hash.clone();

                let _ = self.db
                    .query(
                        "LET $file = (SELECT * FROM file WHERE path CONTAINS $path LIMIT 1); \
                         LET $commit = (SELECT * FROM commit WHERE hash = $hash LIMIT 1); \
                         IF $file AND $commit THEN \
                             RELATE ($file[0].id)->modified_in->($commit[0].id) \
                             SET change_type = $ct, timestamp = $ts \
                         END;"
                            .to_string(),
                    )
                    .bind(("path", file_path))
                    .bind(("hash", commit_hash))
                    .bind(("ct", change_type))
                    .bind(("ts", commit.timestamp))
                    .await;
            }

            count += 1;
        }

        info!("Synced {} commits", count);
        Ok(count)
    }

    /// Calculate hotspot scores: complexity * churn = risk
    pub async fn calculate_hotspots(&self, repo_name: &str) -> Result<Vec<HotspotEntry>> {
        let repo = repo_name.to_string();

        // Get functions with their change frequency (churn)
        let results: Vec<HotspotEntry> = self.db
            .query(
                "SELECT name, file_path, start_line, end_line, \
                 (end_line - start_line) AS size, \
                 count(->modified_in) AS churn, \
                 ((end_line - start_line) * count(->modified_in)) AS risk_score \
                 FROM `function` WHERE repo = $repo \
                 ORDER BY risk_score DESC LIMIT 30"
                    .to_string(),
            )
            .bind(("repo", repo))
            .await?
            .take(0)?;

        Ok(results)
    }

    /// Get the evolution of a specific entity over time
    pub async fn entity_evolution(
        &self,
        entity_name: &str,
    ) -> Result<Vec<EvolutionEntry>> {
        let name = entity_name.to_string();

        let results: Vec<EvolutionEntry> = self.db
            .query(
                "SELECT ->modified_in->commit.{hash, author, message, timestamp} AS commits \
                 FROM file WHERE path CONTAINS $name"
                    .to_string(),
            )
            .bind(("name", name))
            .await?
            .take(0)?;

        Ok(results)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct HotspotEntry {
    pub name: Option<String>,
    pub file_path: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub size: Option<i64>,
    pub churn: Option<i64>,
    pub risk_score: Option<i64>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EvolutionEntry {
    pub commits: Option<serde_json::Value>,
}

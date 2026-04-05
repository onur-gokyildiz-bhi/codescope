use anyhow::Result;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use tracing::{debug, info};

use super::{CommitInfo, ChangeType};

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
                ",
            )
            .await?;
        Ok(())
    }

    /// Sync recent commits from a git repo into the graph
    pub async fn sync_commits(
        &self,
        analyzer: &super::GitAnalyzer,
        repo_name: &str,
        limit: usize,
    ) -> Result<usize> {
        let commits = analyzer.recent_commits(limit)?;
        self.sync_commit_data(&commits, repo_name).await
    }

    /// Sync pre-fetched commits into the graph (thread-safe, no git2 dependency).
    /// Idempotent: uses UPSERT for commits and checks for existing edges before creating.
    pub async fn sync_commit_data(
        &self,
        commits: &[CommitInfo],
        repo_name: &str,
    ) -> Result<usize> {
        self.init_schema().await?;

        info!("Syncing {} commits for repo '{}'...", commits.len(), repo_name);

        // Phase 1: Batch UPSERT all commits in a single query
        let mut commit_query = String::with_capacity(commits.len() * 200);
        for commit in commits {
            let sanitized_hash = commit.hash.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
            let message_escaped = commit.message.lines().next().unwrap_or("")
                .replace('\\', "\\\\").replace('\'', "\\'");
            let author_escaped = commit.author.replace('\\', "\\\\").replace('\'', "\\'");

            commit_query.push_str(&format!(
                "UPSERT commit:{} SET hash = '{}', author = '{}', message = '{}', \
                 timestamp = {}, files_changed = {}, repo = '{}';\n",
                sanitized_hash, commit.hash, author_escaped, message_escaped,
                commit.timestamp, commit.files_changed.len(), repo_name
            ));
        }

        if !commit_query.is_empty() {
            if let Err(e) = self.db.query(&commit_query).await {
                debug!("Batch commit upsert error: {}", e);
            }
        }

        // Phase 2: Batch create modified_in edges — one query per batch of file changes
        let mut edge_query = String::with_capacity(commits.len() * 500);
        let mut count = 0;
        for commit in commits {
            for file_change in &commit.files_changed {
                let ct = match file_change.change_type {
                    ChangeType::Added => "added",
                    ChangeType::Modified => "modified",
                    ChangeType::Deleted => "deleted",
                    ChangeType::Renamed => "renamed",
                };
                let path_escaped = file_change.path.replace('\\', "\\\\").replace('\'', "\\'");
                let sanitized_hash = commit.hash.replace(|c: char| !c.is_ascii_alphanumeric(), "_");

                edge_query.push_str(&format!(
                    "LET $f = (SELECT id FROM file WHERE path CONTAINS '{}' LIMIT 1); \
                     IF $f THEN \
                         RELATE ($f[0].id)->modified_in->(commit:{}) \
                         SET change_type = '{}', timestamp = {} \
                     END;\n",
                    path_escaped, sanitized_hash, ct, commit.timestamp
                ));
            }
            count += 1;

            // Flush in batches of 50 commits to avoid query size limits
            if count % 50 == 0 && !edge_query.is_empty() {
                if let Err(e) = self.db.query(&edge_query).await {
                    debug!("Batch edge insert error: {}", e);
                }
                edge_query.clear();
            }
        }

        // Flush remaining edges
        if !edge_query.is_empty() {
            if let Err(e) = self.db.query(&edge_query).await {
                debug!("Batch edge insert error: {}", e);
            }
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

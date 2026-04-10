use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

/// Shared daemon state — manages DB connections for all projects.
/// Each project gets its own SurrealKv directory under `base_db_path/<repo>/`.
pub struct DaemonState {
    dbs: tokio::sync::RwLock<HashMap<String, Surreal<Db>>>,
    base_db_path: PathBuf,
}

impl DaemonState {
    pub fn new(base_db_path: PathBuf) -> Self {
        Self {
            dbs: tokio::sync::RwLock::new(HashMap::new()),
            base_db_path,
        }
    }

    /// Create a DaemonState pre-populated with a single project — used by stdio mode
    /// so both stdio and daemon modes share the same DB-management codepath.
    pub fn with_initial(base_db_path: PathBuf, repo_name: String, db: Surreal<Db>) -> Self {
        let mut map = HashMap::new();
        map.insert(repo_name, db);
        Self {
            dbs: tokio::sync::RwLock::new(map),
            base_db_path,
        }
    }

    /// Get or create a DB connection for a repo.
    /// Each repo has its own SurrealKv directory — no lock contention.
    pub async fn get_db(&self, repo_name: &str) -> Result<Surreal<Db>> {
        // Check cache first
        {
            let dbs = self.dbs.read().await;
            if let Some(db) = dbs.get(repo_name) {
                return Ok(db.clone());
            }
        }

        // Open new DB for this repo
        let db_path = self.base_db_path.join(repo_name);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        tracing::info!(
            "Opening DB for repo '{}' at {}",
            repo_name,
            db_path.display()
        );

        let db =
            Surreal::new::<surrealdb::engine::local::SurrealKv>(db_path.to_string_lossy().as_ref())
                .await?;
        db.use_ns("codescope").use_db(repo_name).await?;
        crate::graph::schema::init_schema(&db).await?;

        // Cache it
        self.dbs
            .write()
            .await
            .insert(repo_name.to_string(), db.clone());

        Ok(db)
    }

    /// Discover all repos available on disk (scans base_db_path for directories).
    /// Skips hidden directories (starting with `.`).
    pub fn discover_repos(&self) -> Vec<String> {
        let mut repos = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.base_db_path) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.starts_with('.') {
                            repos.push(name.to_string());
                        }
                    }
                }
            }
        }
        repos.sort();
        repos
    }

    /// List repos that are currently loaded (active DB connections).
    pub async fn active_repos(&self) -> Vec<String> {
        self.dbs.read().await.keys().cloned().collect()
    }
}

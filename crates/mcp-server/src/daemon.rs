use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

/// Shared daemon state — manages DB connections for all projects.
/// Each project gets its own RocksDB directory under ~/.codescope/db/<repo>/.
pub struct DaemonState {
    dbs: tokio::sync::RwLock<HashMap<String, Surreal<Db>>>,
    base_db_path: PathBuf,
}

impl DaemonState {
    pub fn new() -> Self {
        let base = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codescope")
            .join("db");
        Self {
            dbs: tokio::sync::RwLock::new(HashMap::new()),
            base_db_path: base,
        }
    }

    /// Get or create a DB connection for a repo.
    /// Each repo has its own RocksDB directory — no lock contention.
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

        tracing::info!("Opening DB for repo '{}' at {}", repo_name, db_path.display());

        let db = Surreal::new::<surrealdb::engine::local::RocksDb>(
            db_path.to_string_lossy().as_ref(),
        )
        .await?;
        db.use_ns("codescope").use_db(repo_name).await?;
        codescope_core::graph::schema::init_schema(&db).await?;

        // Cache it
        self.dbs.write().await.insert(repo_name.to_string(), db.clone());

        Ok(db)
    }

    /// List all open repos
    pub async fn list_repos(&self) -> Vec<String> {
        self.dbs.read().await.keys().cloned().collect()
    }
}

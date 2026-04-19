use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{connect_repo, DbHandle};

/// Shared daemon state — manages DB connections for all projects.
///
/// Post R1-v2 this holds SurrealDB **remote** clients; the `surreal` server
/// is the sole owner of on-disk data at `~/.codescope/surreal-data/`. The
/// `base_db_path` is retained purely so `discover_repos()` can still walk
/// the legacy per-repo directories (used by migration + backward compat).
pub struct DaemonState {
    dbs: tokio::sync::RwLock<HashMap<String, DbHandle>>,
    base_db_path: PathBuf,
}

impl DaemonState {
    pub fn new(base_db_path: PathBuf) -> Self {
        Self {
            dbs: tokio::sync::RwLock::new(HashMap::new()),
            base_db_path,
        }
    }

    /// Create a DaemonState pre-populated with a single project — used by
    /// stdio mode so both stdio and daemon modes share the same
    /// DB-management codepath.
    pub fn with_initial(base_db_path: PathBuf, repo_name: String, db: DbHandle) -> Self {
        let mut map = HashMap::new();
        map.insert(repo_name, db);
        Self {
            dbs: tokio::sync::RwLock::new(map),
            base_db_path,
        }
    }

    /// Get or open a DB connection for a repo.
    ///
    /// All repos live inside the single `surreal` server under NS=`codescope`,
    /// DB=<repo>. No filesystem lock contention — the server handles it.
    pub async fn get_db(&self, repo_name: &str) -> Result<DbHandle> {
        // Cache check
        {
            let dbs = self.dbs.read().await;
            if let Some(db) = dbs.get(repo_name) {
                return Ok(db.clone());
            }
        }

        tracing::info!("Opening remote DB client for repo '{}'", repo_name);
        let db = connect_repo(repo_name).await?;
        crate::graph::schema::init_schema(&db).await?;

        self.dbs
            .write()
            .await
            .insert(repo_name.to_string(), db.clone());

        Ok(db)
    }

    /// Discover repos available on disk (legacy per-repo dirs under
    /// `base_db_path`). Kept for migration + backward compat; once every
    /// consumer uses `get_db` directly this can go away in favour of
    /// asking the server via `INFO FOR KV`.
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

    /// List repos that currently have open (cached) connections.
    pub async fn active_repos(&self) -> Vec<String> {
        self.dbs.read().await.keys().cloned().collect()
    }

    /// Ask the bundled surreal server which databases exist inside
    /// `NS=codescope`. Returns an empty list on any failure — callers can
    /// then fall back to [`discover_repos`] (filesystem walk). Intended
    /// for the daemon's `/mcp/{repo}` route planning.
    pub async fn list_server_repos(&self) -> Vec<String> {
        use serde_json::Value;
        let Ok(admin) = crate::connect_admin().await else {
            return Vec::new();
        };
        let ns = std::env::var("CODESCOPE_DB_NS").unwrap_or_else(|_| crate::DEFAULT_NS.to_string());
        if admin.use_ns(&ns).await.is_err() {
            return Vec::new();
        }
        let Ok(mut resp) = admin.query("INFO FOR NS").await else {
            return Vec::new();
        };
        let Ok(rows): Result<Vec<Value>, _> = resp.take(0) else {
            return Vec::new();
        };
        let mut out: Vec<String> = rows
            .into_iter()
            .filter_map(|row| {
                let dbs = row
                    .get("databases")
                    .or_else(|| row.get("db"))?
                    .as_object()?;
                Some(dbs.keys().cloned().collect::<Vec<_>>())
            })
            .flatten()
            // Filter artefacts:
            // * `.old.<ts>` — surreal auto-creates a DB on
            //   `use_db`, so pre-R7 dry-runs accidentally created
            //   empty backup-named DBs; hide them.
            // * `__spike_test_*` — integration-test leftovers.
            .filter(|n| !n.contains(".old.") && !n.ends_with(".old") && !n.starts_with("__"))
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

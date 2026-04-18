//! Database connection helpers shared by all CLI commands.
//!
//! Post R1-v2 the CLI no longer opens SurrealKV files itself; it hands off
//! to the bundled `surreal` server via [`codescope_core::connect_repo`].
//! The old lock-detection / stale-LOCK recovery logic is gone on purpose
//! — the lock contention it worked around is impossible by construction
//! now (single DB owner = the server).

use anyhow::{Context, Result};
use codescope_core::graph::{migrations, schema};
use codescope_core::{connect_repo, DbHandle};
use std::path::PathBuf;

/// Central per-repo directory — kept for the legacy filesystem layout that
/// R7 migrates into the unified server. Not used to open DBs anymore.
pub fn default_db_path(repo_name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db")
        .join(repo_name)
}

/// Connect to the repo's database inside the shared `surreal` server.
///
/// `_db_path` is accepted only for call-site compatibility during the
/// migration; it is ignored. If the server isn't running, the caller gets
/// a `codescope start` hint in the error surfaced by `connect_repo`.
pub async fn connect_db(_db_path: Option<PathBuf>, repo_name: &str) -> Result<DbHandle> {
    let db = connect_repo(repo_name)
        .await
        .with_context(|| format!("cannot open database for repo '{repo_name}'"))?;
    schema::init_schema(&db).await?;
    migrations::migrate_to_current(&db).await?;
    Ok(db)
}

//! Database connection helpers shared by all CLI commands.

use anyhow::Result;
use codescope_core::graph::schema;
use std::path::PathBuf;

/// Central DB path: ~/.codescope/db/<repo_name>/
pub fn default_db_path(repo_name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db")
        .join(repo_name)
}

pub async fn connect_db(
    db_path: Option<PathBuf>,
    repo_name: &str,
) -> Result<surrealdb::Surreal<surrealdb::engine::local::Db>> {
    let path = db_path.unwrap_or_else(|| default_db_path(repo_name));

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Migrate from old RocksDB format if needed
    // RocksDB has a "CURRENT" file; SurrealKV has "manifest" + "LOCK" — don't confuse them
    if path.join("CURRENT").exists() && !path.join("manifest").exists() {
        let backup = path.with_extension("rocksdb.bak");
        eprintln!(
            "⚠ Old RocksDB data detected at {}.\n  Moving to {} — will re-index with SurrealKV.",
            path.display(),
            backup.display()
        );
        let _ = std::fs::rename(&path, &backup);
        std::fs::create_dir_all(&path)?;
    }

    let db = match surrealdb::Surreal::new::<surrealdb::engine::local::SurrealKv>(
        path.to_string_lossy().as_ref(),
    )
    .await
    {
        Ok(db) => db,
        Err(e) => {
            anyhow::bail!(
                "Failed to open database at {}.\n\
                 Error: {}\n\
                 \n\
                 Try re-indexing or removing the DB directory:\n\
                 rm -rf {}",
                path.display(),
                e,
                path.display()
            );
        }
    };

    db.use_ns("codescope").use_db(repo_name).await?;
    schema::init_schema(&db).await?;

    Ok(db)
}

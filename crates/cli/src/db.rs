//! Database connection helpers shared by all CLI commands.

use anyhow::Result;
use codescope_core::graph::{migrations, schema};
use std::path::{Path, PathBuf};

/// Central DB path: ~/.codescope/db/<repo_name>/
pub fn default_db_path(repo_name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db")
        .join(repo_name)
}

/// Check if any codescope process is currently running.
fn is_codescope_running() -> bool {
    #[cfg(windows)]
    {
        // Windows: use tasklist
        match std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq codescope.exe", "/FO", "CSV", "/NH"])
            .output()
        {
            Ok(output) => {
                let s = String::from_utf8_lossy(&output.stdout);
                // tasklist with no matches returns "INFO: No tasks..."
                s.contains("codescope")
            }
            Err(_) => {
                // If tasklist missing, err on the safe side — say process IS running
                // so we don't accidentally remove a live lock
                true
            }
        }
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("pgrep")
            .args(["-f", "codescope"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Try to remove a stale LOCK file if the owning process is dead.
/// Returns true if lock was removed (safe to retry).
fn try_remove_stale_lock(db_path: &Path) -> bool {
    let lock_file = db_path.join("LOCK");
    if !lock_file.exists() {
        return false;
    }

    if is_codescope_running() {
        // Process is alive — lock is legitimate, don't remove
        return false;
    }

    // Process is dead — lock is stale, safe to remove
    eprintln!("  Removing stale LOCK file (no codescope process found)...");
    std::fs::remove_file(&lock_file).is_ok()
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

    // First attempt
    let result = surrealdb::Surreal::new::<surrealdb::engine::local::SurrealKv>(
        path.to_string_lossy().as_ref(),
    )
    .await;

    let db = match result {
        Ok(db) => db,
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("locked") || err_str.contains("LOCK") {
                // Lock detected — try auto-recovery
                if try_remove_stale_lock(&path) {
                    eprintln!("  Retrying database connection...");
                    // Second attempt after removing stale lock
                    match surrealdb::Surreal::new::<surrealdb::engine::local::SurrealKv>(
                        path.to_string_lossy().as_ref(),
                    )
                    .await
                    {
                        Ok(db) => db,
                        Err(e2) => {
                            anyhow::bail!(
                                "Failed to open database at {} (even after removing stale lock).\n\
                                 Error: {}",
                                path.display(),
                                e2
                            );
                        }
                    }
                } else if is_codescope_running() {
                    // Process is alive — can't steal the lock
                    #[cfg(windows)]
                    let kill_cmd = "taskkill /F /IM codescope.exe";
                    #[cfg(not(windows))]
                    let kill_cmd = "pkill -f codescope";
                    anyhow::bail!(
                        "Database is locked by a running codescope process.\n\
                         \n\
                         The MCP server (codescope-mcp) is using this database.\n\
                         You have two options:\n\
                         \n\
                         Option A — Re-index via the running MCP server (no restart needed):\n\
                           Open Claude Code and run: /cs-index\n\
                           Or use the index_codebase MCP tool directly.\n\
                         \n\
                         Option B — Stop the MCP server and re-init:\n\
                           {}\n\
                           codescope init",
                        kill_cmd
                    );
                } else {
                    anyhow::bail!(
                        "Failed to open database at {}.\n\
                         Error: {}\n\
                         \n\
                         Try removing the lock file:\n\
                         rm -f {}/LOCK\n\
                         codescope init",
                        path.display(),
                        e,
                        path.display()
                    );
                }
            } else {
                anyhow::bail!(
                    "Failed to open database at {}.\n\
                     Error: {}\n\
                     \n\
                     Try removing the DB directory:\n\
                     rm -rf {}",
                    path.display(),
                    e,
                    path.display()
                );
            }
        }
    };

    db.use_ns("codescope").use_db(repo_name).await?;
    schema::init_schema(&db).await?;
    // Auto-upgrade schema if DB was created by an older codescope version.
    migrations::migrate_to_current(&db).await?;

    Ok(db)
}

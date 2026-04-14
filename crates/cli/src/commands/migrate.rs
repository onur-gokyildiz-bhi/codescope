//! `codescope migrate` — bring an existing DB up to the current schema version.
//!
//! This runs automatically on every `connect_db` call, but exposing it as
//! a standalone command is useful for diagnostics and for forcing a
//! migration on a DB that isn't otherwise being opened.
use anyhow::Result;
use codescope_core::graph::{migrations, schema};
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    // connect_db already runs migrate_to_current, but we re-query the version
    // afterwards so we can report the final state to the user.
    let db = connect_db(db_path, repo).await?;

    let before = schema::get_schema_version(&db).await.unwrap_or(0);
    let after = migrations::migrate_to_current(&db).await?;

    println!("Repo:           {}", repo);
    println!("Target version: {}", schema::SCHEMA_VERSION);
    println!("DB version:     {}", after);

    if before == after {
        println!("No migrations needed — DB already at current schema.");
    } else {
        println!("Applied migrations {} -> {}.", before, after);
    }

    Ok(())
}

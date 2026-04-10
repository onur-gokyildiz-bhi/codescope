use anyhow::Result;
use codescope_core::temporal::{GitAnalyzer, TemporalGraphSync};
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(
    path: PathBuf,
    repo_name: &str,
    limit: usize,
    db_path: Option<PathBuf>,
) -> Result<()> {
    let db = connect_db(db_path, repo_name).await?;
    let analyzer = GitAnalyzer::open(&path)?;
    let sync = TemporalGraphSync::new(db);

    println!("Syncing {} recent commits for '{}'...", limit, repo_name);
    let count = sync.sync_commits(&analyzer, repo_name, limit).await?;
    println!("Synced {} commits", count);

    Ok(())
}

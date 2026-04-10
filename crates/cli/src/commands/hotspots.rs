use anyhow::Result;
use codescope_core::temporal::TemporalGraphSync;
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path, repo).await?;
    let sync = TemporalGraphSync::new(db);

    let hotspots = sync.calculate_hotspots(repo).await?;

    if hotspots.is_empty() {
        println!("No hotspots found. Run 'sync-history' first.");
        return Ok(());
    }

    println!(
        "{:<30} {:<40} {:>6} {:>6} {:>10}",
        "Function", "File", "Size", "Churn", "Risk"
    );
    println!("{}", "-".repeat(96));

    for h in &hotspots {
        println!(
            "{:<30} {:<40} {:>6} {:>6} {:>10}",
            h.name.as_deref().unwrap_or("?"),
            h.file_path.as_deref().unwrap_or("?"),
            h.size.unwrap_or(0),
            h.churn.unwrap_or(0),
            h.risk_score.unwrap_or(0),
        );
    }

    Ok(())
}

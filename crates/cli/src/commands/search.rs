use anyhow::Result;
use codescope_core::graph::query::GraphQuery;
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(query: &str, limit: usize, repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path, repo).await?;
    let gq = GraphQuery::new(db);

    let results = gq.search_functions(query).await?;

    if results.is_empty() {
        println!("No results found for '{}'", query);
        return Ok(());
    }

    for (i, r) in results.iter().enumerate().take(limit) {
        println!(
            "{}. {} ({}:{})",
            i + 1,
            r.name.as_deref().unwrap_or("?"),
            r.file_path.as_deref().unwrap_or("?"),
            r.start_line.unwrap_or(0),
        );
        if let Some(sig) = &r.signature {
            println!("   {}", sig);
        }
    }

    Ok(())
}

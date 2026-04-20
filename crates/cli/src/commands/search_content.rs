//! `codescope search-content <query>` — BM25 search over the
//! `indexed_content` table for the active repo.

use anyhow::Result;
use codescope_core::indexed;

pub async fn run(query: String, repo: &str, limit: usize) -> Result<()> {
    let db = codescope_core::connect_repo(repo).await?;
    let hits = indexed::search(&db, &query, limit).await?;
    if hits.is_empty() {
        println!("No matches.");
        return Ok(());
    }
    println!();
    println!("  \x1b[1m{} hits\x1b[0m for '{}'", hits.len(), query);
    for h in &hits {
        println!();
        println!(
            "  \x1b[36m{}\x1b[0m  \x1b[2m({})\x1b[0m",
            h.title,
            h.kind.as_deref().unwrap_or("doc")
        );
        println!("    source: {}", h.source);
        println!("    {}", h.snippet.replace('\n', " "));
        println!("    \x1b[2mscore: {:.2}\x1b[0m", h.score);
    }
    println!();
    Ok(())
}

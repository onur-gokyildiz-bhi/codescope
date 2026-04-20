//! `codescope purge-indexed` — drop every row from the active
//! repo's `indexed_content` table. The `knowledge` table is
//! curated state; this one is fully recoverable by re-fetching,
//! so wiping it is safe.

use anyhow::Result;
use codescope_core::indexed;

pub async fn run(repo: &str, yes: bool) -> Result<()> {
    if !yes {
        eprint!(
            "About to drop every indexed_content row in repo '{repo}'. \
             Knowledge table untouched. Continue? [y/N] "
        );
        use std::io::Write;
        std::io::stderr().flush().ok();
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf).ok();
        if !matches!(buf.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("aborted.");
            return Ok(());
        }
    }
    let db = codescope_core::connect_repo(repo).await?;
    let n = indexed::purge(&db).await?;
    println!("✓ purged {n} indexed_content rows from repo '{repo}'");
    Ok(())
}

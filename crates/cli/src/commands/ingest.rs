//! `codescope ingest <source>` — fetch a URL or read a file,
//! extract text, store in the per-repo `indexed_content` table
//! so the LLM can `search_indexed` it later.

use anyhow::Result;
use codescope_core::indexed;

pub async fn run(
    source: String,
    repo: &str,
    title: Option<String>,
    tags: Vec<String>,
) -> Result<()> {
    let db = codescope_core::connect_repo(repo).await?;
    codescope_core::graph::schema::init_schema(&db).await?;
    let item = indexed::fetch_and_store(&db, &source, title.as_deref(), tags).await?;
    println!(
        "✓ indexed '{}' ({} bytes, kind={})",
        item.title,
        item.size_bytes.unwrap_or(0),
        item.kind.as_deref().unwrap_or("doc")
    );
    println!("  source: {}", item.source);
    Ok(())
}

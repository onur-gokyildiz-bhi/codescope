use anyhow::Result;
use codescope_core::embeddings::{
    EmbeddingPipeline, FastEmbedProvider, OllamaProvider, OpenAIProvider,
};
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(
    query: &str,
    limit: usize,
    provider: &str,
    ollama_url: &str,
    model: &str,
    repo: &str,
    db_path: Option<PathBuf>,
) -> Result<()> {
    let db = connect_db(db_path, repo).await?;

    let embedding_provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider
    {
        "fastembed" => Box::new(FastEmbedProvider::from_name(model)?),
        "ollama" => Box::new(OllamaProvider::new(
            Some(ollama_url.to_string()),
            Some(model.to_string()),
        )),
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;
            Box::new(OpenAIProvider::new(api_key, Some(model.to_string())))
        }
        _ => return Err(anyhow::anyhow!("Unknown provider: {}", provider)),
    };

    let pipeline = EmbeddingPipeline::new(db, embedding_provider);
    let results = pipeline.semantic_search(query, limit).await?;

    if results.is_empty() {
        println!("No semantic results for '{}'", query);
        return Ok(());
    }

    let has_bq = results.first().and_then(|r| r.hamming_distance).is_some();
    let mode = if has_bq {
        "BQ + Cosine (two-stage)"
    } else {
        "Cosine only"
    };
    println!("Semantic search results for '{}' [{}]:\n", query, mode);
    for (i, r) in results.iter().enumerate() {
        let hamming = r
            .hamming_distance
            .map(|h| format!(" hamming:{}", h))
            .unwrap_or_default();
        println!(
            "{}. {} ({}:{}) — cosine: {:.4}{}",
            i + 1,
            r.name,
            r.file_path,
            r.start_line.unwrap_or(0),
            r.score.unwrap_or(0.0),
            hamming,
        );
        if let Some(sig) = &r.signature {
            println!("   {}", sig);
        }
    }

    Ok(())
}

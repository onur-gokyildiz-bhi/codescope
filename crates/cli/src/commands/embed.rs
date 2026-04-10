use anyhow::Result;
use codescope_core::embeddings::{
    EmbeddingPipeline, FastEmbedProvider, OllamaProvider, OpenAIProvider,
};
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(
    provider: &str,
    batch_size: usize,
    ollama_url: &str,
    model: &str,
    repo: &str,
    db_path: Option<PathBuf>,
) -> Result<()> {
    let db = connect_db(db_path, repo).await?;

    let embedding_provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider
    {
        "fastembed" => {
            println!("Using FastEmbed (local, in-process). Model downloads on first run.");
            Box::new(FastEmbedProvider::from_name(model)?)
        }
        "ollama" => Box::new(OllamaProvider::new(
            Some(ollama_url.to_string()),
            Some(model.to_string()),
        )),
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;
            Box::new(OpenAIProvider::new(api_key, Some(model.to_string())))
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown provider: {}. Use 'fastembed', 'ollama', or 'openai'",
                provider
            ))
        }
    };

    println!("Embedding with {} (model: {})...", provider, model);

    let pipeline = EmbeddingPipeline::new(db, embedding_provider);
    let result = pipeline.embed_functions(batch_size).await?;
    let backfilled = pipeline.backfill_binary_quantization().await.unwrap_or(0);
    let dims = pipeline.dimensions();
    let bq_bytes = dims.div_ceil(8);

    println!(
        "Embedded {} functions with Binary Quantization",
        result.embedded
    );
    println!("  BQ backfilled: {}", backfilled);
    println!(
        "  Memory: f32 = {} bytes/vec, BQ = {} bytes/vec ({}x smaller)",
        dims * 4,
        bq_bytes,
        (dims * 4) / bq_bytes
    );
    Ok(())
}

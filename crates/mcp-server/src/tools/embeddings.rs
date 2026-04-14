//! Embedding tools: embed_functions, semantic_search.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = embeddings_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Generate embeddings for all functions in the graph
    #[tool(
        description = "Generate vector embeddings for unembedded functions."
    )]
    async fn embed_functions(&self, Parameters(params): Parameters<EmbedParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let batch_size = params.batch_size.unwrap_or(100);
        let provider_name = params.provider.as_deref().unwrap_or("fastembed");

        let provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider_name {
            "ollama" => Box::new(codescope_core::embeddings::OllamaProvider::new(
                Some("http://localhost:11434".into()),
                Some("nomic-embed-text".into()),
            )),
            "openai" => {
                let api_key = match std::env::var("OPENAI_API_KEY") {
                    Ok(k) => k,
                    Err(_) => return "OPENAI_API_KEY environment variable not set.".into(),
                };
                Box::new(codescope_core::embeddings::OpenAIProvider::new(
                    api_key, None,
                ))
            }
            _ => match codescope_core::embeddings::FastEmbedProvider::new() {
                Ok(p) => Box::new(p),
                Err(e) => return format!("Error creating FastEmbed provider: {}", e),
            },
        };

        let pipeline = codescope_core::embeddings::EmbeddingPipeline::new(ctx.db, provider);

        match pipeline.embed_functions(batch_size).await {
            Ok(result) => {
                let backfilled = pipeline.backfill_binary_quantization().await.unwrap_or(0);
                let dims = pipeline.dimensions();
                let bq_bytes = dims.div_ceil(8);

                format!(
                    "## Embedding Complete\n\n\
                     - **Embedded:** {} functions ({} dimensions)\n\
                     - **Binary Quantized:** {} (BQ backfilled: {})\n\
                     - **Memory per vector:** f32 = {} bytes, BQ = {} bytes (**{}x smaller**)\n\
                     - **Provider:** {}\n\
                     - **Search mode:** Two-stage (Hamming pre-filter → Cosine rerank)",
                    result.embedded,
                    dims,
                    result.binary_quantized,
                    backfilled,
                    dims * 4,
                    bq_bytes,
                    (dims * 4) / bq_bytes,
                    pipeline.provider_name()
                )
            }
            Err(e) => format!("Error embedding functions: {}", e),
        }
    }

    /// Search for semantically similar code using vector embeddings
    #[tool(
        description = "Search code by meaning via vector similarity, not just name."
    )]
    async fn semantic_search(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = params.limit.unwrap_or(10);
        let provider_name = params.provider.as_deref().unwrap_or("fastembed");

        let provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider_name {
            "ollama" => Box::new(codescope_core::embeddings::OllamaProvider::new(
                Some("http://localhost:11434".into()),
                Some("nomic-embed-text".into()),
            )),
            "openai" => {
                let api_key = match std::env::var("OPENAI_API_KEY") {
                    Ok(k) => k,
                    Err(_) => return "OPENAI_API_KEY environment variable not set.".into(),
                };
                Box::new(codescope_core::embeddings::OpenAIProvider::new(
                    api_key, None,
                ))
            }
            _ => match codescope_core::embeddings::FastEmbedProvider::new() {
                Ok(p) => Box::new(p),
                Err(e) => return format!("Error creating FastEmbed provider: {}", e),
            },
        };

        let pipeline = codescope_core::embeddings::EmbeddingPipeline::new(ctx.db, provider);

        match pipeline.semantic_search(&params.query, limit).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!(
                        "No semantic matches for '{}'. Run embed_functions first to generate embeddings.",
                        params.query
                    );
                }
                let has_bq = results.first().and_then(|r| r.hamming_distance).is_some();
                let mode = if has_bq {
                    "BQ + Cosine (two-stage)"
                } else {
                    "Cosine only"
                };
                let mut output = format!(
                    "## Semantic Search: '{}'\n**Mode:** {}\n\n",
                    params.query, mode
                );
                for (i, r) in results.iter().enumerate() {
                    let score = r
                        .score
                        .map(|s| format!("{:.3}", s))
                        .unwrap_or_else(|| "?".into());
                    let hamming = r
                        .hamming_distance
                        .map(|h| format!(" (hamming: {})", h))
                        .unwrap_or_default();
                    output.push_str(&format!(
                        "{}. **{}** ({}) — cosine: {}{}\n",
                        i + 1,
                        r.name,
                        r.file_path,
                        score,
                        hamming,
                    ));
                    if let Some(sig) = &r.signature {
                        output.push_str(&format!("   `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Semantic search error: {}", e),
        }
    }
}

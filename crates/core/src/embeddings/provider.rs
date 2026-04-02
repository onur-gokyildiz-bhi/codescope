use anyhow::Result;
use async_trait::async_trait;

/// Trait for embedding providers (Ollama, OpenAI, etc.)
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for a piece of text
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for a batch of texts
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Return the dimensionality of the embedding vectors
    fn dimensions(&self) -> usize;

    /// Return the name of the provider
    fn name(&self) -> &str;
}

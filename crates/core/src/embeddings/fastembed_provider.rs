use anyhow::Result;
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Mutex;

use super::provider::EmbeddingProvider;

/// In-process embedding provider using FastEmbed (ONNX Runtime)
/// No external service needed — models auto-download on first use
pub struct FastEmbedProvider {
    model: Mutex<TextEmbedding>,
    dimensions: usize,
    _model_name: String,
}

impl FastEmbedProvider {
    /// Create with default model (BGE-Small-EN-v1.5, 384 dims)
    pub fn new() -> Result<Self> {
        Self::with_model(EmbeddingModel::BGESmallENV15)
    }

    /// Create with a specific model
    pub fn with_model(model: EmbeddingModel) -> Result<Self> {
        let dimensions = match model {
            EmbeddingModel::BGESmallENV15 | EmbeddingModel::BGESmallENV15Q => 384,
            EmbeddingModel::AllMiniLML6V2 | EmbeddingModel::AllMiniLML6V2Q => 384,
            EmbeddingModel::AllMiniLML12V2 | EmbeddingModel::AllMiniLML12V2Q => 384,
            EmbeddingModel::BGEBaseENV15 | EmbeddingModel::BGEBaseENV15Q => 768,
            EmbeddingModel::BGELargeENV15 | EmbeddingModel::BGELargeENV15Q => 1024,
            EmbeddingModel::NomicEmbedTextV15 | EmbeddingModel::NomicEmbedTextV15Q => 768,
            EmbeddingModel::NomicEmbedTextV1 => 768,
            _ => 384, // safe default
        };

        let model_name = format!("{:?}", model);

        let text_embedding =
            TextEmbedding::try_new(InitOptions::new(model).with_show_download_progress(true))?;

        Ok(Self {
            model: Mutex::new(text_embedding),
            dimensions,
            _model_name: model_name,
        })
    }

    /// Create from model name string (for CLI/config)
    pub fn from_name(name: &str) -> Result<Self> {
        let model = match name.to_lowercase().as_str() {
            "bge-small" | "bge-small-en" | "bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
            "bge-small-q" => EmbeddingModel::BGESmallENV15Q,
            "bge-base" | "bge-base-en" | "bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
            "bge-large" | "bge-large-en" | "bge-large-en-v1.5" => EmbeddingModel::BGELargeENV15,
            "all-minilm-l6" | "minilm" => EmbeddingModel::AllMiniLML6V2,
            "all-minilm-l12" => EmbeddingModel::AllMiniLML12V2,
            "nomic" | "nomic-embed-text" | "nomic-v1.5" => EmbeddingModel::NomicEmbedTextV15,
            _ => {
                tracing::warn!(
                    "Unknown model '{}', falling back to bge-small-en-v1.5",
                    name
                );
                EmbeddingModel::BGESmallENV15
            }
        };
        Self::with_model(model)
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let text = text.to_string();
        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let embeddings = model.embed(vec![text], None)?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut model = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let embeddings = model.embed(texts, None)?;
        Ok(embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn name(&self) -> &str {
        "fastembed"
    }
}

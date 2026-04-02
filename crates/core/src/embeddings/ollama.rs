use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::provider::EmbeddingProvider;

pub struct OllamaProvider {
    base_url: String,
    model: String,
    dimensions: usize,
}

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl OllamaProvider {
    pub fn new(base_url: Option<String>, model: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".into()),
            model: model.unwrap_or_else(|| "nomic-embed-text".into()),
            dimensions: 768, // nomic-embed-text default
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/api/embed", self.base_url))
            .json(&EmbedRequest {
                model: self.model.clone(),
                input: text.to_string(),
            })
            .send()
            .await?
            .json::<EmbedResponse>()
            .await?;

        resp.embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned from Ollama"))
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

use anyhow::Result;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use tracing::{info, warn};

use super::provider::EmbeddingProvider;

/// Embeds code entities and stores vectors in SurrealDB
pub struct EmbeddingPipeline {
    db: Surreal<Db>,
    provider: Box<dyn EmbeddingProvider>,
}

impl EmbeddingPipeline {
    pub fn new(db: Surreal<Db>, provider: Box<dyn EmbeddingProvider>) -> Self {
        Self { db, provider }
    }

    /// Embed all functions that don't have embeddings yet
    pub async fn embed_functions(&self, batch_size: usize) -> Result<usize> {
        // Get functions without embeddings
        let functions: Vec<FunctionRecord> = self
            .db
            .query(
                "SELECT id, name, signature, file_path FROM `function` WHERE embedding IS NONE LIMIT $limit"
                    .to_string(),
            )
            .bind(("limit", batch_size as i64))
            .await?
            .take(0)?;

        if functions.is_empty() {
            info!("All functions already have embeddings");
            return Ok(0);
        }

        info!("Embedding {} functions...", functions.len());

        let mut count = 0;
        for func in &functions {
            // Build a text representation of the function for embedding
            let text = build_function_text(func);

            match self.provider.embed(&text).await {
                Ok(embedding) => {
                    let id_str = func.id.to_string();
                    let result = self
                        .db
                        .query("UPDATE $id SET embedding = $embedding".to_string())
                        .bind(("id", surrealdb::sql::Thing::from(("function".to_string(), id_str.clone()))))
                        .bind(("embedding", embedding))
                        .await;

                    match result {
                        Ok(_) => {
                            count += 1;
                        }
                        Err(e) => {
                            warn!("Failed to store embedding for {}: {}", func.name, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to embed {}: {}", func.name, e);
                }
            }
        }

        info!("Embedded {} functions", count);
        Ok(count)
    }

    /// Perform semantic vector search for functions
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SemanticSearchResult>> {
        let query_embedding = self.provider.embed(query).await?;

        let results: Vec<SemanticSearchResult> = self
            .db
            .query(
                "SELECT name, qualified_name, signature, file_path, start_line, end_line, \
                 vector::similarity::cosine(embedding, $query_vec) AS score \
                 FROM `function` WHERE embedding IS NOT NONE \
                 ORDER BY score DESC LIMIT $limit"
                    .to_string(),
            )
            .bind(("query_vec", query_embedding))
            .bind(("limit", limit as i64))
            .await?
            .take(0)?;

        Ok(results)
    }

    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    pub fn dimensions(&self) -> usize {
        self.provider.dimensions()
    }
}

#[derive(Debug, serde::Deserialize)]
struct FunctionRecord {
    id: surrealdb::sql::Thing,
    name: String,
    signature: Option<String>,
    file_path: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SemanticSearchResult {
    pub name: String,
    pub qualified_name: Option<String>,
    pub signature: Option<String>,
    pub file_path: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub score: Option<f64>,
}

fn build_function_text(func: &FunctionRecord) -> String {
    let mut text = String::new();
    text.push_str(&func.name);
    if let Some(sig) = &func.signature {
        text.push_str(" ");
        text.push_str(sig);
    }
    text.push_str(" in ");
    text.push_str(&func.file_path);
    text
}

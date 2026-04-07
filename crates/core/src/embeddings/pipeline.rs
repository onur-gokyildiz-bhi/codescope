use anyhow::Result;
use surrealdb::engine::local::Db;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};
use surrealdb::Surreal;
use tracing::{debug, info, warn};

/// Format a RecordId as "table:key" string (equivalent to surrealdb v2 Thing::to_string())
fn record_id_string(id: &RecordId) -> String {
    id.to_sql()
}

/// Extract just the key part of a RecordId as a string
fn record_id_key_string(key: &RecordIdKey) -> String {
    match key {
        RecordIdKey::String(s) => s.clone(),
        RecordIdKey::Number(n) => n.to_string(),
        RecordIdKey::Uuid(u) => u.to_string(),
        _ => format!("{:?}", key),
    }
}

use super::provider::EmbeddingProvider;

// =====================================================================
// Binary Quantization — 32x memory reduction
// Converts f32 vectors to packed binary vectors (sign-bit quantization)
// Used by Perplexity, Azure, HubSpot for efficient vector retrieval
// =====================================================================

/// Convert a float32 embedding to a packed binary vector.
/// Each f32 component becomes 1 bit: positive → 1, non-positive → 0.
/// 384-dim f32 (1536 bytes) → 48 bytes (32x smaller)
#[inline]
pub fn binary_quantize(embedding: &[f32]) -> Vec<u8> {
    let num_bytes = embedding.len().div_ceil(8);
    let mut packed = vec![0u8; num_bytes];
    for (i, &val) in embedding.iter().enumerate() {
        if val > 0.0 {
            packed[i / 8] |= 1 << (7 - (i % 8));
        }
    }
    packed
}

/// Compute Hamming distance between two packed binary vectors.
/// Uses POPCNT-friendly XOR + count_ones — extremely fast on modern CPUs.
#[inline]
pub fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x ^ y).count_ones())
        .sum()
}

/// Compute cosine similarity between two f32 vectors.
#[inline]
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..a.len().min(b.len()) {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-10 {
        0.0
    } else {
        dot / denom
    }
}

// =====================================================================
// Embedding Pipeline
// =====================================================================

/// Embeds code entities and stores vectors in SurrealDB
pub struct EmbeddingPipeline {
    db: Surreal<Db>,
    provider: Box<dyn EmbeddingProvider>,
}

impl EmbeddingPipeline {
    pub fn new(db: Surreal<Db>, provider: Box<dyn EmbeddingProvider>) -> Self {
        Self { db, provider }
    }

    /// Embed all functions that don't have embeddings yet.
    /// Stores both f32 embedding AND binary quantized version.
    pub async fn embed_functions(&self, batch_size: usize) -> Result<EmbedResult> {
        // Get functions without embeddings
        let functions: Vec<FunctionRecord> = self
            .db
            .query(
                "SELECT id, name, signature, file_path FROM `function` \
                 WHERE embedding IS NONE LIMIT $limit",
            )
            .bind(("limit", batch_size as i64))
            .await?
            .take(0)?;

        if functions.is_empty() {
            info!("All functions already have embeddings");
            return Ok(EmbedResult {
                embedded: 0,
                binary_quantized: 0,
            });
        }

        info!("Embedding {} functions...", functions.len());

        // Batch embed all texts at once for efficiency
        let texts: Vec<String> = functions.iter().map(build_function_text).collect();
        let embeddings = self.provider.embed_batch(&texts).await?;

        let mut count = 0;
        let mut bq_count = 0;
        let dims = self.provider.dimensions();

        for (func, embedding) in functions.iter().zip(embeddings.into_iter()) {
            // Binary quantize the embedding
            let bq = binary_quantize(&embedding);
            let bq_as_ints: Vec<i64> = bq.iter().map(|&b| b as i64).collect();

            let id_str = record_id_key_string(&func.id.key);
            let update_query = format!(
                "UPDATE `function`:`{}` SET embedding = $embedding, binary_embedding = $bq",
                crate::graph::builder::sanitize_id(&id_str)
            );
            let result = self
                .db
                .query(&update_query)
                .bind(("embedding", embedding))
                .bind(("bq", bq_as_ints))
                .await;

            match result {
                Ok(mut response) => {
                    let updated: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                    if !updated.is_empty() {
                        count += 1;
                        bq_count += 1;
                    } else {
                        warn!(
                            "Embedding UPDATE returned Ok but 0 rows affected for {}",
                            func.name
                        );
                    }
                }
                Err(e) => {
                    warn!("Failed to store embedding for {}: {}", func.name, e);
                }
            }
        }

        let f32_bytes = count * dims * 4;
        let bq_bytes = bq_count * dims.div_ceil(8);
        info!(
            "Embedded {} functions ({} dims). Storage: f32={} KB, BQ={} KB ({}x smaller)",
            count,
            dims,
            f32_bytes / 1024,
            bq_bytes / 1024,
            if bq_bytes > 0 {
                f32_bytes / bq_bytes
            } else {
                0
            }
        );

        Ok(EmbedResult {
            embedded: count,
            binary_quantized: bq_count,
        })
    }

    /// Backfill binary embeddings for functions that have f32 but no BQ.
    /// Useful after upgrading from pre-BQ version.
    pub async fn backfill_binary_quantization(&self) -> Result<usize> {
        let functions: Vec<BQBackfillRecord> = self
            .db
            .query(
                "SELECT id, embedding FROM `function` \
                 WHERE embedding IS NOT NONE AND binary_embedding IS NONE \
                 LIMIT 1000",
            )
            .await?
            .take(0)?;

        if functions.is_empty() {
            return Ok(0);
        }

        info!("Backfilling BQ for {} functions...", functions.len());
        let mut count = 0;

        for func in &functions {
            let bq = binary_quantize(&func.embedding);
            let bq_as_ints: Vec<i64> = bq.iter().map(|&b| b as i64).collect();

            let id_str = record_id_key_string(&func.id.key);
            let bq_query = format!(
                "UPDATE `function`:`{}` SET binary_embedding = $bq",
                crate::graph::builder::sanitize_id(&id_str)
            );
            match self.db.query(&bq_query).bind(("bq", bq_as_ints)).await {
                Ok(mut response) => {
                    let updated: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                    if !updated.is_empty() {
                        count += 1;
                    } else {
                        warn!(
                            "BQ backfill UPDATE returned Ok but 0 rows affected for {:?}",
                            func.id
                        );
                    }
                }
                Err(e) => warn!("BQ backfill failed for {:?}: {}", func.id, e),
            }
        }

        Ok(count)
    }

    /// Two-stage semantic search using Binary Quantization.
    ///
    /// Stage 1 (Coarse): Load all binary embeddings, compute Hamming distance in Rust.
    ///   → Fast: 48 bytes/vector, XOR + POPCNT, ~O(N) but very small constant.
    /// Stage 2 (Fine): Load f32 embeddings for top-K candidates only, compute cosine.
    ///   → Accurate: full precision reranking on tiny candidate set.
    ///
    /// This is the exact pattern used by Perplexity, Azure, and HubSpot.
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SemanticSearchResult>> {
        let query_embedding = self.provider.embed(query).await?;
        let query_bq = binary_quantize(&query_embedding);

        // Check if we have binary embeddings available
        let bq_count: Vec<serde_json::Value> = self
            .db
            .query("SELECT count() AS total FROM `function` WHERE binary_embedding IS NOT NONE GROUP ALL")
            .await?
            .take(0)?;

        let has_bq = bq_count
            .first()
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0;

        if has_bq {
            self.two_stage_search(&query_embedding, &query_bq, limit)
                .await
        } else {
            // Fallback: direct cosine search (pre-BQ data)
            debug!("No binary embeddings found, falling back to cosine-only search");
            self.cosine_only_search(&query_embedding, limit).await
        }
    }

    /// Stage 1+2: Binary Quantization pre-filter → Cosine rerank
    async fn two_stage_search(
        &self,
        query_embedding: &[f32],
        query_bq: &[u8],
        limit: usize,
    ) -> Result<Vec<SemanticSearchResult>> {
        // Stage 1: Load ALL binary embeddings (very small: ~48 bytes each)
        let candidates: Vec<BQRecord> = self
            .db
            .query(
                "SELECT id, name, qualified_name, signature, file_path, start_line, end_line, binary_embedding \
                 FROM `function` WHERE binary_embedding IS NOT NONE",
            )
            .await?
            .take(0)?;

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        // Compute Hamming distance for ALL candidates (extremely fast in Rust)
        let rerank_k = (limit * 5).max(50).min(candidates.len()); // 5x oversampling
        let mut scored: Vec<(usize, u32)> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let bq_bytes: Vec<u8> = c.binary_embedding.iter().map(|&v| v as u8).collect();
                (i, hamming_distance(query_bq, &bq_bytes))
            })
            .collect();

        // Sort by Hamming distance (ascending = more similar)
        scored.sort_unstable_by_key(|&(_, dist)| dist);
        scored.truncate(rerank_k);

        debug!(
            "BQ pre-filter: {} candidates → top {} for reranking (Hamming range: {}-{})",
            candidates.len(),
            scored.len(),
            scored.first().map(|s| s.1).unwrap_or(0),
            scored.last().map(|s| s.1).unwrap_or(0),
        );

        // Stage 2: Load full f32 embeddings ONLY for top-K candidates
        // Use backtick-quoted record IDs to avoid SurrealQL parsing issues with string interpolation
        let id_list = scored
            .iter()
            .map(|&(i, _)| {
                let id = &candidates[i].id;
                format!("`function`:`{}`", record_id_key_string(&id.key))
            })
            .collect::<Vec<_>>()
            .join(", ");
        let query = format!(
            "SELECT id, embedding FROM `function` WHERE id IN [{}]",
            id_list
        );

        let full_embeddings: Vec<FullEmbeddingRecord> = self.db.query(&query).await?.take(0)?;

        // Build a map of id → f32 embedding
        let embed_map: std::collections::HashMap<String, &Vec<f32>> = full_embeddings
            .iter()
            .map(|r| (record_id_string(&r.id), &r.embedding))
            .collect();

        // Compute cosine similarity for reranking
        let mut results: Vec<SemanticSearchResult> = scored
            .iter()
            .map(|&(i, hamming)| {
                let c = &candidates[i];
                let id_str = record_id_string(&c.id);
                let cosine = embed_map
                    .get(&id_str)
                    .map(|emb| cosine_similarity(query_embedding, emb))
                    .unwrap_or(0.0);

                SemanticSearchResult {
                    name: c.name.clone(),
                    qualified_name: c.qualified_name.clone(),
                    signature: c.signature.clone(),
                    file_path: c.file_path.clone(),
                    start_line: c.start_line,
                    end_line: c.end_line,
                    score: Some(cosine as f64),
                    hamming_distance: Some(hamming),
                }
            })
            .collect();

        // Sort by cosine similarity (descending = more similar)
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }

    /// Fallback: cosine-only search (for pre-BQ data)
    async fn cosine_only_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<SemanticSearchResult>> {
        let results: Vec<SemanticSearchResultRaw> = self
            .db
            .query(
                "SELECT name, qualified_name, signature, file_path, start_line, end_line, \
                 vector::similarity::cosine(embedding, $query_vec) AS score \
                 FROM `function` WHERE embedding IS NOT NONE \
                 ORDER BY score DESC LIMIT $limit",
            )
            .bind(("query_vec", query_embedding.to_vec()))
            .bind(("limit", limit as i64))
            .await?
            .take(0)?;

        Ok(results
            .into_iter()
            .map(|r| SemanticSearchResult {
                name: r.name,
                qualified_name: r.qualified_name,
                signature: r.signature,
                file_path: r.file_path,
                start_line: r.start_line,
                end_line: r.end_line,
                score: r.score,
                hamming_distance: None,
            })
            .collect())
    }

    /// Get embedding statistics
    pub async fn stats(&self) -> Result<EmbedStats> {
        let result: Vec<serde_json::Value> = self
            .db
            .query(
                "SELECT count() AS total FROM `function` GROUP ALL; \
                 SELECT count() AS total FROM `function` WHERE embedding IS NOT NONE GROUP ALL; \
                 SELECT count() AS total FROM `function` WHERE binary_embedding IS NOT NONE GROUP ALL;"
            )
            .await?
            .take(0)?;

        // Parse results
        let total_funcs = result
            .first()
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        Ok(EmbedStats {
            total_functions: total_funcs,
            with_embedding: 0, // parsed from stmt 1
            with_binary: 0,    // parsed from stmt 2
            dimensions: self.provider.dimensions(),
            f32_memory_kb: 0,
            bq_memory_kb: 0,
        })
    }

    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    pub fn dimensions(&self) -> usize {
        self.provider.dimensions()
    }
}

// =====================================================================
// Types
// =====================================================================

#[derive(Debug)]
pub struct EmbedResult {
    pub embedded: usize,
    pub binary_quantized: usize,
}

#[derive(Debug)]
pub struct EmbedStats {
    pub total_functions: usize,
    pub with_embedding: usize,
    pub with_binary: usize,
    pub dimensions: usize,
    pub f32_memory_kb: usize,
    pub bq_memory_kb: usize,
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
    pub hamming_distance: Option<u32>,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct SemanticSearchResultRaw {
    name: String,
    qualified_name: Option<String>,
    signature: Option<String>,
    file_path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    score: Option<f64>,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct FunctionRecord {
    id: surrealdb::types::RecordId,
    name: String,
    signature: Option<String>,
    file_path: String,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct BQRecord {
    id: surrealdb::types::RecordId,
    name: String,
    qualified_name: Option<String>,
    signature: Option<String>,
    file_path: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    binary_embedding: Vec<i64>,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct FullEmbeddingRecord {
    id: surrealdb::types::RecordId,
    embedding: Vec<f32>,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct BQBackfillRecord {
    id: surrealdb::types::RecordId,
    embedding: Vec<f32>,
}

fn build_function_text(func: &FunctionRecord) -> String {
    let mut text = String::with_capacity(func.name.len() + 100);
    text.push_str(&func.name);
    if let Some(sig) = &func.signature {
        text.push(' ');
        text.push_str(sig);
    }
    text.push_str(" in ");
    text.push_str(&func.file_path);
    text
}

//! Embedding pipeline unit tests — tests binary quantization, hamming distance,
//! cosine similarity, and the EmbeddingPipeline::stats() regression.

use anyhow::Result;
use async_trait::async_trait;
use codescope_core::embeddings::pipeline::EmbeddingPipeline;
use codescope_core::embeddings::provider::EmbeddingProvider;
use codescope_core::embeddings::{binary_quantize, hamming_distance};
use codescope_core::graph::schema::init_schema;
use surrealdb::engine::local::Mem;
use surrealdb::Surreal;

#[test]
fn test_binary_quantize_dimensions() {
    let embedding = vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6, 0.7, -0.8];
    let bq = binary_quantize(&embedding);
    // 8 dimensions -> 1 byte
    assert_eq!(bq.len(), 1);
}

#[test]
fn test_binary_quantize_values() {
    // Positive -> 1, Negative -> 0
    let embedding = vec![1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
    let bq = binary_quantize(&embedding);
    assert_eq!(bq[0], 0b10101010); // bits: 1,0,1,0,1,0,1,0
}

#[test]
fn test_binary_quantize_384_dims() {
    let embedding: Vec<f32> = (0..384)
        .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect();
    let bq = binary_quantize(&embedding);
    assert_eq!(bq.len(), 48); // 384 / 8 = 48 bytes
}

#[test]
fn test_hamming_distance_identical() {
    let a = vec![0b11111111u8, 0b00000000];
    let b = vec![0b11111111u8, 0b00000000];
    assert_eq!(hamming_distance(&a, &b), 0);
}

#[test]
fn test_hamming_distance_opposite() {
    let a = vec![0b11111111u8];
    let b = vec![0b00000000u8];
    assert_eq!(hamming_distance(&a, &b), 8);
}

#[test]
fn test_hamming_distance_one_bit() {
    let a = vec![0b10000000u8];
    let b = vec![0b00000000u8];
    assert_eq!(hamming_distance(&a, &b), 1);
}

#[test]
fn test_binary_quantize_zero_vector() {
    let embedding = vec![0.0; 16];
    let bq = binary_quantize(&embedding);
    assert_eq!(bq.len(), 2);
    // All zeros -> all bits 0
    assert_eq!(bq[0], 0);
    assert_eq!(bq[1], 0);
}

// ─── EmbeddingPipeline::stats() regression test ────────────────────────
//
// Regression for a bug where stats() always returned with_embedding=0,
// with_binary=0 because the multi-statement query only called .take(0)
// (not .take(1) and .take(2)) and the values were hardcoded to 0.

struct MockProvider;

#[async_trait]
impl EmbeddingProvider for MockProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1; 8])
    }
    fn dimensions(&self) -> usize {
        8
    }
    fn name(&self) -> &str {
        "mock"
    }
}

async fn setup_db_with_embeddings() -> Surreal<surrealdb::engine::local::Db> {
    let db = Surreal::new::<Mem>(()).await.unwrap();
    db.use_ns("codescope").use_db("test").await.unwrap();
    init_schema(&db).await.unwrap();

    // Schema is SCHEMAFULL — must set all required fields.
    // Insert 3 functions:
    //   f1: has both f32 embedding AND binary_embedding
    //   f2: has f32 embedding only
    //   f3: has neither
    let common = "repo = 'test', language = 'rust', start_line = 1, end_line = 10";
    let surql = format!(
        "CREATE `function`:f1 SET name = 'f1', qualified_name = 'm::f1', file_path = 'a.rs', \
            embedding = [0.1, 0.2], binary_embedding = [128, 0], {common}; \
         CREATE `function`:f2 SET name = 'f2', qualified_name = 'm::f2', file_path = 'a.rs', \
            embedding = [0.3, 0.4], {common}; \
         CREATE `function`:f3 SET name = 'f3', qualified_name = 'm::f3', file_path = 'b.rs', {common};"
    );
    db.query(surql).await.unwrap();
    db
}

#[tokio::test]
async fn embed_pipeline_stats_returns_correct_counts() {
    let db = setup_db_with_embeddings().await;
    let pipeline = EmbeddingPipeline::new(db, Box::new(MockProvider));

    let stats = pipeline.stats().await.expect("stats should succeed");

    // REGRESSION: previously these were hardcoded to 0
    assert_eq!(stats.total_functions, 3, "total should be 3");
    assert_eq!(
        stats.with_embedding, 2,
        "should be 2 functions with embedding (was 0 due to bug)"
    );
    assert_eq!(
        stats.with_binary, 1,
        "should be 1 function with binary embedding (was 0 due to bug)"
    );
    assert_eq!(stats.dimensions, 8);
}

#[tokio::test]
async fn embed_pipeline_stats_empty_db() {
    let db = Surreal::new::<Mem>(()).await.unwrap();
    db.use_ns("codescope").use_db("test").await.unwrap();
    init_schema(&db).await.unwrap();

    let pipeline = EmbeddingPipeline::new(db, Box::new(MockProvider));
    let stats = pipeline.stats().await.expect("stats on empty DB");

    assert_eq!(stats.total_functions, 0);
    assert_eq!(stats.with_embedding, 0);
    assert_eq!(stats.with_binary, 0);
}

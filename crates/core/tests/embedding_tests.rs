//! Embedding pipeline unit tests — tests binary quantization, hamming distance,
//! and cosine similarity without requiring ONNX models.

use codescope_core::embeddings::{binary_quantize, hamming_distance};

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

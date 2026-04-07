pub mod fastembed_provider;
pub mod ollama;
pub mod openai;
pub mod pipeline;
pub mod provider;

pub use fastembed_provider::FastEmbedProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
pub use pipeline::{
    binary_quantize, hamming_distance, EmbedResult, EmbeddingPipeline, SemanticSearchResult,
};
pub use provider::EmbeddingProvider;

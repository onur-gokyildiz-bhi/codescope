pub mod provider;
pub mod ollama;
pub mod openai;
pub mod fastembed_provider;
pub mod pipeline;

pub use provider::EmbeddingProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
pub use fastembed_provider::FastEmbedProvider;
pub use pipeline::{EmbeddingPipeline, SemanticSearchResult, EmbedResult, binary_quantize, hamming_distance};

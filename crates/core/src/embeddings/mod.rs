pub mod provider;
pub mod ollama;
pub mod openai;
pub mod pipeline;

pub use provider::EmbeddingProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
pub use pipeline::{EmbeddingPipeline, SemanticSearchResult};

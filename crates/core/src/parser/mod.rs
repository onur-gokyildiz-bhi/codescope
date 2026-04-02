pub mod languages;
pub mod extractor;

use anyhow::Result;
use std::path::Path;
use tree_sitter::Parser;

use crate::{CodeEntity, CodeRelation};
use languages::LanguageRegistry;
use extractor::EntityExtractor;

/// Parses source files and extracts code entities + relations
pub struct CodeParser {
    registry: LanguageRegistry,
}

impl CodeParser {
    pub fn new() -> Self {
        Self {
            registry: LanguageRegistry::new(),
        }
    }

    /// Parse a single file and extract entities and relations
    pub fn parse_file(
        &self,
        file_path: &Path,
        repo_name: &str,
    ) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let language_config = self
            .registry
            .get_by_extension(ext)
            .ok_or_else(|| anyhow::anyhow!("Unsupported file extension: {}", ext))?;

        let source = std::fs::read_to_string(file_path)?;

        let mut parser = Parser::new();
        parser.set_language(&language_config.language)?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse: {}", file_path.display()))?;

        let extractor = EntityExtractor::new(
            file_path.to_string_lossy().to_string(),
            repo_name.to_string(),
            language_config.name.clone(),
        );

        extractor.extract(&tree, &source)
    }

    /// Check if a file extension is supported
    pub fn supports_extension(&self, ext: &str) -> bool {
        self.registry.get_by_extension(ext).is_some()
    }

    /// List all supported languages
    pub fn supported_languages(&self) -> Vec<String> {
        self.registry.language_names()
    }
}

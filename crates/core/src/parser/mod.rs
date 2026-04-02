pub mod languages;
pub mod extractor;
pub mod content;

use anyhow::Result;
use std::path::Path;
use tree_sitter::Parser;

use crate::{CodeEntity, CodeRelation};
use languages::LanguageRegistry;
use extractor::EntityExtractor;
use content::ContentParserRegistry;

/// Parses source files and extracts code entities + relations.
/// Supports both tree-sitter languages and custom content parsers.
pub struct CodeParser {
    registry: LanguageRegistry,
    content_registry: ContentParserRegistry,
}

impl CodeParser {
    pub fn new() -> Self {
        Self {
            registry: LanguageRegistry::new(),
            content_registry: ContentParserRegistry::new(),
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

        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let rel_path = file_path.to_string_lossy().to_string();
        let source = std::fs::read_to_string(file_path)?;

        // 1. Try content parser by filename (Dockerfile, package.json, Cargo.toml)
        if let Some(parser) = self.content_registry.get_by_filename(filename) {
            return parser.parse(&rel_path, &source, repo_name);
        }

        // 2. Try content parser by extension (json, yaml, toml, md, sql, tf)
        if let Some(parser) = self.content_registry.get_by_extension(ext) {
            return parser.parse(&rel_path, &source, repo_name);
        }

        // 3. Try tree-sitter language by extension
        let language_config = self
            .registry
            .get_by_extension(ext)
            .ok_or_else(|| anyhow::anyhow!("Unsupported file extension: {}", ext))?;

        let mut parser = Parser::new();
        parser.set_language(&language_config.language)?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse: {}", file_path.display()))?;

        let extractor = EntityExtractor::new(
            rel_path,
            repo_name.to_string(),
            language_config.name.clone(),
        );

        extractor.extract(&tree, &source)
    }

    /// Check if a file extension or filename is supported
    pub fn supports_extension(&self, ext: &str) -> bool {
        self.registry.get_by_extension(ext).is_some()
            || self.content_registry.get_by_extension(ext).is_some()
    }

    /// Check if a filename is supported (for Dockerfile, package.json etc.)
    pub fn supports_filename(&self, filename: &str) -> bool {
        self.content_registry.get_by_filename(filename).is_some()
    }

    /// List all supported languages and content types
    pub fn supported_languages(&self) -> Vec<String> {
        let mut langs = self.registry.language_names();
        langs.extend(self.content_registry.parser_names());
        langs
    }
}

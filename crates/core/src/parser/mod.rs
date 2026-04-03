pub mod languages;
pub mod extractor;
pub mod content;

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;
use tree_sitter::Parser;

use crate::{CodeEntity, CodeRelation, EntityKind};
use languages::LanguageRegistry;
use extractor::EntityExtractor;
use content::ContentParserRegistry;

/// Check if a file should be skipped during indexing (build artifacts, generated code, etc.)
pub fn should_skip_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // Build artifact and dependency directories
    let skip_dirs = [
        "node_modules",
        "/build/",
        "\\build\\",
        "/target/",
        "\\target\\",
        ".dart_tool",
        "/dist/",
        "\\dist\\",
        "/.git/",
        "\\.git\\",
        "/vendor/",
        "\\vendor\\",
        "/__pycache__/",
        "\\__pycache__\\",
        "/.next/",
        "\\.next\\",
        "/coverage/",
        "\\coverage\\",
        "/.gradle/",
        "\\.gradle\\",
        "/.cache/",
        "\\.cache\\",
    ];

    for dir in &skip_dirs {
        if path_str.contains(dir) {
            return true;
        }
    }

    // Generated code patterns
    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
        let generated_suffixes = [
            ".g.dart",
            ".freezed.dart",
            ".generated.dart",
            ".config.dart",
            ".pb.go",
            "_generated.go",
            ".gen.ts",
            ".min.js",
            ".min.css",
        ];
        for pat in &generated_suffixes {
            if fname.ends_with(pat) {
                return true;
            }
        }

        // Lock files (not useful for code intelligence)
        let lock_files = [
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "Cargo.lock",
            "pubspec.lock",
            "Podfile.lock",
            "composer.lock",
            "Gemfile.lock",
        ];
        if lock_files.contains(&fname) {
            return true;
        }
    }

    // Large files (>512KB)
    if path.metadata().map(|m| m.len() > 512_000).unwrap_or(false) {
        return true;
    }

    false
}

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

    /// Parse a single file from disk and extract entities and relations.
    pub fn parse_file(
        &self,
        file_path: &Path,
        repo_name: &str,
    ) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let source = std::fs::read_to_string(file_path)?;
        self.parse_source(file_path, &source, repo_name)
    }

    /// Parse pre-read source content (avoids double file read for incremental indexing).
    /// Ensures a File entity always exists for incremental hash tracking.
    pub fn parse_source(
        &self,
        file_path: &Path,
        source: &str,
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

        // Dispatch to appropriate parser
        let (mut entities, relations) =
            if let Some(parser) = self.content_registry.get_by_filename(filename) {
                parser.parse(&rel_path, source, repo_name)?
            } else if let Some(parser) = self.content_registry.get_by_extension(ext) {
                parser.parse(&rel_path, source, repo_name)?
            } else {
                // Tree-sitter language
                let language_config = self
                    .registry
                    .get_by_extension(ext)
                    .ok_or_else(|| anyhow::anyhow!("Unsupported file extension: {}", ext))?;

                let mut parser = Parser::new();
                parser.set_language(&language_config.language)?;

                let tree = parser
                    .parse(source, None)
                    .ok_or_else(|| anyhow::anyhow!("Failed to parse: {}", file_path.display()))?;

                let extractor = EntityExtractor::new(
                    rel_path.clone(),
                    repo_name.to_string(),
                    language_config.name.clone(),
                );

                extractor.extract(&tree, source)?
            };

        // Ensure a File entity exists with a hash (required for incremental tracking).
        // Content parsers create File entities without hash; tree-sitter extractor
        // creates them with hash. Always compute and fill in missing hashes.
        let file_hash = {
            let mut hasher = Sha256::new();
            hasher.update(source.as_bytes());
            hex::encode(hasher.finalize())
        };

        if let Some(file_entity) = entities.iter_mut().find(|e| matches!(e.kind, EntityKind::File)) {
            // File entity exists (from content parser or extractor) — ensure hash is set
            if file_entity.body_hash.is_none() {
                file_entity.body_hash = Some(file_hash);
            }
        } else {
            // No File entity at all — create one
            let lang = if ext.is_empty() {
                filename.to_string()
            } else {
                ext.to_string()
            };

            entities.insert(
                0,
                CodeEntity {
                    kind: EntityKind::File,
                    name: rel_path.clone(),
                    qualified_name: format!("{}:{}", repo_name, rel_path),
                    file_path: rel_path,
                    repo: repo_name.to_string(),
                    start_line: 0,
                    end_line: source.lines().count() as u32,
                    start_col: 0,
                    end_col: 0,
                    signature: None,
                    body: None,
                    body_hash: Some(file_hash),
                    language: lang,
                },
            );
        }

        Ok((entities, relations))
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

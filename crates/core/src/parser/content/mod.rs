pub mod dockerfile_parser;
pub mod gradle_parser;
pub mod json_parser;
pub mod markdown_parser;
pub mod openapi_parser;
pub mod package_parser;
pub mod sql_parser;
pub mod terraform_parser;
pub mod toml_parser;
pub mod yaml_parser;

use crate::{CodeEntity, CodeRelation};
use anyhow::Result;

/// Trait for content parsers that don't use tree-sitter
pub trait ContentParser: Send + Sync {
    fn name(&self) -> &str;
    fn extensions(&self) -> &[&str];
    fn parse(
        &self,
        file_path: &str,
        source: &str,
        repo: &str,
    ) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)>;
}

/// Registry of all content parsers
pub struct ContentParserRegistry {
    parsers: Vec<Box<dyn ContentParser>>,
}

impl Default for ContentParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentParserRegistry {
    pub fn new() -> Self {
        let parsers: Vec<Box<dyn ContentParser>> = vec![
            Box::new(json_parser::JsonParser),
            Box::new(yaml_parser::YamlParser),
            Box::new(toml_parser::TomlParser),
            Box::new(markdown_parser::MarkdownParser),
            Box::new(dockerfile_parser::DockerfileParser),
            Box::new(sql_parser::SqlParser),
            Box::new(terraform_parser::TerraformParser),
            Box::new(openapi_parser::OpenApiParser),
            Box::new(package_parser::PackageParser),
            Box::new(gradle_parser::GradleParser),
        ];
        Self { parsers }
    }

    pub fn get_by_extension(&self, ext: &str) -> Option<&dyn ContentParser> {
        self.parsers
            .iter()
            .find(|p| p.extensions().contains(&ext))
            .map(|p| p.as_ref())
    }

    pub fn get_by_filename(&self, filename: &str) -> Option<&dyn ContentParser> {
        // Special case for files like Dockerfile, Makefile
        let lower = filename.to_lowercase();
        if lower == "dockerfile" || lower.starts_with("dockerfile.") {
            return self
                .parsers
                .iter()
                .find(|p| p.name() == "dockerfile")
                .map(|p| p.as_ref());
        }
        if lower == "package.json" || lower == "cargo.toml" {
            return self
                .parsers
                .iter()
                .find(|p| p.name() == "package")
                .map(|p| p.as_ref());
        }
        // Gradle: build.gradle, build.gradle.kts, settings.gradle, settings.gradle.kts
        if lower.ends_with(".gradle") || lower.ends_with(".gradle.kts") {
            return self
                .parsers
                .iter()
                .find(|p| p.name() == "gradle")
                .map(|p| p.as_ref());
        }
        None
    }

    pub fn parser_names(&self) -> Vec<String> {
        self.parsers.iter().map(|p| p.name().to_string()).collect()
    }
}

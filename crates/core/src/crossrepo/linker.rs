use anyhow::Result;
use serde::Deserialize;
use surrealdb::engine::local::Db;
use surrealdb::types::SurrealValue;
use surrealdb::Surreal;
use tracing::{debug, info};

/// Links entities across multiple repositories by resolving cross-repo imports.
pub struct CrossRepoLinker {
    db: Surreal<Db>,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct ImportRecord {
    body: Option<String>,
    repo: String,
    file_path: String,
    qualified_name: String,
}

#[derive(Debug, Deserialize, SurrealValue)]
struct FileMatch {
    qualified_name: String,
    repo: String,
}

impl CrossRepoLinker {
    pub fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// Resolve cross-repo import references and create `depends_on` relations.
    ///
    /// Scans all imports, extracts module names, then searches other repos
    /// for matching files. Creates graph edges for each resolved cross-repo link.
    pub async fn link_repos(&self) -> Result<usize> {
        let imports: Vec<ImportRecord> = self
            .db
            .query(
                "SELECT body, repo, file_path, qualified_name \
                 FROM import_decl WHERE body IS NOT NONE",
            )
            .await?
            .take(0)?;

        if imports.is_empty() {
            return Ok(0);
        }

        let mut links_created = 0;

        for imp in &imports {
            let body = match &imp.body {
                Some(b) if !b.is_empty() => b,
                _ => continue,
            };

            let module_name = extract_module_from_import(body);
            if module_name.is_empty() {
                continue;
            }

            let matches: Vec<FileMatch> = self
                .db
                .query(
                    "SELECT qualified_name, repo FROM file \
                     WHERE path CONTAINS $module AND repo != $source_repo \
                     LIMIT 5",
                )
                .bind(("module", module_name.clone()))
                .bind(("source_repo", imp.repo.clone()))
                .await?
                .take(0)?;

            for m in &matches {
                let from_id = crate::graph::builder::sanitize_id(&imp.qualified_name);
                let to_id = crate::graph::builder::sanitize_id(&m.qualified_name);

                let query = format!(
                    "RELATE import_decl:`{from_id}`->depends_on->file:`{to_id}` \
                     SET source_repo = $src, target_repo = $tgt, kind = 'cross_repo'"
                );

                match self
                    .db
                    .query(&query)
                    .bind(("src", imp.repo.clone()))
                    .bind(("tgt", m.repo.clone()))
                    .await
                {
                    Ok(_) => {
                        links_created += 1;
                        debug!(
                            "Cross-repo link: {}:{} -> {}:{}",
                            imp.repo, imp.file_path, m.repo, m.qualified_name
                        );
                    }
                    Err(e) => {
                        debug!("Failed to create cross-repo link: {e}");
                    }
                }
            }
        }

        if links_created > 0 {
            info!("Created {} cross-repo links", links_created);
        }

        Ok(links_created)
    }
}

/// Extract the module/package name from an import statement.
fn extract_module_from_import(import_text: &str) -> String {
    let text = import_text.trim();

    // Python: from X import Y
    if text.starts_with("from ") {
        if let Some(module) = text.strip_prefix("from ") {
            if let Some(idx) = module.find(" import") {
                return module[..idx].trim().replace('.', "/");
            }
        }
    }

    // TS/JS: import ... from 'path'
    if let Some(idx) = text.find("from ") {
        let rest = &text[idx + 5..];
        let path = rest
            .trim()
            .trim_matches(|c| c == '\'' || c == '"' || c == ';');
        return path.trim_start_matches("./").to_string();
    }

    // Rust: use foo::bar
    if text.starts_with("use ") {
        let path = text.strip_prefix("use ").unwrap_or(text);
        let path = path.trim_end_matches(';');
        return path.replace("::", "/");
    }

    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_python_import() {
        assert_eq!(
            extract_module_from_import("from foo.bar import baz"),
            "foo/bar"
        );
    }

    #[test]
    fn extract_js_import() {
        assert_eq!(
            extract_module_from_import("import { X } from 'shared/utils'"),
            "shared/utils"
        );
    }

    #[test]
    fn extract_rust_import() {
        assert_eq!(
            extract_module_from_import("use crate::graph::builder;"),
            "crate/graph/builder"
        );
    }

    #[test]
    fn extract_relative_js_import() {
        assert_eq!(
            extract_module_from_import("import X from './helpers'"),
            "helpers"
        );
    }
}

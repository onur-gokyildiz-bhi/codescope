use anyhow::Result;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use serde::Deserialize;

/// Links entities across multiple repositories
pub struct CrossRepoLinker {
    db: Surreal<Db>,
}

#[derive(Debug, Deserialize)]
struct ImportRecord {
    body: Option<String>,
    repo: String,
    #[allow(dead_code)]
    file_path: String,
}

impl CrossRepoLinker {
    pub fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// Attempt to resolve cross-repo import references
    pub async fn link_repos(&self) -> Result<usize> {
        let imports: Vec<ImportRecord> = self.db
            .query("SELECT body, repo, file_path FROM import_decl".to_string())
            .await?
            .take(0)?;

        let mut links_created = 0;

        for imp in imports {
            if let Some(body) = &imp.body {
                let module_name = extract_module_from_import(body);
                let source_repo = imp.repo.clone();

                let matches: Vec<serde_json::Value> = self.db
                    .query("SELECT qualified_name, repo FROM file WHERE path CONTAINS $module AND repo != $source_repo LIMIT 5".to_string())
                    .bind(("module", module_name))
                    .bind(("source_repo", source_repo))
                    .await?
                    .take(0)?;

                for _m in &matches {
                    links_created += 1;
                }
            }
        }

        Ok(links_created)
    }
}

/// Extract the module/package name from an import statement
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
        let path = rest.trim().trim_matches(|c| c == '\'' || c == '"' || c == ';');
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

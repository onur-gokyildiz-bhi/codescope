use anyhow::Result;
use serde::Deserialize;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

/// High-level graph query interface
pub struct GraphQuery {
    db: Surreal<Db>,
}

#[derive(Debug, serde::Serialize, Deserialize)]
pub struct SearchResult {
    pub qualified_name: Option<String>,
    pub name: Option<String>,
    pub file_path: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub language: Option<String>,
    pub signature: Option<String>,
}

impl GraphQuery {
    pub fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// Find a function by name (exact match)
    pub async fn find_function(&self, name: &str) -> Result<Vec<SearchResult>> {
        let name = name.to_string();
        let results: Vec<SearchResult> = self
            .db
            .query("SELECT qualified_name, name, file_path, start_line, end_line, language, signature FROM `function` WHERE name = $name")
            .bind(("name", name))
            .await?
            .take(0)?;
        Ok(results)
    }

    /// Search functions by name pattern
    pub async fn search_functions(&self, pattern: &str) -> Result<Vec<SearchResult>> {
        let pattern = pattern.to_string();
        let results: Vec<SearchResult> = self
            .db
            .query("SELECT qualified_name, name, file_path, start_line, end_line, language, signature FROM `function` WHERE string::contains(string::lowercase(name), string::lowercase($pattern))")
            .bind(("pattern", pattern))
            .await?
            .take(0)?;
        Ok(results)
    }

    /// Find all callers of a function
    pub async fn find_callers(&self, function_name: &str) -> Result<Vec<SearchResult>> {
        let name = function_name.to_string();
        let results: Vec<SearchResult> = self
            .db
            .query(
                "SELECT <-calls<-`function`.* AS callers FROM `function` WHERE name = $name"
            )
            .bind(("name", name))
            .await?
            .take(0)?;
        Ok(results)
    }

    /// Find all functions called by a function
    pub async fn find_callees(&self, function_name: &str) -> Result<Vec<SearchResult>> {
        let name = function_name.to_string();
        let results: Vec<SearchResult> = self
            .db
            .query(
                "SELECT ->calls->`function`.* AS callees FROM `function` WHERE name = $name"
            )
            .bind(("name", name))
            .await?
            .take(0)?;
        Ok(results)
    }

    /// Find all entities in a file
    pub async fn file_entities(&self, file_path: &str) -> Result<Vec<SearchResult>> {
        let mut all = Vec::new();

        let path = file_path.to_string();
        let functions: Vec<SearchResult> = self
            .db
            .query("SELECT qualified_name, name, file_path, start_line, end_line, language, signature FROM `function` WHERE file_path = $path")
            .bind(("path", path))
            .await?
            .take(0)?;
        all.extend(functions);

        let path = file_path.to_string();
        let classes: Vec<SearchResult> = self
            .db
            .query("SELECT qualified_name, name, file_path, start_line, end_line, language FROM class WHERE file_path = $path")
            .bind(("path", path))
            .await?
            .take(0)?;
        all.extend(classes);

        Ok(all)
    }

    /// Execute a raw SurrealQL query
    pub async fn raw_query(&self, query: &str) -> Result<serde_json::Value> {
        let query = query.to_string();
        let mut response = self.db.query(query).await?;
        let result: Vec<serde_json::Value> = response.take(0)?;
        Ok(serde_json::Value::Array(result))
    }

    /// Get graph statistics
    pub async fn stats(&self) -> Result<serde_json::Value> {
        let result = self.raw_query(
            "RETURN {
                files: (SELECT count() FROM file GROUP ALL),
                functions: (SELECT count() FROM `function` GROUP ALL),
                classes: (SELECT count() FROM class GROUP ALL),
                imports: (SELECT count() FROM import_decl GROUP ALL),
                contains_edges: (SELECT count() FROM contains GROUP ALL),
                calls_edges: (SELECT count() FROM calls GROUP ALL),
                imports_edges: (SELECT count() FROM imports GROUP ALL)
            };"
        ).await?;
        Ok(result)
    }
}

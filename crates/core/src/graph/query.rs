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

    /// Get graph statistics — full knowledge graph overview
    pub async fn stats(&self) -> Result<serde_json::Value> {
        let result = self.raw_query(
            "RETURN {
                files: (SELECT count() FROM file GROUP ALL),
                functions: (SELECT count() FROM `function` GROUP ALL),
                classes: (SELECT count() FROM class GROUP ALL),
                imports: (SELECT count() FROM import_decl GROUP ALL),
                configs: (SELECT count() FROM config GROUP ALL),
                docs: (SELECT count() FROM doc GROUP ALL),
                packages: (SELECT count() FROM package GROUP ALL),
                infra: (SELECT count() FROM infra GROUP ALL),
                contains_edges: (SELECT count() FROM contains GROUP ALL),
                calls_edges: (SELECT count() FROM calls GROUP ALL),
                imports_edges: (SELECT count() FROM imports GROUP ALL)
            };"
        ).await?;
        Ok(result)
    }

    // ===== Obsidian-like Context Exploration =====

    /// Explore the full neighborhood of an entity — local graph view.
    /// Finds the entity across all tables, then returns all connected nodes.
    pub async fn explore(&self, name: &str) -> Result<serde_json::Value> {
        let n = name.to_string();

        // Multi-statement: find entity + get neighborhood in one round-trip
        let mut response = self.db.query(
            "SELECT name, qualified_name, file_path, signature, start_line, end_line, \
                    'function' AS entity_type FROM `function` WHERE name = $name; \
             SELECT name, qualified_name, file_path, kind, start_line, end_line, \
                    'class' AS entity_type FROM class WHERE name = $name; \
             SELECT name, qualified_name, file_path, kind, \
                    'config' AS entity_type FROM config WHERE name = $name; \
             SELECT name, qualified_name, file_path, kind, \
                    'doc' AS entity_type FROM doc WHERE name = $name; \
             SELECT name, qualified_name, file_path, kind, \
                    'package' AS entity_type FROM package WHERE name = $name; \
             SELECT path AS name, language, 'file' AS entity_type FROM file WHERE path = $name;"
        ).bind(("name", n.clone())).await?;

        let functions: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let classes: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let configs: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let docs: Vec<serde_json::Value> = response.take(3).unwrap_or_default();
        let packages: Vec<serde_json::Value> = response.take(4).unwrap_or_default();
        let files: Vec<serde_json::Value> = response.take(5).unwrap_or_default();

        // Gather all matches
        let mut entity = serde_json::Map::new();
        let mut found_type = String::new();

        if !functions.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(functions));
            found_type = "function".into();
        } else if !classes.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(classes));
            found_type = "class".into();
        } else if !configs.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(configs));
            found_type = "config".into();
        } else if !docs.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(docs));
            found_type = "doc".into();
        } else if !packages.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(packages));
            found_type = "package".into();
        } else if !files.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(files));
            found_type = "file".into();
        }

        // Get neighborhood based on entity type
        if found_type == "function" {
            let mut resp2 = self.db.query(
                "SELECT name, file_path, signature FROM `function` \
                     WHERE name IN (SELECT VALUE ->calls->`function`.name FROM `function` WHERE name = $name LIMIT 1)[0]; \
                 SELECT name, file_path, signature FROM `function` \
                     WHERE name IN (SELECT VALUE <-calls<-`function`.name FROM `function` WHERE name = $name LIMIT 1)[0]; \
                 SELECT name, start_line, signature FROM `function` \
                     WHERE file_path IN (SELECT VALUE file_path FROM `function` WHERE name = $name LIMIT 1)[0] \
                     AND name != $name \
                     ORDER BY start_line;"
            ).bind(("name", n.clone())).await?;

            let callees: Vec<serde_json::Value> = resp2.take(0).unwrap_or_default();
            let callers: Vec<serde_json::Value> = resp2.take(1).unwrap_or_default();
            let siblings: Vec<serde_json::Value> = resp2.take(2).unwrap_or_default();

            entity.insert("calls_to".into(), serde_json::Value::Array(callees));
            entity.insert("called_by".into(), serde_json::Value::Array(callers));
            entity.insert("sibling_functions".into(), serde_json::Value::Array(siblings));
        } else if found_type == "file" {
            // For files, get all contained entities
            let file_ctx = self.file_context(&n).await?;
            return Ok(file_ctx);
        }

        entity.insert("entity_type".into(), serde_json::Value::String(found_type));
        Ok(serde_json::Value::Object(entity))
    }

    /// Full context for a file — everything connected to it (code, config, doc, deps).
    /// Like opening an Obsidian note with all backlinks and embeds visible.
    pub async fn file_context(&self, file_path: &str) -> Result<serde_json::Value> {
        let p = file_path.to_string();

        let mut response = self.db.query(
            "SELECT path, language, hash, line_count FROM file WHERE path = $path; \
             SELECT name, signature, start_line, end_line FROM `function` WHERE file_path = $path ORDER BY start_line; \
             SELECT name, kind, start_line, end_line FROM class WHERE file_path = $path ORDER BY start_line; \
             SELECT name FROM import_decl WHERE file_path = $path; \
             SELECT name, kind, body FROM config WHERE file_path = $path; \
             SELECT name, kind, body FROM doc WHERE file_path = $path; \
             SELECT name, kind FROM package WHERE file_path = $path; \
             SELECT name, kind FROM infra WHERE file_path = $path;"
        ).bind(("path", p.clone())).await?;

        let file_info: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let functions: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let classes: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let imports: Vec<serde_json::Value> = response.take(3).unwrap_or_default();
        let configs: Vec<serde_json::Value> = response.take(4).unwrap_or_default();
        let docs: Vec<serde_json::Value> = response.take(5).unwrap_or_default();
        let packages: Vec<serde_json::Value> = response.take(6).unwrap_or_default();
        let infra: Vec<serde_json::Value> = response.take(7).unwrap_or_default();

        // For each function, get callers from OTHER files (cross-file links)
        let mut fn_with_links = Vec::new();
        for func in &functions {
            let fname = func.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if fname.is_empty() { continue; }

            let mut resp2 = self.db.query(
                "SELECT name, file_path FROM `function` \
                     WHERE name IN (SELECT VALUE <-calls<-`function`.name FROM `function` \
                     WHERE name = $fname AND file_path = $fpath LIMIT 1)[0] \
                     AND file_path != $fpath;"
            ).bind(("fname", fname.to_string()))
             .bind(("fpath", p.clone()))
             .await?;

            let ext_callers: Vec<serde_json::Value> = resp2.take(0).unwrap_or_default();

            let mut entry = func.clone();
            if !ext_callers.is_empty() {
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("external_callers".into(), serde_json::Value::Array(ext_callers));
                }
            }
            fn_with_links.push(entry);
        }

        let mut ctx = serde_json::Map::new();
        ctx.insert("file".into(), file_info.into_iter().next().unwrap_or(serde_json::Value::Null));
        ctx.insert("functions".into(), serde_json::Value::Array(fn_with_links));
        ctx.insert("classes".into(), serde_json::Value::Array(classes));
        ctx.insert("imports".into(), serde_json::Value::Array(imports));
        if !configs.is_empty() { ctx.insert("configs".into(), serde_json::Value::Array(configs)); }
        if !docs.is_empty() { ctx.insert("docs".into(), serde_json::Value::Array(docs)); }
        if !packages.is_empty() { ctx.insert("packages".into(), serde_json::Value::Array(packages)); }
        if !infra.is_empty() { ctx.insert("infra".into(), serde_json::Value::Array(infra)); }

        Ok(serde_json::Value::Object(ctx))
    }

    /// Search across ALL entity types — universal knowledge graph search.
    /// Returns results grouped by type (code, config, doc, package, infra).
    pub async fn cross_search(&self, keyword: &str, limit: usize) -> Result<serde_json::Value> {
        let kw = keyword.to_string();
        let lim = limit as u32;

        let mut response = self.db.query(
            "SELECT name, file_path, start_line, signature, 'function' AS type \
                 FROM `function` WHERE string::contains(string::lowercase(name), string::lowercase($kw)) LIMIT $lim; \
             SELECT name, file_path, kind, start_line, 'class' AS type \
                 FROM class WHERE string::contains(string::lowercase(name), string::lowercase($kw)) LIMIT $lim; \
             SELECT name, file_path, kind, body, 'config' AS type \
                 FROM config WHERE string::contains(string::lowercase(name), string::lowercase($kw)) LIMIT $lim; \
             SELECT name, file_path, kind, body, 'doc' AS type \
                 FROM doc WHERE string::contains(string::lowercase(name), string::lowercase($kw)) LIMIT $lim; \
             SELECT name, file_path, kind, 'package' AS type \
                 FROM package WHERE string::contains(string::lowercase(name), string::lowercase($kw)) LIMIT $lim; \
             SELECT path AS name, language, 'file' AS type \
                 FROM file WHERE string::contains(string::lowercase(path), string::lowercase($kw)) LIMIT $lim; \
             SELECT name, file_path, 'import' AS type \
                 FROM import_decl WHERE string::contains(string::lowercase(name), string::lowercase($kw)) LIMIT $lim; \
             SELECT name, file_path, kind, 'infra' AS type \
                 FROM infra WHERE string::contains(string::lowercase(name), string::lowercase($kw)) LIMIT $lim;"
        ).bind(("kw", kw)).bind(("lim", lim)).await?;

        let functions: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let classes: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let configs: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let docs: Vec<serde_json::Value> = response.take(3).unwrap_or_default();
        let packages: Vec<serde_json::Value> = response.take(4).unwrap_or_default();
        let files: Vec<serde_json::Value> = response.take(5).unwrap_or_default();
        let imports: Vec<serde_json::Value> = response.take(6).unwrap_or_default();
        let infra: Vec<serde_json::Value> = response.take(7).unwrap_or_default();

        let mut result = serde_json::Map::new();
        let total = functions.len() + classes.len() + configs.len() + docs.len()
            + packages.len() + files.len() + imports.len() + infra.len();
        result.insert("total_results".into(), serde_json::Value::Number(total.into()));
        if !functions.is_empty() { result.insert("functions".into(), serde_json::Value::Array(functions)); }
        if !classes.is_empty() { result.insert("classes".into(), serde_json::Value::Array(classes)); }
        if !configs.is_empty() { result.insert("configs".into(), serde_json::Value::Array(configs)); }
        if !docs.is_empty() { result.insert("docs".into(), serde_json::Value::Array(docs)); }
        if !packages.is_empty() { result.insert("packages".into(), serde_json::Value::Array(packages)); }
        if !files.is_empty() { result.insert("files".into(), serde_json::Value::Array(files)); }
        if !imports.is_empty() { result.insert("imports".into(), serde_json::Value::Array(imports)); }
        if !infra.is_empty() { result.insert("infra".into(), serde_json::Value::Array(infra)); }

        Ok(serde_json::Value::Object(result))
    }

    /// Find all incoming references to an entity — Obsidian-like backlinks.
    /// "What calls/imports/contains/depends on this?"
    pub async fn backlinks(&self, name: &str) -> Result<serde_json::Value> {
        let n = name.to_string();

        // Multi-direction backlink search
        let mut response = self.db.query(
            "SELECT name, file_path, signature, 'caller' AS link_type \
                 FROM `function` WHERE name IN \
                 (SELECT VALUE <-calls<-`function`.name FROM `function` WHERE name = $name LIMIT 1)[0]; \
             SELECT name, file_path, 'importer' AS link_type \
                 FROM import_decl WHERE string::contains(name, $name); \
             SELECT path AS name, language, 'container' AS link_type \
                 FROM file WHERE path IN \
                 (SELECT VALUE file_path FROM `function` WHERE name = $name) \
                 OR path IN (SELECT VALUE file_path FROM class WHERE name = $name) \
                 OR path IN (SELECT VALUE file_path FROM config WHERE name = $name); \
             SELECT name, kind, file_path, 'dependent' AS link_type \
                 FROM package WHERE kind = 'Dependency' AND name = $name;"
        ).bind(("name", n)).await?;

        let callers: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let importers: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let containers: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let dependents: Vec<serde_json::Value> = response.take(3).unwrap_or_default();

        let mut result = serde_json::Map::new();
        let total = callers.len() + importers.len() + containers.len() + dependents.len();
        result.insert("total_backlinks".into(), serde_json::Value::Number(total.into()));
        if !callers.is_empty() { result.insert("callers".into(), serde_json::Value::Array(callers)); }
        if !importers.is_empty() { result.insert("importers".into(), serde_json::Value::Array(importers)); }
        if !containers.is_empty() { result.insert("contained_in".into(), serde_json::Value::Array(containers)); }
        if !dependents.is_empty() { result.insert("dependents".into(), serde_json::Value::Array(dependents)); }

        Ok(serde_json::Value::Object(result))
    }
}

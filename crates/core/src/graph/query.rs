use anyhow::Result;
use serde::Deserialize;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

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
        let pattern = pattern.to_lowercase(); // Lowercase once in Rust, not per-row in DB
        let results: Vec<SearchResult> = self
            .db
            .query("SELECT qualified_name, name, file_path, start_line, end_line, language, signature FROM `function` WHERE string::contains(string::lowercase(name), $pattern)")
            .bind(("pattern", pattern))
            .await?
            .take(0)?;
        Ok(results)
    }

    /// Find all callers of a function
    pub async fn find_callers(&self, function_name: &str) -> Result<Vec<SearchResult>> {
        let name = function_name.to_string();
        // Use direct edge traversal — much faster than subquery on large graphs
        let results: Vec<SearchResult> = self
            .db
            .query(
                "SELECT in.qualified_name AS qualified_name, in.name AS name, \
                 in.file_path AS file_path, in.start_line AS start_line, \
                 in.end_line AS end_line, in.language AS language, in.signature AS signature \
                 FROM calls WHERE out.name = $name AND in.name != NONE",
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
                "SELECT out.qualified_name AS qualified_name, out.name AS name, \
                 out.file_path AS file_path, out.start_line AS start_line, \
                 out.end_line AS end_line, out.language AS language, out.signature AS signature \
                 FROM calls WHERE in.name = $name AND out.name != NONE",
            )
            .bind(("name", name))
            .await?
            .take(0)?;
        Ok(results)
    }

    /// Find all entities in a file — single round-trip for both functions and classes
    pub async fn file_entities(&self, file_path: &str) -> Result<Vec<SearchResult>> {
        let path = file_path.to_string();

        let mut response = self.db
            .query(
                "SELECT qualified_name, name, file_path, start_line, end_line, language, signature \
                 FROM `function` WHERE file_path = $path; \
                 SELECT qualified_name, name, file_path, start_line, end_line, language \
                 FROM class WHERE file_path = $path;"
            )
            .bind(("path", path))
            .await?;

        let functions: Vec<SearchResult> = response.take(0).unwrap_or_default();
        let classes: Vec<SearchResult> = response.take(1).unwrap_or_default();

        let mut all = Vec::with_capacity(functions.len() + classes.len());
        all.extend(functions);
        all.extend(classes);
        Ok(all)
    }

    /// Execute a raw SurrealQL query
    pub async fn raw_query(&self, query: &str) -> Result<serde_json::Value> {
        let query_str = query.to_string();
        // Count semicolons to know how many statements to collect
        let stmt_count = query_str.matches(';').count() + 1;
        let mut response = self.db.query(query_str).await?;

        // Collect results from ALL statements (not just index 0)
        let mut all_results = Vec::new();
        for i in 0..stmt_count {
            match response.take::<Vec<serde_json::Value>>(i) {
                Ok(result) => all_results.push(serde_json::Value::Array(result)),
                Err(_) => break, // No more results
            }
        }

        // If single statement, return flat array for backward compatibility
        if all_results.len() <= 1 {
            Ok(all_results
                .into_iter()
                .next()
                .unwrap_or(serde_json::Value::Array(vec![])))
        } else {
            Ok(serde_json::Value::Array(all_results))
        }
    }

    /// Get graph statistics — full knowledge graph overview
    pub async fn stats(&self) -> Result<serde_json::Value> {
        let result = self
            .raw_query(
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
            };",
            )
            .await?;
        Ok(result)
    }

    // ===== Obsidian-like Context Exploration =====

    /// Explore the full neighborhood of an entity — local graph view.
    /// Finds the entity across all tables, then returns all connected nodes.
    pub async fn explore(&self, name: &str) -> Result<serde_json::Value> {
        let n = name.to_string();

        // Multi-statement: find entity + get neighborhood in one round-trip
        let mut response = self
            .db
            .query(
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
             SELECT path AS name, language, 'file' AS entity_type FROM file WHERE path = $name; \
             SELECT name, qualified_name, file_path, description, node_type, \
                    'skill' AS entity_type FROM skill WHERE name = $name;",
            )
            .bind(("name", n.clone()))
            .await?;

        let functions: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let classes: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let configs: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let docs: Vec<serde_json::Value> = response.take(3).unwrap_or_default();
        let packages: Vec<serde_json::Value> = response.take(4).unwrap_or_default();
        let files: Vec<serde_json::Value> = response.take(5).unwrap_or_default();
        let skills: Vec<serde_json::Value> = response.take(6).unwrap_or_default();

        // Gather all matches
        let mut entity = serde_json::Map::new();
        let mut found_type = String::new();

        if !functions.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(functions));
            found_type = "function".into();
        } else if !classes.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(classes));
            found_type = "class".into();
        } else if !skills.is_empty() {
            entity.insert("matches".into(), serde_json::Value::Array(skills));
            found_type = "skill".into();
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
                "SELECT out.name AS name, out.file_path AS file_path, out.signature AS signature \
                     FROM calls WHERE in.name = $name AND out.name != NONE; \
                 SELECT in.name AS name, in.file_path AS file_path, in.signature AS signature \
                     FROM calls WHERE out.name = $name AND in.name != NONE; \
                 SELECT name, start_line, signature FROM `function` \
                     WHERE file_path IN (SELECT VALUE file_path FROM `function` WHERE name = $name LIMIT 1)[0] \
                     AND name != $name \
                     ORDER BY start_line LIMIT 50;"
            ).bind(("name", n.clone())).await?;

            let callees: Vec<serde_json::Value> = resp2.take(0).unwrap_or_default();
            let callers: Vec<serde_json::Value> = resp2.take(1).unwrap_or_default();
            let siblings: Vec<serde_json::Value> = resp2.take(2).unwrap_or_default();

            entity.insert("calls_to".into(), serde_json::Value::Array(callees));
            entity.insert("called_by".into(), serde_json::Value::Array(callers));
            entity.insert(
                "sibling_functions".into(),
                serde_json::Value::Array(siblings),
            );
        } else if found_type == "skill" {
            // For skills, get wikilink neighbors
            let mut resp2 = self
                .db
                .query(
                    "SELECT out.name AS name, out.description AS description, \
                        out.node_type AS node_type, context \
                     FROM links_to WHERE in.name = $name; \
                 SELECT in.name AS name, in.description AS description, \
                        in.node_type AS node_type, context \
                     FROM links_to WHERE out.name = $name;",
                )
                .bind(("name", n.clone()))
                .await?;

            let links_to: Vec<serde_json::Value> = resp2.take(0).unwrap_or_default();
            let linked_from: Vec<serde_json::Value> = resp2.take(1).unwrap_or_default();

            entity.insert("links_to".into(), serde_json::Value::Array(links_to));
            entity.insert("linked_from".into(), serde_json::Value::Array(linked_from));
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

        // Batch query: get ALL cross-file callers for ALL functions in this file at once (avoids N+1)
        let mut resp2 = self
            .db
            .query(
                "SELECT out.name AS callee_name, in.name AS name, in.file_path AS file_path \
                 FROM calls WHERE out.file_path = $fpath \
                 AND in.name != NONE AND in.file_path != $fpath;",
            )
            .bind(("fpath", p.clone()))
            .await?;

        let all_ext_callers: Vec<serde_json::Value> = resp2.take(0).unwrap_or_default();

        // Group external callers by callee function name
        let mut caller_map: std::collections::HashMap<String, Vec<serde_json::Value>> =
            std::collections::HashMap::new();
        for caller in all_ext_callers {
            if let Some(callee) = caller.get("callee_name").and_then(|v| v.as_str()) {
                let mut entry = caller.clone();
                if let Some(obj) = entry.as_object_mut() {
                    obj.remove("callee_name");
                }
                caller_map
                    .entry(callee.to_string())
                    .or_default()
                    .push(entry);
            }
        }

        let mut fn_with_links = Vec::new();
        for func in &functions {
            let fname = func.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let mut entry = func.clone();
            if let Some(ext_callers) = caller_map.remove(fname) {
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert(
                        "external_callers".into(),
                        serde_json::Value::Array(ext_callers),
                    );
                }
            }
            fn_with_links.push(entry);
        }

        let mut ctx = serde_json::Map::new();
        ctx.insert(
            "file".into(),
            file_info
                .into_iter()
                .next()
                .unwrap_or(serde_json::Value::Null),
        );
        ctx.insert("functions".into(), serde_json::Value::Array(fn_with_links));
        ctx.insert("classes".into(), serde_json::Value::Array(classes));
        ctx.insert("imports".into(), serde_json::Value::Array(imports));
        if !configs.is_empty() {
            ctx.insert("configs".into(), serde_json::Value::Array(configs));
        }
        if !docs.is_empty() {
            ctx.insert("docs".into(), serde_json::Value::Array(docs));
        }
        if !packages.is_empty() {
            ctx.insert("packages".into(), serde_json::Value::Array(packages));
        }
        if !infra.is_empty() {
            ctx.insert("infra".into(), serde_json::Value::Array(infra));
        }

        Ok(serde_json::Value::Object(ctx))
    }

    /// Search across ALL entity types — universal knowledge graph search.
    /// Returns results grouped by type (code, config, doc, package, infra).
    pub async fn cross_search(&self, keyword: &str, limit: usize) -> Result<serde_json::Value> {
        // Compute lowercase once in Rust — avoids 16x string::lowercase() calls in DB
        let kw = keyword.to_lowercase();
        let lim = limit as u32;

        let mut response = self
            .db
            .query(
                "SELECT name, file_path, start_line, signature, 'function' AS type \
                 FROM `function` WHERE string::contains(string::lowercase(name), $kw) LIMIT $lim; \
             SELECT name, file_path, kind, start_line, 'class' AS type \
                 FROM class WHERE string::contains(string::lowercase(name), $kw) LIMIT $lim; \
             SELECT name, file_path, kind, body, 'config' AS type \
                 FROM config WHERE string::contains(string::lowercase(name), $kw) LIMIT $lim; \
             SELECT name, file_path, kind, body, 'doc' AS type \
                 FROM doc WHERE string::contains(string::lowercase(name), $kw) LIMIT $lim; \
             SELECT name, file_path, kind, 'package' AS type \
                 FROM package WHERE string::contains(string::lowercase(name), $kw) LIMIT $lim; \
             SELECT path AS name, language, 'file' AS type \
                 FROM file WHERE string::contains(string::lowercase(path), $kw) LIMIT $lim; \
             SELECT name, file_path, 'import' AS type \
                 FROM import_decl WHERE string::contains(string::lowercase(name), $kw) LIMIT $lim; \
             SELECT name, file_path, kind, 'infra' AS type \
                 FROM infra WHERE string::contains(string::lowercase(name), $kw) LIMIT $lim;",
            )
            .bind(("kw", kw))
            .bind(("lim", lim))
            .await?;

        let functions: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let classes: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let configs: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let docs: Vec<serde_json::Value> = response.take(3).unwrap_or_default();
        let packages: Vec<serde_json::Value> = response.take(4).unwrap_or_default();
        let files: Vec<serde_json::Value> = response.take(5).unwrap_or_default();
        let imports: Vec<serde_json::Value> = response.take(6).unwrap_or_default();
        let infra: Vec<serde_json::Value> = response.take(7).unwrap_or_default();

        let mut result = serde_json::Map::new();
        let total = functions.len()
            + classes.len()
            + configs.len()
            + docs.len()
            + packages.len()
            + files.len()
            + imports.len()
            + infra.len();
        result.insert(
            "total_results".into(),
            serde_json::Value::Number(total.into()),
        );
        if !functions.is_empty() {
            result.insert("functions".into(), serde_json::Value::Array(functions));
        }
        if !classes.is_empty() {
            result.insert("classes".into(), serde_json::Value::Array(classes));
        }
        if !configs.is_empty() {
            result.insert("configs".into(), serde_json::Value::Array(configs));
        }
        if !docs.is_empty() {
            result.insert("docs".into(), serde_json::Value::Array(docs));
        }
        if !packages.is_empty() {
            result.insert("packages".into(), serde_json::Value::Array(packages));
        }
        if !files.is_empty() {
            result.insert("files".into(), serde_json::Value::Array(files));
        }
        if !imports.is_empty() {
            result.insert("imports".into(), serde_json::Value::Array(imports));
        }
        if !infra.is_empty() {
            result.insert("infra".into(), serde_json::Value::Array(infra));
        }

        Ok(serde_json::Value::Object(result))
    }

    // ===== HTTP Cross-Service Linking =====

    /// Find all HTTP client calls in the codebase, optionally filtered by method.
    pub async fn find_http_calls(&self, method: Option<&str>) -> Result<Vec<serde_json::Value>> {
        if let Some(m) = method {
            let m = m.to_uppercase();
            let results: Vec<serde_json::Value> = self
                .db
                .query(
                    "SELECT name, qualified_name, file_path, start_line, end_line, kind AS method, body \
                     FROM http_call WHERE kind = $method ORDER BY file_path, start_line"
                )
                .bind(("method", m))
                .await?
                .take(0)?;
            Ok(results)
        } else {
            let results: Vec<serde_json::Value> = self
                .db
                .query(
                    "SELECT name, qualified_name, file_path, start_line, end_line, kind AS method \
                     FROM http_call ORDER BY file_path, start_line",
                )
                .await?
                .take(0)?;
            Ok(results)
        }
    }

    /// Find which functions make HTTP calls to a given endpoint path pattern.
    pub async fn find_endpoint_callers(
        &self,
        endpoint_pattern: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let pattern = endpoint_pattern.to_lowercase();
        let results: Vec<serde_json::Value> = self
            .db
            .query(
                "SELECT in.name AS caller_name, in.file_path AS caller_file, \
                 in.signature AS caller_signature, \
                 out.name AS http_call, out.kind AS method, out.file_path AS call_file, \
                 out.start_line AS call_line \
                 FROM calls_endpoint WHERE string::contains(string::lowercase(out.name), $pattern)",
            )
            .bind(("pattern", pattern))
            .await?
            .take(0)?;
        Ok(results)
    }

    // ===== Type Hierarchy =====

    /// Traverse the type hierarchy for a class/struct/trait/interface.
    /// Shows parents (supertype chain), children (subtypes), interfaces, and implementors.
    pub async fn type_hierarchy(&self, name: &str, depth: usize) -> Result<serde_json::Value> {
        let max_depth = depth.min(5); // Cap at 5 levels
        self.type_hierarchy_recursive(name, max_depth, 0).await
    }

    /// Recursive type hierarchy traversal with visited tracking via depth limit.
    fn type_hierarchy_recursive<'a>(
        &'a self,
        name: &'a str,
        max_depth: usize,
        current_depth: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        Box::pin(async move {
            let n = name.to_string();

            let mut response = self.db.query(
            "SELECT name, qualified_name, kind, file_path, start_line, end_line \
                FROM class WHERE name = $name; \
             SELECT ->inherits->class.name AS parent, ->inherits->class.kind AS parent_kind, \
                    ->inherits->class.file_path AS parent_file \
                FROM class WHERE name = $name; \
             SELECT <-inherits<-class.name AS child, <-inherits<-class.kind AS child_kind, \
                    <-inherits<-class.file_path AS child_file \
                FROM class WHERE name = $name; \
             SELECT ->implements->class.name AS iface, ->implements->class.kind AS iface_kind \
                FROM class WHERE name = $name; \
             SELECT <-implements<-class.name AS implementor, <-implements<-class.kind AS impl_kind, \
                    <-implements<-class.file_path AS impl_file \
                FROM class WHERE name = $name;"
        ).bind(("name", n)).await?;

            let entities: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
            let parents: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
            let children: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
            let interfaces: Vec<serde_json::Value> = response.take(3).unwrap_or_default();
            let implementors: Vec<serde_json::Value> = response.take(4).unwrap_or_default();

            let mut result = serde_json::Map::new();
            result.insert("name".into(), serde_json::Value::String(name.to_string()));
            result.insert(
                "depth".into(),
                serde_json::Value::Number(current_depth.into()),
            );

            if let Some(entity) = entities.into_iter().next() {
                result.insert("entity".into(), entity);
            }

            // Recursively expand parents up to max_depth
            if !parents.is_empty() {
                if current_depth + 1 < max_depth {
                    let mut expanded = Vec::new();
                    for p in &parents {
                        if let Some(parent_name) = p.get("parent").and_then(|v| v.as_str()) {
                            match self
                                .type_hierarchy_recursive(parent_name, max_depth, current_depth + 1)
                                .await
                            {
                                Ok(tree) => expanded.push(tree),
                                Err(_) => expanded.push(p.clone()),
                            }
                        } else {
                            expanded.push(p.clone());
                        }
                    }
                    result.insert("parents".into(), serde_json::Value::Array(expanded));
                } else {
                    result.insert("parents".into(), serde_json::Value::Array(parents));
                }
            }

            // Recursively expand children up to max_depth
            if !children.is_empty() {
                if current_depth + 1 < max_depth {
                    let mut expanded = Vec::new();
                    for c in &children {
                        if let Some(child_name) = c.get("child").and_then(|v| v.as_str()) {
                            match self
                                .type_hierarchy_recursive(child_name, max_depth, current_depth + 1)
                                .await
                            {
                                Ok(tree) => expanded.push(tree),
                                Err(_) => expanded.push(c.clone()),
                            }
                        } else {
                            expanded.push(c.clone());
                        }
                    }
                    result.insert("children".into(), serde_json::Value::Array(expanded));
                } else {
                    result.insert("children".into(), serde_json::Value::Array(children));
                }
            }

            if !interfaces.is_empty() {
                result.insert("implements".into(), serde_json::Value::Array(interfaces));
            }
            if !implementors.is_empty() {
                result.insert(
                    "implemented_by".into(),
                    serde_json::Value::Array(implementors),
                );
            }

            Ok(serde_json::Value::Object(result))
        }) // async move
    }

    // ===== Skill Graph Traversal =====

    /// Traverse the skill/knowledge graph with progressive disclosure.
    pub async fn traverse_skill_graph(
        &self,
        name: &str,
        _depth: usize,
        detail_level: usize,
    ) -> Result<serde_json::Value> {
        let n = name.to_string();

        // Level 1: Find the root skill node
        let mut response = self
            .db
            .query(
                "SELECT name, qualified_name, kind, file_path, description, node_type, created \
             FROM skill WHERE name = $name OR string::contains(qualified_name, $name) LIMIT 5",
            )
            .bind(("name", n.clone()))
            .await?;

        let roots: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        if roots.is_empty() {
            return Ok(
                serde_json::json!({"error": format!("No skill node found matching '{}'", name)}),
            );
        }

        let root = &roots[0];
        let qname = root
            .get("qualified_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let file_path = root.get("file_path").and_then(|v| v.as_str()).unwrap_or("");

        let mut result = serde_json::Map::new();
        result.insert("skill".into(), root.clone());

        if detail_level < 2 {
            return Ok(serde_json::Value::Object(result));
        }

        // Level 2: Get outgoing and incoming wikilinks
        let qn = qname.to_string();
        let mut resp2 = self
            .db
            .query(
                "SELECT out.name AS name, out.description AS description, \
                    out.node_type AS node_type, context \
             FROM links_to WHERE in.qualified_name = $qn; \
             SELECT in.name AS name, in.description AS description, \
                    in.node_type AS node_type, context \
             FROM links_to WHERE out.qualified_name = $qn;",
            )
            .bind(("qn", qn))
            .await?;

        let links_to: Vec<serde_json::Value> = resp2.take(0).unwrap_or_default();
        let linked_from: Vec<serde_json::Value> = resp2.take(1).unwrap_or_default();

        result.insert("links_to".into(), serde_json::Value::Array(links_to));
        result.insert("linked_from".into(), serde_json::Value::Array(linked_from));

        if detail_level < 3 {
            return Ok(serde_json::Value::Object(result));
        }

        // Level 3: Get sections (headings) from the same file
        let fp = file_path.to_string();
        let mut resp3 = self
            .db
            .query(
                "SELECT name, kind, start_line FROM doc WHERE file_path = $fp \
             AND kind = 'DocSection' ORDER BY start_line",
            )
            .bind(("fp", fp))
            .await?;

        let sections: Vec<serde_json::Value> = resp3.take(0).unwrap_or_default();
        result.insert("sections".into(), serde_json::Value::Array(sections));

        if detail_level < 4 {
            return Ok(serde_json::Value::Object(result));
        }

        // Level 4: Full body content
        let body = root.get("body").cloned().unwrap_or(serde_json::Value::Null);
        result.insert("full_content".into(), body);

        Ok(serde_json::Value::Object(result))
    }

    // ===== Symbol-Level Operations =====

    /// Find all references to a symbol (function/class) across the codebase.
    /// Used by rename_symbol to show what would change.
    pub async fn find_all_references(&self, name: &str) -> Result<serde_json::Value> {
        let n = name.to_string();

        let mut response = self
            .db
            .query(
                // Definition sites
                "SELECT name, qualified_name, file_path, start_line, end_line, signature, \
                    'definition' AS ref_type FROM `function` WHERE name = $name; \
             SELECT name, qualified_name, file_path, start_line, end_line, kind, \
                    'definition' AS ref_type FROM class WHERE name = $name; \
             // Call sites (where this function is called)
             SELECT in.name AS caller_name, in.file_path AS file_path, \
                    in.start_line AS start_line, 'call_site' AS ref_type, \
                    meta::id(id) AS edge_id \
                    FROM calls WHERE out.name = $name AND in.name != NONE; \
             // Import references
             SELECT name, file_path, start_line, 'import' AS ref_type \
                    FROM import_decl WHERE string::contains(name, $name);",
            )
            .bind(("name", n))
            .await?;

        let definitions: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let class_defs: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let call_sites: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let imports: Vec<serde_json::Value> = response.take(3).unwrap_or_default();

        let mut all_refs = Vec::new();
        all_refs.extend(definitions);
        all_refs.extend(class_defs);
        all_refs.extend(call_sites);
        all_refs.extend(imports);

        let mut result = serde_json::Map::new();
        result.insert("symbol".into(), serde_json::Value::String(name.to_string()));
        result.insert(
            "total_references".into(),
            serde_json::Value::Number(all_refs.len().into()),
        );
        result.insert("references".into(), serde_json::Value::Array(all_refs));

        Ok(serde_json::Value::Object(result))
    }

    /// Find unused symbols — functions/classes with zero callers/importers.
    /// Filters out entry points (main, test_, handler, new, init).
    pub async fn find_unused_symbols(&self, min_lines: u32) -> Result<Vec<serde_json::Value>> {
        let results: Vec<serde_json::Value> = self
            .db
            .query(
                "SELECT name, file_path, start_line, end_line, signature, \
                    (end_line - start_line) AS line_count \
             FROM `function` WHERE \
                 name NOT IN (SELECT VALUE out.name FROM calls WHERE out.name != NONE) \
                 AND name != 'main' \
                 AND string::starts_with(name, 'test_') = false \
                 AND string::starts_with(name, 'Test') = false \
                 AND name != 'new' AND name != 'init' AND name != 'setup' \
                 AND name != 'default' AND name != 'from' AND name != 'into' \
                 AND name != 'drop' AND name != 'clone' AND name != 'fmt' \
                 AND name != 'serialize' AND name != 'deserialize' \
                 AND (end_line - start_line) >= $min_lines \
             ORDER BY (end_line - start_line) DESC \
             LIMIT 50",
            )
            .bind(("min_lines", min_lines))
            .await?
            .take(0)?;

        Ok(results)
    }

    /// Check if a symbol can be safely deleted — zero references anywhere.
    pub async fn safe_delete_check(&self, name: &str) -> Result<serde_json::Value> {
        let n = name.to_string();

        let mut response = self
            .db
            .query(
                // Check callers
                "SELECT count() AS cnt FROM calls WHERE out.name = $name GROUP ALL; \
             // Check if imported anywhere
             SELECT count() AS cnt FROM import_decl WHERE string::contains(name, $name) GROUP ALL; \
             // Get the entity details
             SELECT name, file_path, start_line, end_line, signature \
                    FROM `function` WHERE name = $name; \
             SELECT name, file_path, start_line, end_line, kind \
                    FROM class WHERE name = $name;",
            )
            .bind(("name", n))
            .await?;

        let callers: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let importers: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let fn_defs: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let class_defs: Vec<serde_json::Value> = response.take(3).unwrap_or_default();

        let caller_count = callers
            .first()
            .and_then(|v| v.get("cnt"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let import_count = importers
            .first()
            .and_then(|v| v.get("cnt"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let is_safe = caller_count == 0 && import_count == 0;

        let mut definitions = Vec::new();
        definitions.extend(fn_defs);
        definitions.extend(class_defs);

        let mut result = serde_json::Map::new();
        result.insert("symbol".into(), serde_json::Value::String(name.to_string()));
        result.insert("safe_to_delete".into(), serde_json::Value::Bool(is_safe));
        result.insert(
            "caller_count".into(),
            serde_json::Value::Number(caller_count.into()),
        );
        result.insert(
            "import_count".into(),
            serde_json::Value::Number(import_count.into()),
        );
        result.insert("definitions".into(), serde_json::Value::Array(definitions));
        if !is_safe {
            result.insert(
                "reason".into(),
                serde_json::Value::String(format!(
                    "{} callers, {} imports still reference this symbol",
                    caller_count, import_count
                )),
            );
        }

        Ok(serde_json::Value::Object(result))
    }

    /// Find all incoming references to an entity — Obsidian-like backlinks.
    /// "What calls/imports/contains/depends on this?"
    pub async fn backlinks(&self, name: &str) -> Result<serde_json::Value> {
        let n = name.to_string();

        // Multi-direction backlink search
        let mut response = self.db.query(
            "SELECT in.name AS name, in.file_path AS file_path, in.signature AS signature, 'caller' AS link_type \
                 FROM calls WHERE out.name = $name AND in.name != NONE; \
             SELECT name, file_path, 'importer' AS link_type \
                 FROM import_decl WHERE string::contains(name, $name); \
             SELECT path AS name, language, 'container' AS link_type \
                 FROM file WHERE path IN \
                 (SELECT VALUE file_path FROM `function` WHERE name = $name) \
                 OR path IN (SELECT VALUE file_path FROM class WHERE name = $name) \
                 OR path IN (SELECT VALUE file_path FROM config WHERE name = $name); \
             SELECT name, kind, file_path, 'dependent' AS link_type \
                 FROM package WHERE kind = 'Dependency' AND name = $name; \
             SELECT in.name AS name, in.file_path AS file_path, in.description AS description, \
                    'wikilink' AS link_type, context \
                 FROM links_to WHERE out.name = $name;"
        ).bind(("name", n)).await?;

        let callers: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let importers: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let containers: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let dependents: Vec<serde_json::Value> = response.take(3).unwrap_or_default();
        let wikilinks: Vec<serde_json::Value> = response.take(4).unwrap_or_default();

        let mut result = serde_json::Map::new();
        let total =
            callers.len() + importers.len() + containers.len() + dependents.len() + wikilinks.len();
        result.insert(
            "total_backlinks".into(),
            serde_json::Value::Number(total.into()),
        );
        if !callers.is_empty() {
            result.insert("callers".into(), serde_json::Value::Array(callers));
        }
        if !importers.is_empty() {
            result.insert("importers".into(), serde_json::Value::Array(importers));
        }
        if !containers.is_empty() {
            result.insert("contained_in".into(), serde_json::Value::Array(containers));
        }
        if !dependents.is_empty() {
            result.insert("dependents".into(), serde_json::Value::Array(dependents));
        }
        if !wikilinks.is_empty() {
            result.insert("wikilinks".into(), serde_json::Value::Array(wikilinks));
        }

        Ok(serde_json::Value::Object(result))
    }
}

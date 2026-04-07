use anyhow::Result;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use tracing::{debug, warn};

use crate::{CodeEntity, CodeRelation, EntityKind, IndexResult};

/// Batch size for multi-statement DB queries.
/// 200 entities per round-trip balances throughput vs memory.
const BATCH_SIZE: usize = 200;

/// Builds the code knowledge graph in SurrealDB
pub struct GraphBuilder {
    db: Surreal<Db>,
}

impl GraphBuilder {
    pub fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// Insert entities in batches using multi-statement UPSERT SET.
    ///
    /// Groups up to BATCH_SIZE entities per DB round-trip. Uses UPSERT SET
    /// to handle both new and existing records (preserves fields like embeddings).
    /// Falls back to individual inserts on batch failure.
    pub async fn insert_entities(&self, entities: &[CodeEntity]) -> Result<usize> {
        if entities.is_empty() {
            return Ok(0);
        }

        let mut total = 0;

        for chunk in entities.chunks(BATCH_SIZE) {
            let mut query = String::with_capacity(chunk.len() * 512);

            for entity in chunk {
                let table = escape_table(entity.kind.table_name());
                let id = sanitize_id(&entity.qualified_name);
                let set_clause = build_entity_set(entity);
                query.push_str(&format!("UPSERT {}:{} {};\n", table, id, set_clause));
            }

            match self.db.query(&query).await {
                Ok(_response) => {
                    // Batch query succeeded — all UPSERT statements executed.
                    // Note: response.take() can fail with serde deserialization errors
                    // on SurrealDB native types (record IDs), even when the UPSERT
                    // itself succeeded. Count based on batch success instead.
                    total += chunk.len();
                }
                Err(e) => {
                    debug!("Batch upsert failed ({}), falling back to individual", e);
                    for entity in chunk {
                        let table = escape_table(entity.kind.table_name());
                        let id = sanitize_id(&entity.qualified_name);
                        let set_clause = build_entity_set(entity);
                        let q = format!("UPSERT {}:{} {};", table, id, set_clause);
                        match self.db.query(&q).await {
                            Ok(_) => total += 1,
                            Err(e2) => {
                                warn!("Entity upsert failed {}: {}", entity.qualified_name, e2);
                            }
                        }
                    }
                }
            }
        }

        Ok(total)
    }

    /// Insert relations in batches using multi-statement RELATE.
    ///
    /// Groups up to BATCH_SIZE relations per DB round-trip.
    /// Falls back to individual inserts on batch failure.
    pub async fn insert_relations(&self, relations: &[CodeRelation]) -> Result<usize> {
        if relations.is_empty() {
            return Ok(0);
        }

        let mut total = 0;

        for chunk in relations.chunks(BATCH_SIZE) {
            let mut query = String::with_capacity(chunk.len() * 256);

            for rel in chunk {
                let from_table = escape_table(&rel.from_table);
                let from_id = sanitize_id(&rel.from_entity);
                let edge = escape_table(rel.kind.table_name());
                let to_table = escape_table(&rel.to_table);
                let to_id = sanitize_id(&rel.to_entity);

                let meta_set = rel
                    .metadata
                    .as_ref()
                    .map(build_meta_set)
                    .unwrap_or_default();

                query.push_str(&format!(
                    "RELATE {}:{}->{}->{}:{}{};\n",
                    from_table, from_id, edge, to_table, to_id, meta_set
                ));
            }

            match self.db.query(&query).await {
                Ok(_response) => {
                    // Batch RELATE succeeded — count all relations in chunk.
                    total += chunk.len();
                }
                Err(e) => {
                    debug!("Batch relate failed ({}), falling back to individual", e);
                    for rel in chunk {
                        let from_table = escape_table(&rel.from_table);
                        let from_id = sanitize_id(&rel.from_entity);
                        let edge = escape_table(rel.kind.table_name());
                        let to_table = escape_table(&rel.to_table);
                        let to_id = sanitize_id(&rel.to_entity);
                        let meta_set = rel
                            .metadata
                            .as_ref()
                            .map(build_meta_set)
                            .unwrap_or_default();
                        let q = format!(
                            "RELATE {}:{}->{}->{}:{}{};",
                            from_table, from_id, edge, to_table, to_id, meta_set
                        );
                        match self.db.query(&q).await {
                            Ok(_) => total += 1,
                            Err(e2) => {
                                warn!(
                                    "Relation failed {} -[{}]-> {}: {}",
                                    rel.from_entity,
                                    rel.kind.table_name(),
                                    rel.to_entity,
                                    e2
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(total)
    }

    /// Delete all entities for a specific file (single multi-statement query).
    pub async fn delete_file_entities(&self, file_path: &str, repo: &str) -> Result<()> {
        self.db
            .query(
                "DELETE FROM `function` WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM class WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM module WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM variable WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM import_decl WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM config WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM doc WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM api WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM db_entity WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM infra WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM package WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM skill WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM http_call WHERE file_path = $path AND repo = $repo; \
                 DELETE FROM file WHERE path = $path AND repo = $repo;",
            )
            .bind(("path", file_path.to_string()))
            .bind(("repo", repo.to_string()))
            .await?;
        Ok(())
    }

    /// Clear all data for a specific repo (single multi-statement query).
    pub async fn clear_repo(&self, repo: &str) -> Result<()> {
        self.db
            .query(
                "DELETE FROM file WHERE repo = $repo; \
                 DELETE FROM `function` WHERE repo = $repo; \
                 DELETE FROM class WHERE repo = $repo; \
                 DELETE FROM module WHERE repo = $repo; \
                 DELETE FROM variable WHERE repo = $repo; \
                 DELETE FROM import_decl WHERE repo = $repo; \
                 DELETE FROM config WHERE repo = $repo; \
                 DELETE FROM doc WHERE repo = $repo; \
                 DELETE FROM api WHERE repo = $repo; \
                 DELETE FROM db_entity WHERE repo = $repo; \
                 DELETE FROM infra WHERE repo = $repo; \
                 DELETE FROM package WHERE repo = $repo; \
                 DELETE FROM http_call WHERE repo = $repo; \
                 DELETE FROM skill WHERE repo = $repo;",
            )
            .bind(("repo", repo.to_string()))
            .await?;
        Ok(())
    }

    /// Resolve cross-file call targets.
    ///
    /// After indexing, many `calls` edges point to `function:repo_file_callee` where
    /// callee was assumed to be in the same file. For cross-file calls, the target
    /// doesn't exist. This method finds orphan targets and re-links them to matching
    /// functions by name in the same repo.
    pub async fn resolve_call_targets(&self, repo: &str) -> Result<usize> {
        // Step 1: Find calls where the target function doesn't exist
        // Step 2: For each orphan, find a function with matching name in the repo
        // Step 3: Delete orphan edge, create new one pointing to correct target
        //
        // We do this in SurrealQL for efficiency:
        let query = "LET $orphans = (SELECT id, in AS caller, out AS callee, \
               out.name AS target_name, meta::id(out) AS target_id \
             FROM calls \
             WHERE out.name IS NULL AND meta::tb(out) = 'function');
             RETURN array::len($orphans);"
            .to_string();

        let mut response = self.db.query(&query).await?;
        let orphan_count: Option<i64> = response.take(1).unwrap_or(None);
        let count = orphan_count.unwrap_or(0) as usize;

        if count == 0 {
            return Ok(0);
        }

        debug!(
            "Found {} orphan call targets, attempting resolution...",
            count
        );

        // Build a name→qualified_name index for all functions in the repo
        let resolve_query = format!(
            "LET $fns = (SELECT name, id FROM `function` WHERE repo = '{}');
             LET $orphans = (SELECT id, in AS caller, meta::id(out) AS raw_target FROM calls WHERE out.name IS NULL AND meta::tb(out) = 'function');
             FOR $o IN $orphans {{
               LET $raw = $o.raw_target;
               LET $parts = string::split($raw, '_');
               LET $callee_name = array::last($parts);
               LET $matches = (SELECT id FROM `function` WHERE name = $callee_name AND repo = '{}' LIMIT 1);
               IF array::len($matches) > 0 {{
                 LET $target = $matches[0].id;
                 DELETE $o.id;
                 RELATE ($o.caller)->calls->($target) SET line = 0;
               }};
             }};
             RETURN 'done';",
            repo.replace('\'', "\\'"),
            repo.replace('\'', "\\'"),
        );

        match self.db.query(&resolve_query).await {
            Ok(_) => {
                debug!("Call target resolution completed for {} orphans", count);
                Ok(count)
            }
            Err(e) => {
                warn!("Call target resolution failed: {}", e);
                Ok(0)
            }
        }
    }

    /// Link HTTP client calls to matching API endpoint definitions.
    ///
    /// Matches http_call entities to api entities by HTTP method + path pattern.
    /// For example: http_call "GET /users/{id}" → api "GET /users/{id}".
    pub async fn link_http_endpoints(&self, repo: &str) -> Result<usize> {
        let repo_escaped = repo.replace('\'', "\\'");

        // Find all HTTP calls and API endpoints in the repo,
        // then match by method + normalized path
        let query = format!(
            "LET $calls = (SELECT id, name, kind, qualified_name FROM http_call WHERE repo = '{}');
             LET $endpoints = (SELECT id, name, qualified_name FROM api WHERE repo = '{}' AND kind = 'ApiEndpoint');
             LET $linked = 0;
             FOR $call IN $calls {{
               FOR $ep IN $endpoints {{
                 IF string::lowercase($call.name) = string::lowercase($ep.name) {{
                   RELATE ($call.id)->calls_endpoint->($ep.id) SET method = $call.kind;
                   LET $linked = $linked + 1;
                 }};
               }};
             }};
             RETURN $linked;",
            repo_escaped, repo_escaped,
        );

        match self.db.query(&query).await {
            Ok(mut response) => {
                let count: Option<i64> = response.take(4).unwrap_or(None);
                let linked = count.unwrap_or(0) as usize;
                if linked > 0 {
                    debug!("Linked {} HTTP calls to API endpoints", linked);
                }
                Ok(linked)
            }
            Err(e) => {
                warn!("HTTP endpoint linking failed: {}", e);
                Ok(0)
            }
        }
    }

    /// Resolve virtual dispatch edges for OOP languages (C#, Java).
    ///
    /// Finds methods with "override" in their signature, then links the base
    /// class method to the override via a `calls` edge with kind = 'virtual_dispatch'.
    pub async fn resolve_virtual_dispatch(&self, repo: &str) -> Result<usize> {
        // Find all override methods
        let overrides: Vec<serde_json::Value> = self
            .db
            .query(
                "SELECT name, qualified_name, file_path, signature FROM `function` \
                 WHERE signature != NONE AND (signature ~ 'override' OR signature ~ '@Override') AND repo = $repo",
            )
            .bind(("repo", repo.to_string()))
            .await?
            .take(0)?;

        let mut count = 0;
        for ov in &overrides {
            let name = match ov.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => continue,
            };
            let ov_qname = match ov.get("qualified_name").and_then(|v| v.as_str()) {
                Some(q) => q,
                None => continue,
            };

            // Find base class methods with the same name but different qualified_name
            let bases: Vec<serde_json::Value> = self
                .db
                .query(
                    "SELECT qualified_name FROM `function` WHERE name = $name AND qualified_name != $qname AND repo = $repo LIMIT 10",
                )
                .bind(("name", name.to_string()))
                .bind(("qname", ov_qname.to_string()))
                .bind(("repo", repo.to_string()))
                .await?
                .take(0)?;

            for base in &bases {
                if let Some(base_qname) = base.get("qualified_name").and_then(|v| v.as_str()) {
                    let from_id = sanitize_id(base_qname);
                    let to_id = sanitize_id(ov_qname);
                    let query = format!(
                        "RELATE `function`:`{}`->calls->`function`:`{}` SET kind = 'virtual_dispatch'",
                        from_id, to_id
                    );
                    if self.db.query(&query).await.is_ok() {
                        count += 1;
                    }
                }
            }
        }

        if count > 0 {
            tracing::info!("Resolved {} virtual dispatch edges", count);
        }
        Ok(count)
    }

    /// Get stats about the current graph
    pub async fn stats(&self) -> Result<IndexResult> {
        let mut response = self
            .db
            .query(
                "SELECT count() FROM file GROUP ALL;
                 SELECT count() FROM `function` GROUP ALL;
                 SELECT count() FROM class GROUP ALL;
                 SELECT count() FROM import_decl GROUP ALL;
                 SELECT count() FROM contains GROUP ALL;
                 SELECT count() FROM calls GROUP ALL;
                 SELECT count() FROM imports GROUP ALL;",
            )
            .await?;

        fn extract_count(result: Vec<serde_json::Value>) -> usize {
            result
                .first()
                .and_then(|v| v.get("count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize
        }

        let files: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let functions: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let classes: Vec<serde_json::Value> = response.take(2).unwrap_or_default();
        let _imports: Vec<serde_json::Value> = response.take(3).unwrap_or_default();
        let _contains: Vec<serde_json::Value> = response.take(4).unwrap_or_default();
        let _calls: Vec<serde_json::Value> = response.take(5).unwrap_or_default();
        let _import_rels: Vec<serde_json::Value> = response.take(6).unwrap_or_default();

        Ok(IndexResult {
            files_processed: extract_count(files),
            entities_extracted: extract_count(functions) + extract_count(classes),
            relations_created: 0, // Summed from edge tables but not critical
            errors: vec![],
        })
    }
}

/// Build SET clause for entity UPSERT (schema-aware per table).
/// Uses explicit SET field = value syntax to avoid JSON parsing issues.
fn build_entity_set(entity: &CodeEntity) -> String {
    match &entity.kind {
        EntityKind::File => format!(
            "SET path = {}, language = {}, hash = {}, repo = {}, line_count = {}",
            surql_str(&entity.file_path),
            surql_str(&entity.language),
            surql_opt_str(&entity.body_hash),
            surql_str(&entity.repo),
            entity.end_line,
        ),
        EntityKind::Function | EntityKind::Method => format!(
            "SET name = {}, qualified_name = {}, signature = {}, body_hash = {}, \
             file_path = {}, repo = {}, language = {}, start_line = {}, end_line = {}",
            surql_str(&entity.name),
            surql_str(&entity.qualified_name),
            surql_opt_str(&entity.signature),
            surql_opt_str(&entity.body_hash),
            surql_str(&entity.file_path),
            surql_str(&entity.repo),
            surql_str(&entity.language),
            entity.start_line,
            entity.end_line,
        ),
        EntityKind::Class
        | EntityKind::Struct
        | EntityKind::Interface
        | EntityKind::Trait
        | EntityKind::Enum
        | EntityKind::TypeAlias => format!(
            "SET name = {}, qualified_name = {}, kind = {}, \
             file_path = {}, repo = {}, language = {}, start_line = {}, end_line = {}",
            surql_str(&entity.name),
            surql_str(&entity.qualified_name),
            surql_str(&format!("{:?}", entity.kind)),
            surql_str(&entity.file_path),
            surql_str(&entity.repo),
            surql_str(&entity.language),
            entity.start_line,
            entity.end_line,
        ),
        EntityKind::Import => format!(
            "SET name = {}, qualified_name = {}, file_path = {}, repo = {}, body = {}",
            surql_str(&entity.name),
            surql_str(&entity.qualified_name),
            surql_str(&entity.file_path),
            surql_str(&entity.repo),
            surql_opt_str(&entity.body),
        ),
        EntityKind::Module => format!(
            "SET name = {}, qualified_name = {}, file_path = {}, repo = {}",
            surql_str(&entity.name),
            surql_str(&entity.qualified_name),
            surql_str(&entity.file_path),
            surql_str(&entity.repo),
        ),
        EntityKind::Variable | EntityKind::Constant => format!(
            "SET name = {}, qualified_name = {}, file_path = {}, repo = {}",
            surql_str(&entity.name),
            surql_str(&entity.qualified_name),
            surql_str(&entity.file_path),
            surql_str(&entity.repo),
        ),
        EntityKind::ConversationSession => format!(
            "SET name = {}, qualified_name = {}, kind = {}, \
             file_path = {}, repo = {}, language = {}, \
             start_line = {}, end_line = {}, body = {}, hash = {}, timestamp = {}",
            surql_str(&entity.name),
            surql_str(&entity.qualified_name),
            surql_str(&format!("{:?}", entity.kind)),
            surql_str(&entity.file_path),
            surql_str(&entity.repo),
            surql_str(&entity.language),
            entity.start_line,
            entity.end_line,
            surql_opt_str(&entity.body),
            surql_opt_str(&entity.body_hash),
            surql_opt_str(&entity.signature), // timestamp stored in signature field
        ),
        EntityKind::SkillNode | EntityKind::SkillMOC => format!(
            "SET name = {}, qualified_name = {}, kind = {}, \
             file_path = {}, repo = {}, language = {}, \
             start_line = {}, end_line = {}, body = {}, \
             description = {}, node_type = {}, created = {}",
            surql_str(&entity.name),
            surql_str(&entity.qualified_name),
            surql_str(&format!("{:?}", entity.kind)),
            surql_str(&entity.file_path),
            surql_str(&entity.repo),
            surql_str(&entity.language),
            entity.start_line,
            entity.end_line,
            surql_opt_str(&entity.body),
            surql_opt_str(&entity.body), // description = body (frontmatter description)
            surql_str(&format!("{:?}", entity.kind)), // node_type from kind
            surql_opt_str(&entity.signature), // created date in signature field
        ),
        // All other entities: config, doc, api, db_entity, infra, package, conversation
        _ => format!(
            "SET name = {}, qualified_name = {}, kind = {}, \
             file_path = {}, repo = {}, language = {}, \
             start_line = {}, end_line = {}, body = {}",
            surql_str(&entity.name),
            surql_str(&entity.qualified_name),
            surql_str(&format!("{:?}", entity.kind)),
            surql_str(&entity.file_path),
            surql_str(&entity.repo),
            surql_str(&entity.language),
            entity.start_line,
            entity.end_line,
            surql_opt_str(&entity.body),
        ),
    }
}

/// Escape a string for SurrealQL (single-quoted, with escaping).
fn surql_str(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Escape an optional string for SurrealQL.
fn surql_opt_str(s: &Option<String>) -> String {
    match s {
        Some(v) => surql_str(v),
        None => "NONE".to_string(),
    }
}

/// Build SET clause from relation metadata JSON.
fn build_meta_set(meta: &serde_json::Value) -> String {
    if let Some(obj) = meta.as_object() {
        let parts: Vec<String> = obj.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
        if parts.is_empty() {
            String::new()
        } else {
            format!(" SET {}", parts.join(", "))
        }
    } else {
        String::new()
    }
}

/// Escape table name for SurrealDB (handles reserved words like 'function').
fn escape_table(name: &str) -> String {
    format!("`{}`", name)
}

/// Sanitize a string to be a valid SurrealDB record ID.
/// Replaces all special characters with underscores, collapses doubles,
/// and trims leading/trailing underscores.
pub fn sanitize_id(s: &str) -> String {
    s.replace(
        [
            '/', '\\', ':', '.', ' ', '<', '>', '"', '\'', '(', ')', ',', ';', '{', '}', '[', ']',
            '-', '`', '~', '!', '@', '#', '$', '%', '^', '&', '*', '+', '=', '|', '?',
        ],
        "_",
    )
    .replace("__", "_")
    .replace("__", "_") // Second pass for triple underscores
    .trim_matches('_')
    .to_string()
}

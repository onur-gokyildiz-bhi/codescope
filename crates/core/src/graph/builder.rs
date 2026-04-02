use anyhow::Result;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use tracing::{debug, warn};

use crate::{CodeEntity, CodeRelation, EntityKind, IndexResult};

/// Builds the code knowledge graph in SurrealDB
pub struct GraphBuilder {
    db: Surreal<Db>,
}

impl GraphBuilder {
    pub fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// Insert a batch of entities into SurrealDB
    pub async fn insert_entities(&self, entities: &[CodeEntity]) -> Result<usize> {
        let mut count = 0;

        for entity in entities {
            let table = entity.kind.table_name().to_string();
            let id = sanitize_id(&entity.qualified_name);

            // Clone all values to owned Strings for SurrealDB's 'static requirement
            let name = entity.name.clone();
            let qname = entity.qualified_name.clone();
            let path = entity.file_path.clone();
            let repo = entity.repo.clone();
            let lang = entity.language.clone();
            let sig = entity.signature.clone();
            let hash = entity.body_hash.clone();
            let body = entity.body.clone();
            let start = entity.start_line;
            let end = entity.end_line;

            let result = match &entity.kind {
                EntityKind::File => {
                    self.db
                        .query(
                            "CREATE type::thing($table, $id) SET \
                             path = $path, language = $lang, hash = $hash, \
                             repo = $repo, line_count = $lines"
                        )
                        .bind(("table", table))
                        .bind(("id", id))
                        .bind(("path", path))
                        .bind(("lang", lang))
                        .bind(("hash", hash))
                        .bind(("repo", repo))
                        .bind(("lines", end))
                        .await
                }
                EntityKind::Function | EntityKind::Method => {
                    self.db
                        .query(
                            "CREATE type::thing($table, $id) SET \
                             name = $name, qualified_name = $qname, \
                             signature = $sig, body_hash = $hash, \
                             file_path = $path, repo = $repo, language = $lang, \
                             start_line = $start, end_line = $end"
                        )
                        .bind(("table", table))
                        .bind(("id", id))
                        .bind(("name", name))
                        .bind(("qname", qname))
                        .bind(("sig", sig))
                        .bind(("hash", hash))
                        .bind(("path", path))
                        .bind(("repo", repo))
                        .bind(("lang", lang))
                        .bind(("start", start))
                        .bind(("end", end))
                        .await
                }
                EntityKind::Class
                | EntityKind::Struct
                | EntityKind::Interface
                | EntityKind::Trait
                | EntityKind::Enum => {
                    let kind_str = format!("{:?}", entity.kind);
                    self.db
                        .query(
                            "CREATE type::thing($table, $id) SET \
                             name = $name, qualified_name = $qname, \
                             kind = $kind, file_path = $path, repo = $repo, \
                             language = $lang, start_line = $start, end_line = $end"
                        )
                        .bind(("table", table))
                        .bind(("id", id))
                        .bind(("name", name))
                        .bind(("qname", qname))
                        .bind(("kind", kind_str))
                        .bind(("path", path))
                        .bind(("repo", repo))
                        .bind(("lang", lang))
                        .bind(("start", start))
                        .bind(("end", end))
                        .await
                }
                EntityKind::Import => {
                    self.db
                        .query(
                            "CREATE type::thing($table, $id) SET \
                             name = $name, qualified_name = $qname, \
                             file_path = $path, repo = $repo, body = $body"
                        )
                        .bind(("table", table))
                        .bind(("id", id))
                        .bind(("name", name))
                        .bind(("qname", qname))
                        .bind(("path", path))
                        .bind(("repo", repo))
                        .bind(("body", body))
                        .await
                }
                _ => {
                    // Generic insert for config, doc, api, db_entity, infra, package
                    let kind_str = format!("{:?}", entity.kind);
                    self.db
                        .query(
                            "CREATE type::thing($table, $id) SET \
                             name = $name, qualified_name = $qname, \
                             kind = $kind, file_path = $path, repo = $repo, \
                             language = $lang, start_line = $start, end_line = $end, \
                             body = $body"
                        )
                        .bind(("table", table))
                        .bind(("id", id))
                        .bind(("name", name))
                        .bind(("qname", qname))
                        .bind(("kind", kind_str))
                        .bind(("path", path))
                        .bind(("repo", repo))
                        .bind(("lang", lang))
                        .bind(("start", start))
                        .bind(("end", end))
                        .bind(("body", body))
                        .await
                }
            };

            match result {
                Ok(_) => {
                    count += 1;
                    debug!("Created entity: {} ({})", entity.qualified_name, entity.kind.table_name());
                }
                Err(e) => {
                    warn!("Failed to create entity {}: {}", entity.qualified_name, e);
                }
            }
        }

        Ok(count)
    }

    /// Insert a batch of relations (edges) into SurrealDB using RELATE
    pub async fn insert_relations(&self, relations: &[CodeRelation]) -> Result<usize> {
        let mut count = 0;

        for rel in relations {
            let table = rel.kind.table_name();
            let from_id = sanitize_id(&rel.from_entity);
            let to_id = sanitize_id(&rel.to_entity);

            // Use inline SurrealQL with string interpolation for the edge table
            // RELATE needs record IDs, so we construct them inline
            let query = format!(
                "RELATE (type::thing('function', $from))->{}->(type::thing('function', $to))",
                table
            );

            let result = self.db
                .query(query)
                .bind(("from", from_id))
                .bind(("to", to_id))
                .await;

            match result {
                Ok(_) => {
                    count += 1;
                    debug!("Created relation: {} -[{}]-> {}", rel.from_entity, table, rel.to_entity);
                }
                Err(e) => {
                    warn!(
                        "Failed to create relation {} -[{}]-> {}: {}",
                        rel.from_entity, table, rel.to_entity, e
                    );
                }
            }
        }

        Ok(count)
    }

    /// Clear all data for a specific repo
    pub async fn clear_repo(&self, repo: &str) -> Result<()> {
        let tables = [
            "file", "`function`", "class", "module", "variable", "import_decl",
        ];
        for table in &tables {
            let repo_owned = repo.to_string();
            self.db
                .query(format!("DELETE FROM {} WHERE repo = $repo", table))
                .bind(("repo", repo_owned))
                .await?;
        }
        Ok(())
    }

    /// Get stats about the current graph
    pub async fn stats(&self) -> Result<IndexResult> {
        let _response = self.db
            .query(
                "SELECT count() FROM file GROUP ALL;
                 SELECT count() FROM function GROUP ALL;
                 SELECT count() FROM class GROUP ALL;
                 SELECT count() FROM import_decl GROUP ALL;
                 SELECT count() FROM contains GROUP ALL;
                 SELECT count() FROM calls GROUP ALL;
                 SELECT count() FROM imports GROUP ALL;"
            )
            .await?;

        Ok(IndexResult::default())
    }
}

/// Sanitize a string to be a valid SurrealDB record ID
fn sanitize_id(s: &str) -> String {
    s.replace(['/', '\\', ':', '.', ' ', '<', '>', '"', '\'', '(', ')', ',', ';', '{', '}', '[', ']'], "_")
        .replace("__", "_")
        .trim_matches('_')
        .to_string()
}

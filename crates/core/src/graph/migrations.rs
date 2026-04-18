//! Schema migrations for the knowledge graph DB.
//!
//! Each migration bumps `schema_version` by 1 and performs any SurrealQL
//! operations required to bring an older DB forward. Migrations MUST be
//! idempotent — running them twice against an already-migrated DB is a no-op.
//!
//! The `init_schema` function uses `DEFINE ... IF NOT EXISTS` everywhere,
//! so structural additions (new tables, new fields) generally do not need
//! a dedicated migration — they are picked up automatically on next connect.
//! Migrations are reserved for data transforms, renames, or cleanups that
//! cannot be expressed with IF NOT EXISTS guards.
//!
//! See `crates/core/src/graph/schema.rs` for the current `SCHEMA_VERSION`.
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use crate::DbHandle;

use super::schema::{get_schema_version, set_schema_version, SCHEMA_VERSION};

/// A single forward migration step.
pub struct Migration {
    pub from_version: u32,
    pub to_version: u32,
    pub description: &'static str,
    pub run: for<'a> fn(&'a DbHandle) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>,
}

/// Registered migrations, in ascending order. To add a new one, append an
/// entry with `from_version = SCHEMA_VERSION - 1`, `to_version = SCHEMA_VERSION`,
/// and implement the required data transforms inside `run`.
pub fn migrations() -> Vec<Migration> {
    vec![
        // v0 → v1: introduce schema_version tracking itself. No data changes;
        // the meta:schema row is written by `set_schema_version` after `run`.
        Migration {
            from_version: 0,
            to_version: 1,
            description: "Initial schema_version tracking",
            run: |_db| Box::pin(async move { Ok(()) }),
        },
        Migration {
            from_version: 1,
            to_version: 2,
            description: "Record additive tables from v0.7.x (knowledge, conversation, decision, ...) in schema_version",
            run: |_db| Box::pin(async move {
                // All tables added with IF NOT EXISTS; no data migration needed.
                // This migration exists purely to update meta:schema.version
                // so downstream tools can detect the capability.
                Ok(())
            }),
        },
        // v2 → v3: add ORDER BY / sort indexes for hot-path timestamp + language
        // queries (Linnaeus audit). `DEFINE INDEX IF NOT EXISTS` is safe on
        // existing DBs, so the migration just replays the DEFINE statements.
        Migration {
            from_version: 2,
            to_version: 3,
            description: "Add sort/ORDER BY indexes (knowledge.updated_at, *.timestamp, file.language)",
            run: |db| {
                Box::pin(async move {
                    db.query(
                        "
                        DEFINE INDEX IF NOT EXISTS know_updated_at ON knowledge FIELDS updated_at;
                        DEFINE INDEX IF NOT EXISTS decision_timestamp ON decision FIELDS timestamp;
                        DEFINE INDEX IF NOT EXISTS problem_timestamp ON problem FIELDS timestamp;
                        DEFINE INDEX IF NOT EXISTS solution_timestamp ON solution FIELDS timestamp;
                        DEFINE INDEX IF NOT EXISTS conv_topic_timestamp ON conv_topic FIELDS timestamp;
                        DEFINE INDEX IF NOT EXISTS conversation_timestamp ON conversation FIELDS timestamp;
                        DEFINE INDEX IF NOT EXISTS file_language ON file FIELDS language;
                        ",
                    )
                    .await?;
                    Ok(())
                })
            },
        },
    ]
}

/// Walk the DB from whatever version it's at up to `SCHEMA_VERSION`, applying
/// each registered migration in order. Returns the final version.
///
/// Safe to call on every connect — idempotent once the DB is at current.
pub async fn migrate_to_current(db: &DbHandle) -> Result<u32> {
    let mut current = get_schema_version(db).await.unwrap_or(0);
    let target = SCHEMA_VERSION;

    if current == target {
        return Ok(current);
    }

    for m in migrations() {
        if m.from_version == current && m.to_version <= target {
            tracing::info!(
                "Applying migration {} -> {}: {}",
                m.from_version,
                m.to_version,
                m.description
            );
            (m.run)(db).await?;
            set_schema_version(db, m.to_version).await?;
            current = m.to_version;
        }
    }

    Ok(current)
}

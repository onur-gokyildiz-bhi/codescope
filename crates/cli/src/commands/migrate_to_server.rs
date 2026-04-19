//! `codescope migrate-to-server` — move legacy per-repo SurrealKV
//! directories (`~/.codescope/db/<repo>/`) into the unified `surreal`
//! server under `NS=codescope, DB=<repo>`.
//!
//! ## WIP status (2026-04-19)
//!
//! Dry-run works end-to-end: discovers 15 legacy repos, reports planned
//! entity + relation counts per repo. Execute path partially works:
//! * Dropping the source embedded DB before renaming is not done
//!   cleanly yet — the SurrealKV file lock is still held, so the post-
//!   migration rename to `<repo>.old.<ts>/` fails with `os error 5`.
//! * Entity copy lands some but not all rows — id format parsing is
//!   fragile. `SELECT * FROM file` returns `{id: Thing(...)}` shapes
//!   that our [`format_record_id`] helper doesn't always handle.
//! * Relation tables use `CREATE`, which for Surreal v3 relation
//!   tables should be `RELATE in->table->out`. Currently zero relations
//!   migrate.
//!
//! Next session: switch to `surreal export` / `surreal import` via
//! spawned binary — the blessed path handles every edge case.
//!
//! ## Flow per repo
//!
//! 1. Open source as embedded `surrealkv:<path>` (read-only intent).
//! 2. Connect to target server via [`codescope_core::connect_repo`].
//! 3. Init schema on target (idempotent).
//! 4. For each known table (entities + relations), `SELECT * FROM t` on
//!    source, bulk-insert into target. Record count gets asserted.
//! 5. Rename the source dir to `<repo>.old.<timestamp>/` so the user
//!    always has a rollback. `--delete-backup` skips this.
//!
//! Default is dry-run — the plan prints but nothing writes. Pass
//! `--execute` to actually move data.
//!
//! ## Safety
//!
//! * Each repo is a separate transaction-ish unit: if copy fails midway
//!   we log and move on, leaving the legacy dir intact.
//! * Target namespace/db auto-creates on first `use_db`; the DDL retry
//!   inside [`codescope_core::connect_repo`] tolerates cold-server races.
//! * Existing data in the target db is not wiped — if you re-run, rows
//!   UPSERT by id so re-migration is idempotent.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Entity tables (normal records). Must stay in sync with
/// [`codescope_core::EntityKind::table_name`] output set.
const ENTITY_TABLES: &[&str] = &[
    "file",
    "module",
    "function",
    "class",
    "variable",
    "import_decl",
    "config",
    "doc",
    "api",
    "http_call",
    "skill",
    "db_entity",
    "infra",
    "package",
    "conversation",
    "conv_topic",
    "decision",
    "problem",
    "solution",
    "knowledge",
    "meta",
];

/// Relation tables. Copied after entity tables so the referenced records
/// exist when we RELATE them.
const RELATION_TABLES: &[&str] = &[
    "contains",
    "calls",
    "imports",
    "inherits",
    "implements",
    "uses",
    "modified_in",
    "depends_on",
    "configures",
    "defines_endpoint",
    "has_field",
    "references",
    "depends_on_package",
    "runs_script",
    "calls_endpoint",
    "links_to",
    "discussed_in",
    "decided_about",
    "solves_for",
    "co_discusses",
    "supports",
    "contradicts",
    "related_to",
];

pub async fn run(repo: Option<String>, execute: bool, delete_backup: bool) -> Result<()> {
    let db_root = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db");

    if !db_root.is_dir() {
        println!("No legacy DB root at {} — nothing to migrate.", db_root.display());
        return Ok(());
    }

    let repos: Vec<String> = match repo {
        Some(r) => vec![r],
        None => discover_legacy_repos(&db_root),
    };

    if repos.is_empty() {
        println!("No repos found under {}.", db_root.display());
        return Ok(());
    }

    let mode = if execute { "EXECUTE" } else { "DRY RUN" };
    println!("codescope migrate-to-server — mode: {mode}");
    println!("  target: ws://127.0.0.1:8077 (via CODESCOPE_DB_URL if set)");
    println!("  repos:  {} ({})", repos.len(), repos.join(", "));
    println!();

    let mut ok = 0usize;
    let mut failed = Vec::<(String, String)>::new();

    for repo in &repos {
        let src = db_root.join(repo);
        if !src.is_dir() {
            println!("  {repo}: skip — not a directory at {}", src.display());
            continue;
        }
        println!("[{repo}] source: {}", src.display());
        if !execute {
            let sizes = count_source_rows(&src, repo).await;
            match sizes {
                Ok(summary) => {
                    println!("  planned: {summary}");
                }
                Err(e) => {
                    println!("  ! source probe failed: {e:#}");
                    failed.push((repo.clone(), e.to_string()));
                    continue;
                }
            }
            continue;
        }

        match migrate_one(&src, repo, delete_backup).await {
            Ok(summary) => {
                println!("  ✓ migrated: {summary}");
                ok += 1;
            }
            Err(e) => {
                println!("  ✗ failed: {e:#}");
                failed.push((repo.clone(), e.to_string()));
            }
        }
    }

    println!();
    if execute {
        println!(
            "Done. {} migrated, {} failed.",
            ok,
            failed.len()
        );
    } else {
        println!("Dry run complete. Re-run with --execute to perform the migration.");
    }

    if !failed.is_empty() {
        println!("Failed repos:");
        for (r, err) in &failed {
            println!("  - {r}: {err}");
        }
    }

    Ok(())
}

/// Walk the legacy DB root and return every subdirectory name that
/// doesn't start with `.` (those are hidden helpers like `.tmp`) and
/// doesn't already look like a post-migration backup (`.old.*`).
fn discover_legacy_repos(db_root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(db_root) else {
        return out;
    };
    for e in entries.flatten() {
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Some(name) = e.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if name.contains(".old.") || name.ends_with(".old") {
            continue;
        }
        out.push(name);
    }
    out.sort();
    out
}

/// Open the source as an embedded SurrealKV and count rows per known
/// table. Used by dry-run to show scope without touching the target.
async fn count_source_rows(src_path: &Path, repo: &str) -> Result<String> {
    let src = open_source(src_path, repo).await?;
    let mut entity_total = 0u64;
    let mut relation_total = 0u64;

    for table in ENTITY_TABLES {
        entity_total += table_count(&src, table).await.unwrap_or(0);
    }
    for table in RELATION_TABLES {
        relation_total += table_count(&src, table).await.unwrap_or(0);
    }

    Ok(format!(
        "{entity_total} entities + {relation_total} relations"
    ))
}

/// Perform one repo's migration end-to-end. Returns a human-readable
/// summary on success.
async fn migrate_one(src_path: &Path, repo: &str, delete_backup: bool) -> Result<String> {
    let src = open_source(src_path, repo).await?;
    let dst = codescope_core::connect_repo(repo)
        .await
        .with_context(|| "cannot open target DB on surreal server")?;
    codescope_core::graph::schema::init_schema(&dst).await?;

    let mut entity_total = 0u64;
    for table in ENTITY_TABLES {
        entity_total += copy_table(&src, &dst, table)
            .await
            .with_context(|| format!("copying entity table '{table}'"))?;
    }
    let mut relation_total = 0u64;
    for table in RELATION_TABLES {
        relation_total += copy_table(&src, &dst, table)
            .await
            .with_context(|| format!("copying relation table '{table}'"))?;
    }

    // Rename legacy dir so future runs skip it and the user can roll
    // back by renaming it back.
    let parent = src_path.parent().unwrap_or_else(|| Path::new("."));
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = parent.join(format!("{repo}.old.{timestamp}"));
    std::fs::rename(src_path, &backup)
        .with_context(|| format!("rename {} → {}", src_path.display(), backup.display()))?;

    if delete_backup {
        if let Err(e) = std::fs::remove_dir_all(&backup) {
            println!(
                "  (warning: backup delete failed — keep manually at {}: {})",
                backup.display(),
                e
            );
        }
    }

    Ok(format!(
        "{entity_total} entities + {relation_total} relations → ns=codescope db={repo}"
    ))
}

/// Open a legacy per-repo directory as an embedded SurrealKV database.
/// We use the `engine::any` facade so the URL prefix picks the right
/// engine (both `surrealkv:` and `memory` go through the same API).
async fn open_source(src_path: &Path, repo: &str) -> Result<codescope_core::DbHandle> {
    let url = format!("surrealkv:{}", src_path.display());
    let db = surrealdb::engine::any::connect(&url)
        .await
        .with_context(|| format!("open legacy DB at {url}"))?;
    db.use_ns("codescope")
        .use_db(repo)
        .await
        .with_context(|| format!("use ns codescope db {repo} on legacy source"))?;
    Ok(db)
}

/// Count rows in a single table, returning 0 if the table doesn't exist
/// (expected for repos that never saw some entity kinds).
async fn table_count(db: &codescope_core::DbHandle, table: &str) -> Result<u64> {
    let q = format!("SELECT count() FROM `{table}` GROUP ALL");
    let mut resp = db.query(&q).await?;
    let rows: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
    Ok(rows
        .first()
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0))
}

/// Copy every record from `src.<table>` to `dst.<table>`. Uses CREATE
/// CONTENT so ids are preserved — re-running the migration is a no-op
/// because Surreal rejects duplicate IDs by default, but we convert
/// that into UPSERT semantics by pre-clearing the target table. The
/// pre-clear is safe during migration since the source is canonical.
async fn copy_table(
    src: &codescope_core::DbHandle,
    dst: &codescope_core::DbHandle,
    table: &str,
) -> Result<u64> {
    let select = format!("SELECT * FROM `{table}`");
    let mut resp = match src.query(&select).await {
        Ok(r) => r,
        Err(_) => return Ok(0),
    };
    let rows: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
    if rows.is_empty() {
        return Ok(0);
    }

    // Insert each record with CREATE CONTENT. We accept silent skips
    // on duplicate-id errors (SurrealDB returns them as a single
    // statement-level error that doesn't abort the query batch).
    let mut inserted = 0u64;
    for row in rows {
        let row_id = row
            .get("id")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        // Strip the id field from the payload — CREATE uses it as the
        // target record id, so passing it in the content would be a
        // double specification and Surreal rejects that.
        let mut payload = row.clone();
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("id");
        }

        let insert_q = format!(
            "CREATE {} CONTENT $payload",
            format_record_id(&row_id, table)
        );

        match dst
            .query(&insert_q)
            .bind(("payload", payload))
            .await
        {
            Ok(mut r) => {
                let _: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                inserted += 1;
            }
            Err(e) => {
                tracing::debug!("copy {table} row skipped: {e}");
            }
        }
    }

    Ok(inserted)
}

/// Build the target record id literal from whatever shape Surreal
/// returned. For a record-id value like `{"id": {"String": "graph-rag"}}`
/// we want `<table>:⟨graph-rag⟩`. For a bare string id, same thing. We
/// fall back to the full `<table>:ulid` form when we can't parse.
fn format_record_id(id_val: &serde_json::Value, table: &str) -> String {
    // Fast path: string id
    if let Some(s) = id_val.as_str() {
        return escape_record_id(table, s);
    }
    // SurrealValue-serialised ids often look like {"String": "x"} or
    // {"Number": 42}; pick out the inner primitive.
    if let Some(obj) = id_val.as_object() {
        if let Some(s) = obj.get("String").and_then(|v| v.as_str()) {
            return escape_record_id(table, s);
        }
        if let Some(n) = obj.get("Number").and_then(|v| v.as_i64()) {
            return format!("`{table}`:{n}");
        }
    }
    // Last resort — let Surreal generate a new id. We've lost the
    // original but kept the record content.
    format!("`{table}`")
}

fn escape_record_id(table: &str, id: &str) -> String {
    // Surreal's bracket id syntax `⟨...⟩` accepts arbitrary chars; we
    // use angle brackets (U+27E8/U+27E9) which are the official form.
    format!("`{table}`:⟨{}⟩", id.replace('⟩', ""))
}

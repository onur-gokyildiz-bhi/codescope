//! Bulk ingest Claude Code conversation transcripts (JSONL) into the
//! knowledge graph.
//!
//! Walks a directory tree (default `~/.claude/projects`) for `.jsonl`
//! files, parses each via `codescope_core::conversation::parse_conversation`,
//! and bulk-inserts the extracted entities + relations (decisions,
//! problems, solutions, session metadata, topic links) into either the
//! cross-project **global** knowledge DB (`~/.codescope/db/_global/`) or
//! a specific project's DB.
//!
//! Incremental mode (default): skips files whose hash already exists in
//! the target DB's `conversation` table so re-running is cheap.

use anyhow::{Context, Result};
use codescope_core::graph::builder::GraphBuilder;
use codescope_core::{CodeEntity, CodeRelation};
use std::path::{Path, PathBuf};
use std::time::Instant;
use surrealdb::engine::local::Db;
use surrealdb::engine::local::SurrealKv;
use surrealdb::types::SurrealValue;
use surrealdb::Surreal;

pub async fn run(
    dir: Option<PathBuf>,
    scope: String,
    repo: Option<String>,
    full: bool,
) -> Result<()> {
    let start = Instant::now();

    // Resolve scan directory: default to ~/.claude/projects
    let scan_dir = dir.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join("projects")
    });

    if !scan_dir.is_dir() {
        anyhow::bail!(
            "Scan directory does not exist or is not a directory: {}",
            scan_dir.display()
        );
    }

    // Resolve target DB + repo name
    let (db, target_repo, target_label): (Surreal<Db>, String, String) = match scope.as_str() {
        "global" | "g" => {
            let db = codescope_mcp::helpers::connect_global_db()
                .await
                .context("failed to connect to global DB")?;
            let r = codescope_mcp::helpers::GLOBAL_REPO.to_string();
            let lbl = format!("global ({})", r);
            (db, r, lbl)
        }
        "project" | "p" => {
            let r = repo.ok_or_else(|| {
                anyhow::anyhow!("--repo required when scope=project (project DB target)")
            })?;
            let path = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".codescope")
                .join("db")
                .join(&r);
            std::fs::create_dir_all(&path)?;
            let db = Surreal::new::<SurrealKv>(path.to_string_lossy().as_ref())
                .await
                .with_context(|| format!("failed to open project DB at {}", path.display()))?;
            db.use_ns("codescope").use_db(&r).await?;
            codescope_core::graph::schema::init_schema(&db).await?;
            let lbl = format!("project ({})", r);
            (db, r.clone(), lbl)
        }
        other => anyhow::bail!(
            "unknown --scope '{}'; expected 'global' or 'project'",
            other
        ),
    };

    let builder = GraphBuilder::new(db.clone());

    // Walk for *.jsonl
    println!("Scanning {} for .jsonl files...", scan_dir.display());
    let mut jsonl_files: Vec<PathBuf> = Vec::new();
    collect_jsonl_files(&scan_dir, &mut jsonl_files);
    println!("  Found {} files", jsonl_files.len());

    if jsonl_files.is_empty() {
        println!("Nothing to ingest.");
        return Ok(());
    }

    // Incremental dedup: skip files whose filename already has a stored hash.
    // Parsing is cheap compared to the deterministic hash check, so we skip
    // before parse.
    println!(
        "Target: {}  |  mode: {}",
        target_label,
        if full {
            "full (re-parse all)"
        } else {
            "incremental"
        }
    );

    let known_entities: Vec<String> = Vec::new();
    let mut all_entities: Vec<CodeEntity> = Vec::new();
    let mut all_relations: Vec<CodeRelation> = Vec::new();
    let mut parsed = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<(String, String)> = Vec::new();

    for (idx, path) in jsonl_files.iter().enumerate() {
        let fname = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unnamed>")
            .to_string();

        if !full {
            match check_conversation_hash(&db, &fname).await {
                Ok(Some(_)) => {
                    skipped += 1;
                    continue;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::debug!("hash check failed for {}: {} (treating as new)", fname, e);
                }
            }
        }

        match codescope_core::conversation::parse_conversation(path, &target_repo, &known_entities)
        {
            Ok((entities, relations, _res)) => {
                all_entities.extend(entities);
                all_relations.extend(relations);
                parsed += 1;
            }
            Err(e) => {
                errors.push((fname.clone(), e.to_string()));
            }
        }

        if (idx + 1) % 100 == 0 {
            println!(
                "  ... {}/{} processed ({} parsed, {} skipped, {} errors)",
                idx + 1,
                jsonl_files.len(),
                parsed,
                skipped,
                errors.len()
            );
        }
    }

    let parse_elapsed = start.elapsed();
    println!(
        "  Parsed {} files in {:.1}s (skipped {}, errors {})",
        parsed,
        parse_elapsed.as_secs_f64(),
        skipped,
        errors.len()
    );

    if all_entities.is_empty() && all_relations.is_empty() {
        println!();
        println!("No new data to insert — all files already indexed.");
        return Ok(());
    }

    let insert_start = Instant::now();
    println!(
        "Bulk insert: {} entities + {} relations...",
        all_entities.len(),
        all_relations.len()
    );
    builder.insert_entities(&all_entities).await?;
    builder.insert_relations(&all_relations).await?;
    let insert_elapsed = insert_start.elapsed();

    let total = start.elapsed();
    println!();
    println!("Ingest complete! ({:.1}s total)", total.as_secs_f64());
    println!("  Target:            {}", target_label);
    println!("  Files processed:   {}", parsed);
    println!("  Files skipped:     {} (already indexed)", skipped);
    println!("  Entities added:    {}", all_entities.len());
    println!("  Relations added:   {}", all_relations.len());
    println!("  Parse time:        {:.1}s", parse_elapsed.as_secs_f64());
    println!("  Insert time:       {:.1}s", insert_elapsed.as_secs_f64());
    if !errors.is_empty() {
        println!("  Errors:            {}", errors.len());
        for (name, msg) in errors.iter().take(10) {
            println!("    - {}: {}", name, msg);
        }
        if errors.len() > 10 {
            println!("    ... and {} more", errors.len() - 10);
        }
    }

    Ok(())
}

/// Recursively collect every `.jsonl` file under `dir`. Ignores symlinks and
/// I/O errors on individual entries (best-effort traversal).
fn collect_jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

/// Standalone variant of `codescope_mcp::helpers::check_conversation_hash` —
/// inlined to avoid widening that helper's `pub(crate)` visibility just for
/// this CLI command.
async fn check_conversation_hash(db: &Surreal<Db>, file_name: &str) -> Result<Option<String>> {
    #[derive(serde::Deserialize, SurrealValue)]
    struct HashRecord {
        hash: Option<String>,
    }
    let results: Vec<HashRecord> = db
        .query("SELECT hash FROM conversation WHERE file_path = $name LIMIT 1")
        .bind(("name", file_name.to_string()))
        .await?
        .take(0)?;
    Ok(results.into_iter().next().and_then(|r| r.hash))
}

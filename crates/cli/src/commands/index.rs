use anyhow::Result;
use codescope_core::graph::builder::GraphBuilder;
use codescope_core::graph::query::GraphQuery;
use codescope_core::parser::CodeParser;
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(
    path: PathBuf,
    repo_name: &str,
    clean: bool,
    db_path: Option<PathBuf>,
) -> Result<()> {
    use codescope_core::graph::IncrementalIndexer;
    use std::collections::HashMap;
    use std::time::Instant;

    let start_time = Instant::now();
    let db = connect_db(db_path, repo_name).await?;
    let builder = GraphBuilder::new(db.clone());
    let incremental = IncrementalIndexer::new(db.clone());

    if clean {
        println!(
            "Full re-index: clearing existing data for '{}'...",
            repo_name
        );
        builder.clear_repo(repo_name).await?;
    } else {
        println!(
            "Incremental index of {} as '{}'...",
            path.display(),
            repo_name
        );
        let deleted = incremental.cleanup_deleted_files(&path, repo_name).await?;
        if deleted > 0 {
            println!("  Cleaned up {} deleted files", deleted);
        }
    }

    // Phase 1: Collect all supported files
    let collect_start = Instant::now();
    let tmp_parser = CodeParser::new();
    let walker = ignore::WalkBuilder::new(&path)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .build();

    let all_files: Vec<PathBuf> = walker
        .flatten()
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| {
            let fp = e.path();
            let ext = fp.extension().and_then(|e| e.to_str()).unwrap_or("");
            let fname = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            (tmp_parser.supports_extension(ext) || tmp_parser.supports_filename(fname))
                && !codescope_core::parser::should_skip_file(fp)
        })
        .map(|e| e.into_path())
        .collect();
    drop(tmp_parser);

    println!(
        "  Found {} supported files ({:.1}s)",
        all_files.len(),
        collect_start.elapsed().as_secs_f64()
    );

    // Phase 2: Pre-load known hashes for incremental comparison (single DB query)
    let known_hashes: HashMap<String, String> = if !clean {
        incremental
            .load_file_hashes(repo_name)
            .await
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    // Phase 3: Parse files in parallel using rayon (CPU-bound work)
    let parse_start = Instant::now();
    let base_path = path.clone();
    let repo_owned = repo_name.to_string();
    let is_clean = clean;

    let parse_results = tokio::task::spawn_blocking(move || {
        use codescope_core::graph::incremental::hash_content;
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let parser = CodeParser::new();
        let skipped = AtomicUsize::new(0);

        let results: Vec<(
            String,
            Result<
                (
                    Vec<codescope_core::CodeEntity>,
                    Vec<codescope_core::CodeRelation>,
                ),
                String,
            >,
        )> = all_files
            .par_iter()
            .filter_map(|file_path| {
                let rel_path = file_path
                    .strip_prefix(&base_path)
                    .unwrap_or(file_path)
                    .to_string_lossy()
                    .to_string()
                    .replace('\\', "/");

                // Read file content once (used for both hashing and parsing)
                let content = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(e) => return Some((rel_path, Err(e.to_string()))),
                };

                // Incremental: skip unchanged files by comparing content hash
                if !is_clean {
                    let hash = hash_content(&content);
                    if known_hashes.get(&rel_path) == Some(&hash) {
                        skipped.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                }

                // Parse with relative path (ensures consistent paths in DB)
                match parser.parse_source(std::path::Path::new(&rel_path), &content, &repo_owned) {
                    Ok(result) => Some((rel_path, Ok(result))),
                    Err(e) => Some((rel_path, Err(e.to_string()))),
                }
            })
            .collect();

        (results, skipped.load(Ordering::Relaxed))
    })
    .await?;

    let (results, files_skipped) = parse_results;
    let parse_duration = parse_start.elapsed();
    println!(
        "  Parsed {} files, {} unchanged ({:.1}s)",
        results.len(),
        files_skipped,
        parse_duration.as_secs_f64()
    );

    // Phase 4: Batch insert results into DB.
    //
    // Collect all entities and relations from all files into flat Vecs,
    // then bulk-insert in one pass (chunked internally by GraphBuilder).
    // Previously we called insert_entities/insert_relations per-file, which
    // turned each file into 3 DB roundtrips (delete + entities + relations).
    // For 237 files that was 711 roundtrips dominating total time (~27s).
    // Corpus-wide bulk drops to ~(total_entities + total_relations) / 50.
    let insert_start = Instant::now();
    let mut all_entities: Vec<codescope_core::CodeEntity> = Vec::new();
    let mut all_relations: Vec<codescope_core::CodeRelation> = Vec::new();
    let mut files_processed = 0usize;
    let mut errors = Vec::new();

    for (rel_path, result) in results {
        match result {
            Ok((entities, relations)) => {
                // Incremental: delete old entities for changed files.
                // Keep this per-file — deletes scale with CHANGED files, not
                // total, so it's not usually a bottleneck.
                if !clean {
                    if let Err(e) = builder.delete_file_entities(&rel_path, repo_name).await {
                        tracing::warn!("Delete entities failed: {e}");
                    }
                }

                all_entities.extend(entities);
                all_relations.extend(relations);
                files_processed += 1;

                if files_processed.is_multiple_of(100) {
                    println!("  ... {} files parsed", files_processed);
                }
            }
            Err(e) => {
                errors.push(format!("{}: {}", rel_path, e));
            }
        }
    }

    let total_entities = all_entities.len();
    let total_relations = all_relations.len();

    if total_entities > 0 || total_relations > 0 {
        println!(
            "  Bulk insert: {} entities + {} relations...",
            total_entities, total_relations
        );
        builder.insert_entities(&all_entities).await?;
        builder.insert_relations(&all_relations).await?;
    }

    let total_time = start_time.elapsed();
    println!();
    println!(
        "Indexing complete! ({:.1}s total)",
        total_time.as_secs_f64()
    );
    println!("  Files indexed:      {}", files_processed);
    if files_skipped > 0 {
        println!("  Files unchanged:    {}", files_skipped);
    }
    println!("  Entities extracted: {}", total_entities);
    println!("  Relations created:  {}", total_relations);
    println!("  Parse time:         {:.1}s", parse_duration.as_secs_f64());
    println!(
        "  Insert time:        {:.1}s",
        insert_start.elapsed().as_secs_f64()
    );

    if !errors.is_empty() {
        println!("  Errors:             {}", errors.len());
        for err in errors.iter().take(10) {
            println!("    - {}", err);
        }
        if errors.len() > 10 {
            println!("    ... and {} more", errors.len() - 10);
        }
    }

    // Check calls created
    let gq = GraphQuery::new(db.clone());
    if let Ok(result) = gq
        .raw_query("SELECT count() AS cnt FROM calls GROUP ALL")
        .await
    {
        if let Some(cnt) = result
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.get("cnt"))
            .and_then(|v| v.as_u64())
        {
            println!("  Calls edges:        {}", cnt);
        }
    }

    // Phase 5: Resolve cross-file call targets
    match builder.resolve_call_targets(repo_name).await {
        Ok(resolved) if resolved > 0 => {
            println!("  Call targets resolved: {}", resolved);
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Call target resolution failed: {}", e),
    }

    // Phase 5b: Resolve virtual dispatch for OOP languages
    match builder.resolve_virtual_dispatch(repo_name).await {
        Ok(resolved) if resolved > 0 => {
            println!("  Virtual dispatch resolved: {}", resolved);
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Virtual dispatch resolution failed: {}", e),
    }

    // Phase 6: Index conversations from Claude session logs
    {
        let project_dir = codescope_mcp::helpers::find_claude_project_dir(&path, repo_name);
        let mut jsonl_files = Vec::new();
        codescope_mcp::collect_jsonl_files(&project_dir, &mut jsonl_files);

        if !jsonl_files.is_empty() {
            let known_entities: Vec<String> = Vec::new();
            let mut conv_count = 0;
            for jsonl_path in &jsonl_files {
                match codescope_core::conversation::parse_conversation(
                    jsonl_path,
                    repo_name,
                    &known_entities,
                ) {
                    Ok((entities, relations, _)) => {
                        if let Err(e) = builder.insert_entities(&entities).await {
                            tracing::warn!("Conv entity insert failed: {e}");
                        }
                        if let Err(e) = builder.insert_relations(&relations).await {
                            tracing::warn!("Conv relation insert failed: {e}");
                        }
                        conv_count += 1;
                    }
                    Err(e) => {
                        tracing::debug!("Conversation parse error: {}", e);
                    }
                }
            }

            // Index memory files
            let memory_dir = project_dir.join("memory");
            let mut mem_count = 0;
            if memory_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&memory_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.extension().map(|e| e == "md").unwrap_or(false) {
                            if let Ok((ents, rels)) =
                                codescope_core::conversation::parse_memory_file(
                                    &p,
                                    repo_name,
                                    &known_entities,
                                )
                            {
                                let _ = builder.insert_entities(&ents).await;
                                let _ = builder.insert_relations(&rels).await;
                                mem_count += 1;
                            }
                        }
                    }
                }
            }

            if conv_count > 0 || mem_count > 0 {
                println!(
                    "  Conversations: {} sessions, {} memory files",
                    conv_count, mem_count
                );
            }
        }
    }

    Ok(())
}

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use codescope_core::parser::CodeParser;
use codescope_core::graph::schema;
use codescope_core::graph::builder::GraphBuilder;
use codescope_core::graph::query::GraphQuery;
use codescope_core::temporal::GitAnalyzer;

#[derive(Parser)]
#[command(name = "codescope")]
#[command(about = "Codescope — Rust-native code intelligence engine")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Repository name for DB isolation (default: current directory name)
    #[arg(long, global = true)]
    repo: Option<String>,

    /// Override database path (default: ~/.codescope/db/<repo>/)
    #[arg(long, global = true)]
    db_path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Index a codebase into the knowledge graph
    Index {
        /// Path to the codebase to index
        path: PathBuf,

        /// Clear existing data for this repo before indexing
        #[arg(long)]
        clean: bool,
    },

    /// Search the code graph
    Search {
        /// Search query
        query: String,

        /// Limit results
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Query the graph with raw SurrealQL
    Query {
        /// SurrealQL query
        surql: String,
    },

    /// Show graph statistics
    Stats,

    /// Analyze git history
    History {
        /// Path to the git repository
        path: PathBuf,

        #[command(subcommand)]
        action: HistoryAction,
    },

    /// Generate embeddings for indexed functions
    Embed {
        /// Embedding provider (fastembed, ollama, openai)
        #[arg(long, default_value = "fastembed")]
        provider: String,

        /// Batch size
        #[arg(long, default_value = "100")]
        batch_size: usize,

        /// Ollama base URL
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,

        /// Model name
        #[arg(long, default_value = "nomic-embed-text")]
        model: String,
    },

    /// Semantic search using embeddings
    SemanticSearch {
        /// Natural language query
        query: String,

        /// Limit results
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Embedding provider (fastembed, ollama, openai)
        #[arg(long, default_value = "fastembed")]
        provider: String,

        /// Ollama base URL
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,

        /// Model name
        #[arg(long, default_value = "nomic-embed-text")]
        model: String,
    },

    /// Sync git history into the graph database
    SyncHistory {
        /// Path to the git repository
        path: PathBuf,

        /// Number of recent commits to sync
        #[arg(long, default_value = "200")]
        limit: usize,
    },

    /// Detect code hotspots (high complexity + high churn)
    Hotspots,

    /// List supported languages
    Languages,

    /// Initialize Codescope in current project (creates .mcp.json + first index)
    Init {
        /// Path to project (default: current directory)
        path: Option<PathBuf>,
    },

    /// Install codescope binary to ~/.local/bin (adds to PATH)
    Install,
}

#[derive(Subcommand)]
enum HistoryAction {
    /// Show recent commits
    Commits {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show file churn (most changed files)
    Churn {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show change coupling (files changed together)
    Coupling {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show contributor map
    Contributors,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Global repo name: --repo flag > current directory name
    let global_repo = cli.repo.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".into())
    });

    match cli.command {
        Commands::Index { path, clean } => {
            let repo_name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| global_repo.clone());
            cmd_index(path, &repo_name, clean, cli.db_path).await?;
        }
        Commands::Search { query, limit } => {
            cmd_search(&query, limit, &global_repo, cli.db_path).await?;
        }
        Commands::Query { surql } => {
            cmd_query(&surql, &global_repo, cli.db_path).await?;
        }
        Commands::Stats => {
            cmd_stats(&global_repo, cli.db_path).await?;
        }
        Commands::History { path, action } => {
            cmd_history(path, action)?;
        }
        Commands::SyncHistory { path, limit } => {
            cmd_sync_history(path, &global_repo, limit, cli.db_path).await?;
        }
        Commands::Hotspots => {
            cmd_hotspots(&global_repo, cli.db_path).await?;
        }
        Commands::Embed { provider, batch_size, ollama_url, model } => {
            cmd_embed(&provider, batch_size, &ollama_url, &model, &global_repo, cli.db_path).await?;
        }
        Commands::SemanticSearch { query, limit, provider, ollama_url, model } => {
            cmd_semantic_search(&query, limit, &provider, &ollama_url, &model, &global_repo, cli.db_path).await?;
        }
        Commands::Languages => {
            cmd_languages();
        }
        Commands::Init { path } => {
            let project_path = path.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            cmd_init(project_path, &global_repo, cli.db_path).await?;
        }
        Commands::Install => {
            cmd_install()?;
        }
    }

    Ok(())
}

/// Central DB path: ~/.codescope/db/<repo_name>/
fn default_db_path(repo_name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db")
        .join(repo_name)
}

async fn connect_db(db_path: Option<PathBuf>, repo_name: &str) -> Result<surrealdb::Surreal<surrealdb::engine::local::Db>> {
    let path = db_path.unwrap_or_else(|| default_db_path(repo_name));

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Migrate from old RocksDB format if needed
    if path.join("CURRENT").exists() || path.join("LOCK").exists() {
        let backup = path.with_extension("rocksdb.bak");
        eprintln!(
            "⚠ Old RocksDB data detected at {}.\n  Moving to {} — will re-index with SurrealKV.",
            path.display(), backup.display()
        );
        let _ = std::fs::rename(&path, &backup);
        std::fs::create_dir_all(&path)?;
    }

    let db = match surrealdb::Surreal::new::<surrealdb::engine::local::SurrealKv>(
        path.to_string_lossy().as_ref()
    ).await {
        Ok(db) => db,
        Err(e) => {
            anyhow::bail!(
                "Failed to open database at {}.\n\
                 Error: {}\n\
                 \n\
                 Try re-indexing or removing the DB directory:\n\
                 rm -rf {}", path.display(), e, path.display()
            );
        }
    };

    db.use_ns("codescope").use_db(repo_name).await?;
    schema::init_schema(&db).await?;

    Ok(db)
}

async fn cmd_index(path: PathBuf, repo_name: &str, clean: bool, db_path: Option<PathBuf>) -> Result<()> {
    use codescope_core::graph::IncrementalIndexer;
    use std::collections::HashMap;
    use std::time::Instant;

    let start_time = Instant::now();
    let db = connect_db(db_path, repo_name).await?;
    let builder = GraphBuilder::new(db.clone());
    let incremental = IncrementalIndexer::new(db.clone());

    if clean {
        println!("Full re-index: clearing existing data for '{}'...", repo_name);
        builder.clear_repo(repo_name).await?;
    } else {
        println!("Incremental index of {} as '{}'...", path.display(), repo_name);
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

        let results: Vec<(String, Result<(Vec<codescope_core::CodeEntity>, Vec<codescope_core::CodeRelation>), String>)> = all_files
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
    println!(
        "  Parsed {} files, {} unchanged ({:.1}s)",
        results.len(),
        files_skipped,
        parse_start.elapsed().as_secs_f64()
    );

    // Phase 4: Batch insert results into DB (async, sequential for DB safety)
    let insert_start = Instant::now();
    let mut total_entities = 0usize;
    let mut total_relations = 0usize;
    let mut files_processed = 0usize;
    let mut errors = Vec::new();

    for (rel_path, result) in results {
        match result {
            Ok((entities, relations)) => {
                // Delete old entities for incremental re-index
                if !clean {
                    let _ = builder.delete_file_entities(&rel_path, repo_name).await;
                }

                let ent_count = entities.len();
                let rel_count = relations.len();

                builder.insert_entities(&entities).await?;
                builder.insert_relations(&relations).await?;

                total_entities += ent_count;
                total_relations += rel_count;
                files_processed += 1;

                if files_processed % 100 == 0 {
                    println!("  ... {} files indexed", files_processed);
                }
            }
            Err(e) => {
                errors.push(format!("{}: {}", rel_path, e));
            }
        }
    }

    let total_time = start_time.elapsed();
    println!();
    println!("Indexing complete! ({:.1}s total)", total_time.as_secs_f64());
    println!("  Files indexed:      {}", files_processed);
    if files_skipped > 0 {
        println!("  Files unchanged:    {}", files_skipped);
    }
    println!("  Entities extracted: {}", total_entities);
    println!("  Relations created:  {}", total_relations);
    println!(
        "  Parse time:         {:.1}s",
        parse_start.elapsed().as_secs_f64()
    );
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

    Ok(())
}

async fn cmd_search(query: &str, limit: usize, repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path, repo).await?;
    let gq = GraphQuery::new(db);

    let results = gq.search_functions(query).await?;

    if results.is_empty() {
        println!("No results found for '{}'", query);
        return Ok(());
    }

    for (i, r) in results.iter().enumerate().take(limit) {
        println!(
            "{}. {} ({}:{})",
            i + 1,
            r.name.as_deref().unwrap_or("?"),
            r.file_path.as_deref().unwrap_or("?"),
            r.start_line.unwrap_or(0),
        );
        if let Some(sig) = &r.signature {
            println!("   {}", sig);
        }
    }

    Ok(())
}

async fn cmd_query(surql: &str, repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path, repo).await?;
    let gq = GraphQuery::new(db);

    let result = gq.raw_query(surql).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}

async fn cmd_stats(repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path, repo).await?;
    let gq = GraphQuery::new(db);

    let stats = gq.stats().await?;
    println!("{}", serde_json::to_string_pretty(&stats)?);

    Ok(())
}

fn cmd_history(path: PathBuf, action: HistoryAction) -> Result<()> {
    let analyzer = GitAnalyzer::open(&path)?;

    match action {
        HistoryAction::Commits { limit } => {
            let commits = analyzer.recent_commits(limit)?;
            for c in &commits {
                println!(
                    "{} {} — {} ({} files)",
                    &c.hash[..8],
                    c.author,
                    c.message.lines().next().unwrap_or(""),
                    c.files_changed.len()
                );
            }
        }
        HistoryAction::Churn { limit } => {
            let churn = analyzer.file_churn(limit)?;
            for (file, count) in &churn {
                println!("{:>4}  {}", count, file);
            }
        }
        HistoryAction::Coupling { limit } => {
            let coupling = analyzer.change_coupling(limit)?;
            for (a, b, count) in &coupling {
                println!("{:>4}  {} <-> {}", count, a, b);
            }
        }
        HistoryAction::Contributors => {
            let map = analyzer.contributor_map()?;
            for (author, files) in &map {
                println!("{} ({} files touched):", author, files.len());
                for (file, count) in files.iter().take(5) {
                    println!("  {:>4}  {}", count, file);
                }
                if files.len() > 5 {
                    println!("  ... and {} more", files.len() - 5);
                }
            }
        }
    }

    Ok(())
}

async fn cmd_sync_history(path: PathBuf, repo_name: &str, limit: usize, db_path: Option<PathBuf>) -> Result<()> {
    use codescope_core::temporal::{GitAnalyzer, TemporalGraphSync};

    let db = connect_db(db_path, repo_name).await?;
    let analyzer = GitAnalyzer::open(&path)?;
    let sync = TemporalGraphSync::new(db);

    println!("Syncing {} recent commits for '{}'...", limit, repo_name);
    let count = sync.sync_commits(&analyzer, repo_name, limit).await?;
    println!("Synced {} commits", count);

    Ok(())
}

async fn cmd_hotspots(repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    use codescope_core::temporal::TemporalGraphSync;

    let db = connect_db(db_path, repo).await?;
    let sync = TemporalGraphSync::new(db);

    let hotspots = sync.calculate_hotspots(repo).await?;

    if hotspots.is_empty() {
        println!("No hotspots found. Run 'sync-history' first.");
        return Ok(());
    }

    println!("{:<30} {:<40} {:>6} {:>6} {:>10}", "Function", "File", "Size", "Churn", "Risk");
    println!("{}", "-".repeat(96));

    for h in &hotspots {
        println!(
            "{:<30} {:<40} {:>6} {:>6} {:>10}",
            h.name.as_deref().unwrap_or("?"),
            h.file_path.as_deref().unwrap_or("?"),
            h.size.unwrap_or(0),
            h.churn.unwrap_or(0),
            h.risk_score.unwrap_or(0),
        );
    }

    Ok(())
}

async fn cmd_embed(
    provider: &str,
    batch_size: usize,
    ollama_url: &str,
    model: &str,
    repo: &str,
    db_path: Option<PathBuf>,
) -> Result<()> {
    use codescope_core::embeddings::{EmbeddingPipeline, FastEmbedProvider, OllamaProvider, OpenAIProvider};

    let db = connect_db(db_path, repo).await?;

    let embedding_provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider {
        "fastembed" => {
            println!("Using FastEmbed (local, in-process). Model downloads on first run.");
            Box::new(FastEmbedProvider::from_name(model)?)
        }
        "ollama" => Box::new(OllamaProvider::new(
            Some(ollama_url.to_string()),
            Some(model.to_string()),
        )),
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;
            Box::new(OpenAIProvider::new(api_key, Some(model.to_string())))
        }
        _ => return Err(anyhow::anyhow!("Unknown provider: {}. Use 'fastembed', 'ollama', or 'openai'", provider)),
    };

    println!("Embedding with {} (model: {})...", provider, model);

    let pipeline = EmbeddingPipeline::new(db, embedding_provider);
    let count = pipeline.embed_functions(batch_size).await?;

    println!("Embedded {} functions", count);
    Ok(())
}

async fn cmd_semantic_search(
    query: &str,
    limit: usize,
    provider: &str,
    ollama_url: &str,
    model: &str,
    repo: &str,
    db_path: Option<PathBuf>,
) -> Result<()> {
    use codescope_core::embeddings::{EmbeddingPipeline, FastEmbedProvider, OllamaProvider, OpenAIProvider};

    let db = connect_db(db_path, repo).await?;

    let embedding_provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider {
        "fastembed" => Box::new(FastEmbedProvider::from_name(model)?),
        "ollama" => Box::new(OllamaProvider::new(
            Some(ollama_url.to_string()),
            Some(model.to_string()),
        )),
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;
            Box::new(OpenAIProvider::new(api_key, Some(model.to_string())))
        }
        _ => return Err(anyhow::anyhow!("Unknown provider: {}", provider)),
    };

    let pipeline = EmbeddingPipeline::new(db, embedding_provider);
    let results = pipeline.semantic_search(query, limit).await?;

    if results.is_empty() {
        println!("No semantic results for '{}'", query);
        return Ok(());
    }

    println!("Semantic search results for '{}':\n", query);
    for (i, r) in results.iter().enumerate() {
        println!(
            "{}. {} ({}:{}) — score: {:.4}",
            i + 1,
            r.name,
            r.file_path,
            r.start_line.unwrap_or(0),
            r.score.unwrap_or(0.0),
        );
        if let Some(sig) = &r.signature {
            println!("   {}", sig);
        }
    }

    Ok(())
}

fn cmd_languages() {
    let parser = CodeParser::new();
    println!("Supported languages:");
    for lang in parser.supported_languages() {
        println!("  - {}", lang);
    }
}

async fn cmd_init(project_path: PathBuf, repo_name: &str, db_path: Option<PathBuf>) -> Result<()> {
    use std::time::Instant;

    let project_path = std::fs::canonicalize(&project_path)
        .unwrap_or_else(|_| project_path.clone());
    // Strip Windows extended-length prefix (\\?\)
    let project_path = {
        let s = project_path.to_string_lossy();
        if s.starts_with(r"\\?\") {
            PathBuf::from(&s[4..])
        } else {
            project_path
        }
    };

    let repo_name = project_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| repo_name.to_string());

    println!("🔧 Initializing Codescope for '{}'...\n", repo_name);

    // Step 1: Find codescope-mcp binary
    let mcp_binary = find_mcp_binary();
    if mcp_binary.is_none() {
        eprintln!("⚠ codescope-mcp binary not found. Run 'codescope install' first,");
        eprintln!("  or build with: cargo build --release -p codescope-mcp");
    }

    // Step 2: Create .mcp.json
    let mcp_json_path = project_path.join(".mcp.json");
    let mcp_cmd = mcp_binary
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new("codescope-mcp"));

    let project_path_str = project_path.to_string_lossy().replace('\\', "\\\\");
    let mcp_cmd_str = mcp_cmd.to_string_lossy().replace('\\', "\\\\");

    let mcp_json = format!(
        r#"{{
  "mcpServers": {{
    "codescope": {{
      "command": "{}",
      "args": ["{}", "--repo", "{}", "--auto-index"]
    }}
  }}
}}
"#,
        mcp_cmd_str, project_path_str, repo_name
    );

    if mcp_json_path.exists() {
        println!("📄 .mcp.json already exists — updating...");
    } else {
        println!("📄 Creating .mcp.json...");
    }
    std::fs::write(&mcp_json_path, &mcp_json)?;
    println!("   {}", mcp_json_path.display());

    // Step 3: Add .mcp.json to .gitignore if not already there
    let gitignore_path = project_path.join(".gitignore");
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
        if !content.contains(".mcp.json") {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&gitignore_path)?;
            use std::io::Write;
            writeln!(f, "\n# Codescope MCP config (user-specific paths)\n.mcp.json")?;
            println!("📝 Added .mcp.json to .gitignore");
        }
    }

    // Step 4: First index
    println!("\n📊 Indexing codebase...");
    let start = Instant::now();
    let db = connect_db(db_path, &repo_name).await?;
    let builder = GraphBuilder::new(db.clone());
    let parser = CodeParser::new();

    // Discover files using ignore crate (respects .gitignore)
    let walker = ignore::WalkBuilder::new(&project_path)
        .hidden(false)
        .git_ignore(true)
        .build();

    let all_files: Vec<PathBuf> = walker
        .flatten()
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| {
            let fp = e.path();
            let ext = fp.extension().and_then(|e| e.to_str()).unwrap_or("");
            let fname = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            (parser.supports_extension(ext) || parser.supports_filename(fname))
                && !codescope_core::parser::should_skip_file(fp)
        })
        .map(|e| e.into_path())
        .collect();

    let mut file_count = 0;
    let mut entity_count = 0;
    let mut relation_count = 0;

    for file_path in &all_files {
        let rel_path = file_path.strip_prefix(&project_path).unwrap_or(file_path);
        let rel_str = rel_path.to_string_lossy().replace('\\', "/");

        if let Ok((entities, relations)) = parser.parse_source(
            std::path::Path::new(&rel_str),
            &std::fs::read_to_string(file_path).unwrap_or_default(),
            &repo_name,
        ) {
            let _ = builder.insert_entities(&entities).await;
            let _ = builder.insert_relations(&relations).await;
            entity_count += entities.len();
            relation_count += relations.len();
            file_count += 1;
        }

        if file_count % 100 == 0 && file_count > 0 {
            eprint!("\r   ... {} files indexed", file_count);
        }
    }
    if file_count >= 100 { eprintln!(); }

    // Resolve call targets
    let _ = builder.resolve_call_targets(&repo_name).await;

    let elapsed = start.elapsed();
    println!("   {} files, {} entities, {} relations ({:.1}s)",
        file_count, entity_count, relation_count, elapsed.as_secs_f64());

    // Step 5: Summary
    println!("\n✅ Codescope initialized!\n");
    println!("   Next time you open this project in Claude Code,");
    println!("   Codescope starts automatically with 36 MCP tools.\n");
    println!("   Manual commands:");
    println!("     codescope search <query> --repo {}", repo_name);
    println!("     codescope stats --repo {}", repo_name);
    println!("     codescope-web --repo {} --port 8080", repo_name);

    Ok(())
}

fn cmd_install() -> Result<()> {
    // Find the compiled binary
    let exe_name = if cfg!(windows) { "codescope-mcp.exe" } else { "codescope-mcp" };
    let cli_exe = if cfg!(windows) { "codescope.exe" } else { "codescope" };
    let web_exe = if cfg!(windows) { "codescope-web.exe" } else { "codescope-web" };

    // Try to find from same directory as current executable, or from target/release
    let current_exe = std::env::current_exe().ok();
    let source_dir = current_exe.as_ref()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf());

    let install_dir = if cfg!(windows) {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local")
            .join("bin")
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local")
            .join("bin")
    };

    std::fs::create_dir_all(&install_dir)?;

    let mut installed = Vec::new();

    for binary in &[exe_name, cli_exe, web_exe] {
        let source = source_dir.as_ref().map(|d| d.join(binary));
        if let Some(src) = &source {
            if src.exists() {
                let dest = install_dir.join(binary);
                std::fs::copy(src, &dest)?;
                installed.push(dest.display().to_string());
            }
        }
    }

    if installed.is_empty() {
        println!("⚠ No binaries found. Build first:\n");
        println!("  cargo build --release");
        println!("\nThen run from the release directory:");
        println!("  ./target/release/codescope install");
        return Ok(());
    }

    println!("✅ Installed {} binaries to {}:\n", installed.len(), install_dir.display());
    for p in &installed {
        println!("   {}", p);
    }

    // Check if install_dir is in PATH
    let path_var = std::env::var("PATH").unwrap_or_default();
    let install_str = install_dir.to_string_lossy();
    if !path_var.contains(install_str.as_ref()) {
        println!("\n⚠ {} is not in your PATH. Add it:", install_dir.display());
        if cfg!(windows) {
            println!("\n  PowerShell (permanent):");
            println!("  [Environment]::SetEnvironmentVariable('PATH', $env:PATH + ';{}', 'User')", install_dir.display());
        } else {
            println!("\n  echo 'export PATH=\"{}:$PATH\"' >> ~/.bashrc && source ~/.bashrc", install_dir.display());
        }
    }

    println!("\n🚀 Now run in any project:");
    println!("   cd <your-project>");
    println!("   codescope init");

    Ok(())
}

/// Find the codescope-mcp binary — check PATH, common locations, and sibling dir.
fn find_mcp_binary() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { "codescope-mcp.exe" } else { "codescope-mcp" };

    // Check ~/.local/bin
    let local_bin = dirs::home_dir()
        .map(|h| h.join(".local").join("bin").join(exe_name));
    if let Some(ref p) = local_bin {
        if p.exists() { return Some(p.clone()); }
    }

    // Check same directory as current executable
    if let Ok(current) = std::env::current_exe() {
        let sibling = current.parent().map(|p| p.join(exe_name));
        if let Some(ref p) = sibling {
            if p.exists() { return Some(p.clone()); }
        }
    }

    // Check if in PATH
    if let Ok(output) = std::process::Command::new("which")
        .arg(exe_name)
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    // Windows: try where.exe
    if cfg!(windows) {
        if let Ok(output) = std::process::Command::new("where.exe")
            .arg(exe_name)
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout)
                    .lines().next().unwrap_or("").trim().to_string();
                if !path.is_empty() {
                    return Some(PathBuf::from(path));
                }
            }
        }
    }

    None
}

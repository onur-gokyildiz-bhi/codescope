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

    /// Database path (default: .graph-rag/db in the target directory)
    #[arg(long, global = true)]
    db_path: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Index a codebase into the knowledge graph
    Index {
        /// Path to the codebase to index
        path: PathBuf,

        /// Repository name (default: directory name)
        #[arg(long)]
        repo: Option<String>,

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
        /// Embedding provider (ollama, openai)
        #[arg(long, default_value = "ollama")]
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

        /// Embedding provider
        #[arg(long, default_value = "ollama")]
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

        /// Repository name
        #[arg(long)]
        repo: Option<String>,

        /// Number of recent commits to sync
        #[arg(long, default_value = "200")]
        limit: usize,
    },

    /// Detect code hotspots (high complexity + high churn)
    Hotspots {
        /// Repository name
        #[arg(long, default_value = "default")]
        repo: String,
    },

    /// List supported languages
    Languages,
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

    match cli.command {
        Commands::Index { path, repo, clean } => {
            cmd_index(path, repo, clean, cli.db_path).await?;
        }
        Commands::Search { query, limit } => {
            cmd_search(&query, limit, cli.db_path).await?;
        }
        Commands::Query { surql } => {
            cmd_query(&surql, cli.db_path).await?;
        }
        Commands::Stats => {
            cmd_stats(cli.db_path).await?;
        }
        Commands::History { path, action } => {
            cmd_history(path, action)?;
        }
        Commands::SyncHistory { path, repo, limit } => {
            cmd_sync_history(path, repo, limit, cli.db_path).await?;
        }
        Commands::Hotspots { repo } => {
            cmd_hotspots(&repo, cli.db_path).await?;
        }
        Commands::Embed { provider, batch_size, ollama_url, model } => {
            cmd_embed(&provider, batch_size, &ollama_url, &model, cli.db_path).await?;
        }
        Commands::SemanticSearch { query, limit, provider, ollama_url, model } => {
            cmd_semantic_search(&query, limit, &provider, &ollama_url, &model, cli.db_path).await?;
        }
        Commands::Languages => {
            cmd_languages();
        }
    }

    Ok(())
}

async fn connect_db(db_path: Option<PathBuf>) -> Result<surrealdb::Surreal<surrealdb::engine::local::Db>> {
    let path = db_path.unwrap_or_else(|| PathBuf::from(".graph-rag/db"));

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let db = surrealdb::Surreal::new::<surrealdb::engine::local::RocksDb>(
        path.to_string_lossy().as_ref()
    ).await?;

    db.use_ns("graph_rag").use_db("code").await?;
    schema::init_schema(&db).await?;

    Ok(db)
}

async fn cmd_index(path: PathBuf, repo: Option<String>, clean: bool, db_path: Option<PathBuf>) -> Result<()> {
    let repo_name = repo.unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into())
    });

    println!("Indexing {} as repo '{}'...", path.display(), repo_name);

    let db = connect_db(db_path).await?;
    let builder = GraphBuilder::new(db.clone());

    if clean {
        println!("Clearing existing data for repo '{}'...", repo_name);
        builder.clear_repo(&repo_name).await?;
    }

    let parser = CodeParser::new();

    let mut total_entities = 0usize;
    let mut total_relations = 0usize;
    let mut files_processed = 0usize;
    let mut errors = Vec::new();

    // Walk the directory, respecting .gitignore
    let walker = ignore::WalkBuilder::new(&path)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Walk error: {}", e));
                continue;
            }
        };

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let file_path = entry.path();
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let filename = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !parser.supports_extension(ext) && !parser.supports_filename(filename) {
            continue;
        }

        // Get relative path
        let rel_path = file_path
            .strip_prefix(&path)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");

        match parser.parse_file(file_path, &repo_name) {
            Ok((entities, relations)) => {
                let ent_count = entities.len();
                let rel_count = relations.len();

                builder.insert_entities(&entities).await?;
                builder.insert_relations(&relations).await?;

                total_entities += ent_count;
                total_relations += rel_count;
                files_processed += 1;

                if files_processed % 50 == 0 {
                    println!("  ... {} files processed", files_processed);
                }
            }
            Err(e) => {
                errors.push(format!("{}: {}", rel_path, e));
            }
        }
    }

    println!();
    println!("Indexing complete!");
    println!("  Files processed:    {}", files_processed);
    println!("  Entities extracted: {}", total_entities);
    println!("  Relations created:  {}", total_relations);

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

async fn cmd_search(query: &str, limit: usize, db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path).await?;
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

async fn cmd_query(surql: &str, db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path).await?;
    let gq = GraphQuery::new(db);

    let result = gq.raw_query(surql).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}

async fn cmd_stats(db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path).await?;
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

async fn cmd_sync_history(path: PathBuf, repo: Option<String>, limit: usize, db_path: Option<PathBuf>) -> Result<()> {
    use codescope_core::temporal::{GitAnalyzer, TemporalGraphSync};

    let repo_name = repo.unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".into())
    });

    let db = connect_db(db_path).await?;
    let analyzer = GitAnalyzer::open(&path)?;
    let sync = TemporalGraphSync::new(db);

    println!("Syncing {} recent commits for '{}'...", limit, repo_name);
    let count = sync.sync_commits(&analyzer, &repo_name, limit).await?;
    println!("Synced {} commits", count);

    Ok(())
}

async fn cmd_hotspots(repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    use codescope_core::temporal::TemporalGraphSync;

    let db = connect_db(db_path).await?;
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
    db_path: Option<PathBuf>,
) -> Result<()> {
    use codescope_core::embeddings::{EmbeddingPipeline, OllamaProvider, OpenAIProvider};

    let db = connect_db(db_path).await?;

    let embedding_provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider {
        "ollama" => Box::new(OllamaProvider::new(
            Some(ollama_url.to_string()),
            Some(model.to_string()),
        )),
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY environment variable not set"))?;
            Box::new(OpenAIProvider::new(api_key, Some(model.to_string())))
        }
        _ => return Err(anyhow::anyhow!("Unknown provider: {}. Use 'ollama' or 'openai'", provider)),
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
    db_path: Option<PathBuf>,
) -> Result<()> {
    use codescope_core::embeddings::{EmbeddingPipeline, OllamaProvider, OpenAIProvider};

    let db = connect_db(db_path).await?;

    let embedding_provider: Box<dyn codescope_core::embeddings::EmbeddingProvider> = match provider {
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

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use rmcp::ServiceExt;

mod server;
mod tools;

#[derive(Parser)]
#[command(name = "codescope-mcp")]
#[command(about = "Codescope MCP Server — Code intelligence for AI agents")]
struct Args {
    /// Path to the codebase to analyze
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Repository name
    #[arg(long)]
    repo: Option<String>,

    /// Database path
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// Auto-index on startup
    #[arg(long)]
    auto_index: bool,

    /// Embedding provider (ollama, openai, none)
    #[arg(long, default_value = "none")]
    embeddings: String,

    /// Ollama base URL
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,

    /// Ollama model for embeddings
    #[arg(long, default_value = "nomic-embed-text")]
    ollama_model: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args = Args::parse();

    let repo_name = args.repo.unwrap_or_else(|| {
        args.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".into())
    });

    let db_path = args
        .db_path
        .unwrap_or_else(|| args.path.join(".graph-rag/db"));

    tracing::info!("Starting graph-rag MCP server for repo '{}'", repo_name);

    // Connect to SurrealDB
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db =
        surrealdb::Surreal::new::<surrealdb::engine::local::RocksDb>(db_path.to_string_lossy().as_ref())
            .await?;
    db.use_ns("graph_rag").use_db("code").await?;
    codescope_core::graph::schema::init_schema(&db).await?;

    // Auto-index if requested
    if args.auto_index {
        tracing::info!("Auto-indexing {}...", args.path.display());
        let parser = codescope_core::parser::CodeParser::new();
        let builder = codescope_core::graph::builder::GraphBuilder::new(db.clone());

        let walker = ignore::WalkBuilder::new(&args.path)
            .hidden(true)
            .git_ignore(true)
            .build();

        let mut files = 0;
        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }
            let file_path = entry.path();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let filename = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !parser.supports_extension(ext) && !parser.supports_filename(filename) {
                continue;
            }
            if let Ok((entities, relations)) = parser.parse_file(file_path, &repo_name) {
                let _ = builder.insert_entities(&entities).await;
                let _ = builder.insert_relations(&relations).await;
                files += 1;
            }
        }
        tracing::info!("Indexed {} files", files);
    }

    // Create and run the MCP server
    let mcp_server = server::GraphRagServer::new(db, repo_name, args.path);

    let service = mcp_server.serve(rmcp::transport::stdio()).await?;
    tracing::info!("MCP server running on stdio");
    service.waiting().await?;

    Ok(())
}

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "codescope-web")]
#[command(about = "Codescope Web UI — Graph visualization dashboard")]
struct Args {
    /// Path to the codebase to visualize
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Database path (default: ~/.codescope/db/{repo})
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// Repository name (used to find DB at ~/.codescope/db/{repo})
    #[arg(long)]
    repo: Option<String>,

    /// Port to listen on
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Auto-index the codebase on startup
    #[arg(long)]
    auto_index: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    codescope_web::run_web(
        args.path,
        args.repo,
        args.port,
        args.auto_index,
        args.db_path,
    )
    .await
}

pub mod daemon;
pub mod helpers;
pub mod indexing;
pub mod nlp;
pub mod params;
pub mod server;
pub mod watcher;

pub use server::GraphRagServer;

use anyhow::Result;
use std::path::PathBuf;

use rmcp::ServiceExt;

/// Run MCP server in stdio mode — single project, one process.
/// This is the main entry point used by both the standalone binary and the unified CLI.
pub async fn run_stdio(path: PathBuf, repo: Option<String>, auto_index: bool) -> Result<()> {
    // Debug log to file (always, for troubleshooting MCP startup)
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file = log_dir.join("mcp-debug.log");
    let _ = std::fs::write(&log_file, format!(
        "[{}] Starting codescope-mcp\n  path: {:?}\n  repo: {:?}\n  auto_index: {}\n  cwd: {:?}\n  pid: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        path, repo, auto_index,
        std::env::current_dir().ok(),
        std::process::id(),
    ));

    let repo_name = repo.unwrap_or_else(|| {
        path.canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".into())
    });

    let db_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db")
        .join(&repo_name);

    // Append to debug log
    let _ = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_file)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "  repo_name: {}\n  db_path: {:?}", repo_name, db_path)
        });

    tracing::info!(
        "Stdio mode: repo '{}', db: {}",
        repo_name,
        db_path.display()
    );

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = match surrealdb::Surreal::new::<surrealdb::engine::local::SurrealKv>(
        db_path.to_string_lossy().as_ref(),
    )
    .await
    {
        Ok(db) => db,
        Err(e) => {
            let _ = std::fs::OpenOptions::new()
                .append(true)
                .open(&log_file)
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "  DB ERROR: {}", e)
                });
            return Err(e.into());
        }
    };
    db.use_ns("codescope").use_db(&repo_name).await?;
    codescope_core::graph::schema::init_schema(&db).await?;

    let _ = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_file)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "  DB connected, MCP serving...")
        });

    // Create MCP server BEFORE spawning background tasks so we can share context_summary
    let mcp_server = server::GraphRagServer::new(db.clone(), repo_name.clone(), path.clone());

    // Background auto-index using IndexingPipeline orchestrator
    if auto_index {
        let pipeline = indexing::IndexingPipeline::new(
            db.clone(),
            repo_name.clone(),
            path.clone(),
            mcp_server.clone(),
        );
        tokio::spawn(async move {
            pipeline.run_full().await;
        });
    }

    // Spawn embedded web UI on port 9876 (same DB, no lock conflict)
    {
        let web_db = db.clone();
        tokio::spawn(async move {
            let router = codescope_web::build_web_router(web_db);
            match tokio::net::TcpListener::bind("127.0.0.1:9876").await {
                Ok(listener) => {
                    tracing::info!("Web UI: http://localhost:9876");
                    let _ = axum::serve(listener, router).await;
                }
                Err(e) => tracing::debug!("Web UI not started (port 9876 busy): {}", e),
            }
        });
    }

    let service = mcp_server.serve(rmcp::transport::stdio()).await?;
    tracing::info!("MCP server running on stdio");
    service.waiting().await?;

    // Graceful shutdown — give background tasks time to finish
    tracing::info!("MCP session ended, shutting down...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    Ok(())
}

/// Recursively collect all .jsonl files in a directory
pub fn collect_jsonl_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_jsonl_files(&path, out);
            } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                out.push(path);
            }
        }
    }
}

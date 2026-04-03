use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use rmcp::ServiceExt;

mod daemon;
mod server;
mod tools;
mod watcher;

#[derive(Parser)]
#[command(name = "codescope-mcp")]
#[command(about = "Codescope MCP Server — Code intelligence for AI agents")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to the codebase to analyze (stdio mode shorthand)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Repository name
    #[arg(long)]
    repo: Option<String>,

    /// Auto-index on startup
    #[arg(long)]
    auto_index: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Run as stdio MCP server (default, one project per process)
    Stdio {
        /// Path to the codebase
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Repository name
        #[arg(long)]
        repo: Option<String>,

        /// Auto-index on startup
        #[arg(long)]
        auto_index: bool,
    },

    /// Run as SSE daemon (single process, multi-project)
    Serve {
        /// Port to listen on
        #[arg(long, default_value = "3333")]
        port: u16,

        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// Start daemon in background
    Start {
        #[arg(long, default_value = "3333")]
        port: u16,
    },

    /// Stop running daemon
    Stop {
        #[arg(long, default_value = "3333")]
        port: u16,
    },

    /// Check daemon status
    Status {
        #[arg(long, default_value = "3333")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Immediate startup log — before ANYTHING else
    {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let log = home.join(".codescope").join("startup.log");
        let _ = std::fs::create_dir_all(log.parent().unwrap());
        let _ = std::fs::write(&log, format!(
            "ALIVE at {}\nargs: {:?}\ncwd: {:?}\npid: {}\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            std::env::args().collect::<Vec<_>>(),
            std::env::current_dir().ok(),
            std::process::id(),
        ));
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args = Args::parse();

    match args.command {
        Some(Command::Serve { port, bind }) => {
            run_daemon(&bind, port).await
        }
        Some(Command::Start { port }) => {
            start_daemon_background(port)
        }
        Some(Command::Stop { port }) => {
            stop_daemon(port).await
        }
        Some(Command::Status { port }) => {
            check_status(port).await
        }
        Some(Command::Stdio { path, repo, auto_index }) => {
            run_stdio(path, repo, auto_index).await
        }
        // No subcommand = backward-compatible stdio mode
        None => {
            run_stdio(args.path, args.repo, args.auto_index).await
        }
    }
}

/// Stdio mode — single project, one process (backward compatible)
async fn run_stdio(path: PathBuf, repo: Option<String>, auto_index: bool) -> Result<()> {
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
    let _ = std::fs::OpenOptions::new().append(true).open(&log_file).and_then(|mut f| {
        use std::io::Write;
        writeln!(f, "  repo_name: {}\n  db_path: {:?}", repo_name, db_path)
    });

    tracing::info!("Stdio mode: repo '{}', db: {}", repo_name, db_path.display());

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = match surrealdb::Surreal::new::<surrealdb::engine::local::RocksDb>(
        db_path.to_string_lossy().as_ref(),
    )
    .await {
        Ok(db) => db,
        Err(e) => {
            let _ = std::fs::OpenOptions::new().append(true).open(&log_file).and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "  DB ERROR: {}", e)
            });
            return Err(e.into());
        }
    };
    db.use_ns("codescope").use_db(&repo_name).await?;
    codescope_core::graph::schema::init_schema(&db).await?;

    let _ = std::fs::OpenOptions::new().append(true).open(&log_file).and_then(|mut f| {
        use std::io::Write;
        writeln!(f, "  DB connected, MCP serving...")
    });

    // Create MCP server BEFORE spawning background tasks so we can share context_summary
    let mcp_server = server::GraphRagServer::new(db.clone(), repo_name.clone(), path.clone());

    // Background auto-index with parallel parsing
    if auto_index {
        let index_db = db.clone();
        let index_path = path.clone();
        let index_repo = repo_name.clone();
        let mcp_handle = mcp_server.clone();
        tokio::spawn(async move {
            tracing::info!("Background indexing {}...", index_path.display());
            let builder = codescope_core::graph::builder::GraphBuilder::new(index_db.clone());

            // Phase 1: Collect + parse files in parallel (CPU-bound, rayon thread pool)
            let parse_path = index_path.clone();
            let parse_repo = index_repo.clone();
            let results = tokio::task::spawn_blocking(move || {
                use rayon::prelude::*;
                let parser = codescope_core::parser::CodeParser::new();
                let walker = ignore::WalkBuilder::new(&parse_path)
                    .hidden(true)
                    .git_ignore(true)
                    .build();

                let files: Vec<std::path::PathBuf> = walker
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

                tracing::info!("Found {} files to parse", files.len());

                files
                    .par_iter()
                    .filter_map(|file_path| {
                        let rel_path = file_path
                            .strip_prefix(&parse_path)
                            .unwrap_or(file_path)
                            .to_string_lossy()
                            .to_string()
                            .replace('\\', "/");
                        let content = std::fs::read_to_string(file_path).ok()?;
                        parser
                            .parse_source(std::path::Path::new(&rel_path), &content, &parse_repo)
                            .ok()
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();

            // Phase 2: Batch insert results (async DB operations)
            let mut file_count = 0;
            for (entities, relations) in results {
                let _ = builder.insert_entities(&entities).await;
                let _ = builder.insert_relations(&relations).await;
                file_count += 1;
            }

            tracing::info!("Background indexing complete: {} files", file_count);

            // Phase 2.5: Resolve cross-file call targets
            match builder.resolve_call_targets(&index_repo).await {
                Ok(resolved) if resolved > 0 => {
                    tracing::info!("Resolved {} cross-file call targets", resolved);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("Call target resolution failed: {}", e),
            }

            // Phase 3: Auto-index conversations + memory files
            let project_dir = server::find_claude_project_dir(&index_path, &index_repo);
            tracing::info!("Auto-indexing conversations from {}", project_dir.display());

            let known_entities: Vec<String> = Vec::new();

            let mut jsonl_files = Vec::new();
            collect_jsonl_files(&project_dir, &mut jsonl_files);

            let mut conv_count = 0;
            for jsonl_path in &jsonl_files {
                match codescope_core::conversation::parse_conversation(jsonl_path, &index_repo, &known_entities) {
                    Ok((entities, relations, _)) => {
                        let _ = builder.insert_entities(&entities).await;
                        let _ = builder.insert_relations(&relations).await;
                        conv_count += 1;
                    }
                    Err(e) => {
                        tracing::debug!("Conversation parse error {}: {}", jsonl_path.display(), e);
                    }
                }
            }

            // Index memory files
            let memory_dir = project_dir.join("memory");
            let mut mem_count = 0;
            if memory_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&memory_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map(|e| e == "md").unwrap_or(false) {
                            if let Ok((ents, rels)) = codescope_core::conversation::parse_memory_file(&path, &index_repo, &known_entities) {
                                let _ = builder.insert_entities(&ents).await;
                                let _ = builder.insert_relations(&rels).await;
                                mem_count += 1;
                            }
                        }
                    }
                }
            }

            tracing::info!(
                "Conversation indexing: {} sessions, {} memory files",
                conv_count, mem_count
            );

            // Phase 4: Generate CONTEXT.md + load context summary into MCP server
            server::generate_context_md(&index_db, &index_repo, &index_path).await;
            mcp_handle.load_context_summary().await;

            tracing::info!("Context summary loaded into MCP server instructions");

            // Phase 5: Start file watcher for live re-indexing
            match watcher::start_watcher(&index_path) {
                Ok(rx) => {
                    watcher::spawn_reindex_task(rx, index_db, index_repo, index_path);
                    tracing::info!("File watcher active — changes will auto-reindex");
                }
                Err(e) => {
                    tracing::warn!("File watcher failed to start: {}", e);
                }
            }
        });
    }

    let service = mcp_server.serve(rmcp::transport::stdio()).await?;
    tracing::info!("MCP server running on stdio");
    service.waiting().await?;

    Ok(())
}

/// Daemon mode — SSE server, multi-project
async fn run_daemon(bind: &str, port: u16) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", bind, port).parse()?;
    let state = Arc::new(daemon::DaemonState::new());

    // Write PID file for stop command
    let pid_path = pid_file_path(port);
    let _ = std::fs::create_dir_all(pid_path.parent().unwrap());
    std::fs::write(&pid_path, std::process::id().to_string())?;

    tracing::info!("Codescope daemon starting on {}", addr);
    eprintln!("Codescope daemon listening on http://{}", addr);

    let mut sse_server = rmcp::transport::sse_server::SseServer::serve(addr).await?;

    tracing::info!("SSE server ready, waiting for connections...");

    while let Some(transport) = sse_server.next_transport().await {
        let state = state.clone();
        tokio::spawn(async move {
            tracing::info!("New MCP connection");
            let handler = server::GraphRagServer::new_daemon(state);
            match handler.serve(transport).await {
                Ok(service) => {
                    if let Err(e) = service.waiting().await {
                        tracing::error!("Connection error: {}", e);
                    }
                }
                Err(e) => tracing::error!("Failed to serve connection: {}", e),
            }
        });
    }

    // Cleanup PID file on exit
    let _ = std::fs::remove_file(&pid_path);

    Ok(())
}

fn pid_file_path(port: u16) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join(format!("daemon-{}.pid", port))
}

/// Start daemon as a background process
fn start_daemon_background(port: u16) -> Result<()> {
    let exe = std::env::current_exe()?;
    let log_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("daemon.log");
    std::fs::create_dir_all(log_path.parent().unwrap())?;
    let log_file = std::fs::File::create(&log_path)?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        let child = Command::new(exe)
            .args(["serve", "--port", &port.to_string()])
            .env("RUST_LOG", "info")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::from(log_file))
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()?;
        eprintln!("Codescope daemon started (PID {}) on port {}", child.id(), port);
        eprintln!("Log: {}", log_path.display());
    }

    #[cfg(not(windows))]
    {
        use std::process::Command;
        let child = Command::new(exe)
            .args(["serve", "--port", &port.to_string()])
            .env("RUST_LOG", "info")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::from(log_file))
            .spawn()?;
        eprintln!("Codescope daemon started (PID {}) on port {}", child.id(), port);
        eprintln!("Log: {}", log_path.display());
    }

    Ok(())
}

/// Stop daemon by reading PID file and killing the process
async fn stop_daemon(port: u16) -> Result<()> {
    let pid_path = pid_file_path(port);

    let pid_str = match std::fs::read_to_string(&pid_path) {
        Ok(s) => s.trim().to_string(),
        Err(_) => {
            eprintln!("No daemon PID file found for port {}. Is the daemon running?", port);
            return Ok(());
        }
    };

    let pid: u32 = match pid_str.parse() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Invalid PID in file: {}", pid_str);
            let _ = std::fs::remove_file(&pid_path);
            return Ok(());
        }
    };

    // Kill the process
    #[cfg(windows)]
    {
        use std::process::Command;
        let result = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
        match result {
            Ok(output) if output.status.success() => {
                eprintln!("Daemon (PID {}) stopped.", pid);
            }
            _ => {
                eprintln!("Could not stop daemon (PID {}). Process may have already exited.", pid);
            }
        }
    }

    #[cfg(not(windows))]
    {
        use std::process::Command;
        let result = Command::new("kill")
            .args([&pid.to_string()])
            .output();
        match result {
            Ok(output) if output.status.success() => {
                eprintln!("Daemon (PID {}) stopped.", pid);
            }
            _ => {
                eprintln!("Could not stop daemon (PID {}). Process may have already exited.", pid);
            }
        }
    }

    let _ = std::fs::remove_file(&pid_path);
    Ok(())
}

/// Check daemon status
async fn check_status(port: u16) -> Result<()> {
    let url = format!("http://127.0.0.1:{}/sse", port);
    match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            eprintln!("Codescope daemon is running on port {}", port);
        }
        _ => {
            eprintln!("No daemon detected on port {}", port);
        }
    }
    Ok(())
}

/// Recursively collect all .jsonl files in a directory
fn collect_jsonl_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
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


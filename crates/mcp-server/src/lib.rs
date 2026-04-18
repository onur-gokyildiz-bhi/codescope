pub mod daemon;
pub mod helpers;
pub mod index_state;
pub mod indexing;
pub mod nlp;
pub mod params;
pub mod server;
pub mod telemetry;
pub mod tools;
pub mod watcher;

pub use server::GraphRagServer;

use anyhow::Result;
use std::path::PathBuf;

use rmcp::ServiceExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Run MCP server in stdio mode — single project, one process.
/// Backward-compatible 3-arg entrypoint (CLI crate still uses this).
/// Defaults to background auto-indexing — the serve loop starts immediately
/// and tools return a structured "indexing in progress" response (readiness
/// gate) until the build completes. This is what MCP clients expect: a
/// blocking startup can exceed the client's handshake timeout on large repos.
pub async fn run_stdio(path: PathBuf, repo: Option<String>, auto_index: bool) -> Result<()> {
    run_stdio_with_options(path, repo, auto_index, false).await
}

/// Run MCP server in stdio mode with full option control.
///
/// `auto_index_blocking` controls whether auto-indexing blocks the serve
/// loop (tools never see an empty graph on first call) or runs in the
/// background (fast startup; tools return "indexing in progress" until
/// the build completes). Default is background, because blocking startup
/// on large repos (~minutes) exceeds the MCP client's handshake timeout.
/// Opt into blocking with `--auto-index-blocking` or
/// `CODESCOPE_AUTO_INDEX_BLOCKING=1` for one-off CLI runs on small repos.
pub async fn run_stdio_with_options(
    path: PathBuf,
    repo: Option<String>,
    auto_index: bool,
    auto_index_blocking: bool,
) -> Result<()> {
    // Background is the default; user must explicitly opt into blocking.
    let blocking_mode = auto_index_blocking
        || std::env::var("CODESCOPE_AUTO_INDEX_BLOCKING")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
    let background_mode = !blocking_mode;

    // --- Logging ---
    //
    // In stdio MCP mode, stdout is reserved for JSON-RPC and stderr is
    // often swallowed by the host (Claude Desktop, Cursor, etc.), which
    // means `tracing` output to stderr was invisible for real deployments.
    // Write to a per-PID file under the platform state dir instead. stdout
    // stays JSON-RPC-clean; a one-line stderr notice tells users where to
    // find the log (in case the host *does* forward stderr somewhere).
    let (log_path, _guard) = init_stdio_logging();
    eprintln!("codescope-mcp log file: {}", log_path.display());

    let otel_provider = telemetry::init().ok().flatten();
    let otel_layer = otel_provider.as_ref().map(|provider| {
        use opentelemetry::trace::TracerProvider as _;
        let tracer = provider.tracer("codescope-mcp");
        tracing_opentelemetry::layer().with_tracer(tracer)
    });

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // File-based subscriber (non-blocking) + optional OTel layer.
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(_guard.writer())
        .with_ansi(false);

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(otel_layer)
        .try_init();

    tracing::info!("codescope-mcp logging to {}", log_path.display());
    if otel_provider.is_some() {
        tracing::info!("OpenTelemetry OTLP export enabled (CODESCOPE_OTLP_ENDPOINT)");
    } else {
        tracing::debug!("OpenTelemetry disabled (set CODESCOPE_OTLP_ENDPOINT to enable)");
    }

    // Debug log to file (always, for troubleshooting MCP startup)
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file = log_dir.join("mcp-debug.log");
    let _ = std::fs::write(&log_file, format!(
        "[{}] Starting codescope-mcp\n  path: {:?}\n  repo: {:?}\n  auto_index: {}\n  background: {}\n  cwd: {:?}\n  pid: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        path, repo, auto_index, background_mode,
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
    let db = match codescope_core::connect_path(&db_path).await {
        Ok(db) => db,
        Err(e) => {
            let _ = std::fs::OpenOptions::new()
                .append(true)
                .open(&log_file)
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "  DB ERROR: {}", e)
                });
            return Err(e);
        }
    };
    codescope_core::graph::schema::init_schema(&db).await?;
    codescope_core::graph::migrations::migrate_to_current(&db).await?;

    let _ = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_file)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "  DB connected, MCP serving...")
        });

    // Create MCP server BEFORE spawning background tasks so we can share context_summary
    let mcp_server = server::GraphRagServer::new(db.clone(), repo_name.clone(), path.clone());

    // Ensure .claude/rules/codescope-mandatory.md exists so Claude Code
    // is required to use codescope MCP tools instead of Read/Grep.
    // This runs on every MCP server startup (idempotent — skips if exists).
    {
        let rules_dir = path.join(".claude").join("rules");
        let rule_path = rules_dir.join("codescope-mandatory.md");
        if !rule_path.exists() {
            let _ = std::fs::create_dir_all(&rules_dir);
            let _ = std::fs::write(
                &rule_path,
                include_str!("../../../.claude/rules/codescope-mandatory.md"),
            );
            tracing::info!("Created .claude/rules/codescope-mandatory.md");
        }
    }

    // Auto-index: by default we .await the pipeline before serving so MCP
    // tools never see an empty graph. Power users who want the server up
    // immediately can pass --auto-index-background (or set the env var)
    // — the readiness gate on every tool handler still surfaces a
    // "indexing in progress" response instead of empty results.
    if auto_index {
        let pipeline = indexing::IndexingPipeline::new(
            db.clone(),
            repo_name.clone(),
            path.clone(),
            mcp_server.clone(),
        );

        if background_mode {
            tracing::info!("Auto-index starting in background (--auto-index-background)");
            // Note: `run_full` internally spawns the file watcher at
            // phase 5, so we don't need a separate spawn here — unlike
            // the old code, which double-started the watcher.
            //
            // TODO(race): state stays `Idle` between here and the first
            // `state.start()` call inside `run_full`. During that tiny
            // window (~microseconds on startup) a tool call would see
            // Idle and be let through with an empty graph. Acceptable
            // for v1 since it's sub-ms; tighten by flipping the state
            // to Indexing synchronously before the spawn if it ever
            // becomes a real issue.
            mcp_server.index_state().start().await;
            tokio::spawn(async move {
                pipeline.run_full().await;
            });
        } else {
            tracing::info!("Auto-index running (blocking) — tools will be ready on first call");
            // Blocking: wait for core phases to finish before accepting
            // tool calls. Non-critical phases (health loop, periodic
            // reindex) are still spawned in the background by run_full.
            pipeline.run_full().await;
            tracing::info!("Auto-index complete, starting MCP serve loop");
        }
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

    // Flush any pending OpenTelemetry spans before exit. No-op if OTLP
    // was never enabled.
    telemetry::shutdown();

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

// ─── File-based logging for stdio MCP mode ─────────────────────────────────

/// Resolve the platform-appropriate state directory for logs.
/// - Linux/Mac: `$XDG_STATE_HOME/codescope/logs` or `~/.local/state/codescope/logs`
/// - Windows:   `%LOCALAPPDATA%/codescope/logs`
fn resolve_log_dir() -> PathBuf {
    // Honor XDG_STATE_HOME first (portable override).
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("codescope").join("logs");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(lad) = std::env::var("LOCALAPPDATA") {
            if !lad.is_empty() {
                return PathBuf::from(lad).join("codescope").join("logs");
            }
        }
    }
    // Linux/Mac fallback: ~/.local/state/codescope/logs
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local")
        .join("state")
        .join("codescope")
        .join("logs")
}

/// Lightweight writer guard. Wraps a file in a mutex so the
/// tracing_subscriber `MakeWriter` impl is `Send + Sync`.
///
/// We deliberately avoid adding the `tracing-appender` crate here — its
/// non-blocking writer adds a background thread and a new dependency
/// for what we currently expect to be <100 log lines/sec. If log volume
/// ever becomes a tool-latency issue, swap this for `tracing_appender::
/// non_blocking` and keep the `_guard` around for the process lifetime.
/// TODO: revisit once we see real per-tool tracing volumes.
pub struct LogGuard {
    file: std::sync::Arc<std::sync::Mutex<std::fs::File>>,
}

impl LogGuard {
    pub fn writer(&self) -> FileMakeWriter {
        FileMakeWriter {
            file: self.file.clone(),
        }
    }
}

/// `MakeWriter` impl that produces a shared `Mutex<File>` handle on each
/// `make_writer` call. Good enough for per-tool-span logging volumes
/// (thousands of lines/sec worst case — well below fs limits).
#[derive(Clone)]
pub struct FileMakeWriter {
    file: std::sync::Arc<std::sync::Mutex<std::fs::File>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileMakeWriter {
    type Writer = FileWriter;
    fn make_writer(&'a self) -> Self::Writer {
        FileWriter {
            file: self.file.clone(),
        }
    }
}

pub struct FileWriter {
    file: std::sync::Arc<std::sync::Mutex<std::fs::File>>,
}

impl std::io::Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut f) = self.file.lock() {
            f.write(buf)
        } else {
            Ok(buf.len())
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if let Ok(mut f) = self.file.lock() {
            f.flush()
        } else {
            Ok(())
        }
    }
}

/// Initialize stdio-mode logging. Returns `(log_file_path, guard)` — the
/// guard must be kept alive for the process lifetime, otherwise the
/// underlying file handle drops.
fn init_stdio_logging() -> (PathBuf, LogGuard) {
    let dir = resolve_log_dir();
    let _ = std::fs::create_dir_all(&dir);
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let log_path = dir.join(format!("mcp-{}-{}.log", std::process::id(), ts));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap_or_else(|_| {
            // If the state dir isn't writable, fall back to a temp file so
            // the process still starts rather than panicking.
            let tmp =
                std::env::temp_dir().join(format!("codescope-mcp-{}.log", std::process::id()));
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&tmp)
                .unwrap_or_else(|_| std::fs::File::create(&tmp).expect("temp log file"))
        });
    let guard = LogGuard {
        file: std::sync::Arc::new(std::sync::Mutex::new(file)),
    };
    (log_path, guard)
}

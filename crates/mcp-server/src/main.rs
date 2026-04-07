use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

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
        #[arg(long, default_value = "9877")]
        port: u16,

        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// Start daemon in background
    Start {
        #[arg(long, default_value = "9877")]
        port: u16,
    },

    /// Stop running daemon
    Stop {
        #[arg(long, default_value = "9877")]
        port: u16,
    },

    /// Check daemon status
    Status {
        #[arg(long, default_value = "9877")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Immediate startup log — before ANYTHING else
    {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let log = home.join(".codescope").join("startup.log");
        if let Some(parent) = log.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &log,
            format!(
                "ALIVE at {}\nargs: {:?}\ncwd: {:?}\npid: {}\n",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                std::env::args().collect::<Vec<_>>(),
                std::env::current_dir().ok(),
                std::process::id(),
            ),
        );
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args = Args::parse();

    match args.command {
        Some(Command::Serve { port, bind }) => run_daemon(&bind, port).await,
        Some(Command::Start { port }) => start_daemon_background(port),
        Some(Command::Stop { port }) => stop_daemon(port).await,
        Some(Command::Status { port }) => check_status(port).await,
        Some(Command::Stdio {
            path,
            repo,
            auto_index,
        }) => codescope_mcp::run_stdio(path, repo, auto_index).await,
        // No subcommand = backward-compatible stdio mode
        None => codescope_mcp::run_stdio(args.path, args.repo, args.auto_index).await,
    }
}

/// Daemon mode — Streamable HTTP server, multi-project
async fn run_daemon(bind: &str, port: u16) -> Result<()> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    let addr: SocketAddr = format!("{}:{}", bind, port).parse()?;
    let state = Arc::new(codescope_mcp::daemon::DaemonState::new());

    // Write PID file
    let pid_path = pid_file_path(port);
    if let Some(parent) = pid_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let ct = CancellationToken::new();

    let service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || {
                Ok(codescope_mcp::server::GraphRagServer::new_daemon(
                    state.clone(),
                ))
            }
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_cancellation_token(ct.child_token()),
    );

    let router = axum::Router::new().nest_service("/mcp", service);

    tracing::info!("Codescope daemon starting on {}", addr);
    eprintln!("Codescope daemon listening on http://{}/mcp", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            ct.cancel();
        })
        .await?;

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
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
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
        eprintln!(
            "Codescope daemon started (PID {}) on port {}",
            child.id(),
            port
        );
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
        eprintln!(
            "Codescope daemon started (PID {}) on port {}",
            child.id(),
            port
        );
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
            eprintln!(
                "No daemon PID file found for port {}. Is the daemon running?",
                port
            );
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
                eprintln!(
                    "Could not stop daemon (PID {}). Process may have already exited.",
                    pid
                );
            }
        }
    }

    #[cfg(not(windows))]
    {
        use std::process::Command;
        let result = Command::new("kill").args([&pid.to_string()]).output();
        match result {
            Ok(output) if output.status.success() => {
                eprintln!("Daemon (PID {}) stopped.", pid);
            }
            _ => {
                eprintln!(
                    "Could not stop daemon (PID {}). Process may have already exited.",
                    pid
                );
            }
        }
    }

    let _ = std::fs::remove_file(&pid_path);
    Ok(())
}

/// Check daemon status
async fn check_status(port: u16) -> Result<()> {
    let url = format!("http://127.0.0.1:{}/mcp", port);
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

//! Daemon mode — MCP + Web UI on single port, multi-project.
//!
//! R5 routes:
//!
//! * `/mcp` — generic MCP endpoint. Session must call `init_project`
//!   to pin a repo; kept for back-compat.
//! * `/mcp/{repo}` — per-repo endpoint. Pre-binds the session to the
//!   repo inside `NS=codescope`, so tool calls work without
//!   `init_project`. Repos are pre-discovered at daemon startup from
//!   the bundled surreal server; late-added repos need a daemon
//!   restart to appear (acceptable for v1; add-on-demand can land
//!   later).

use anyhow::Result;
use codescope_core::daemon::DaemonState;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub async fn run(bind: &str, port: u16) -> Result<()> {
    let addr: std::net::SocketAddr = format!("{}:{}", bind, port).parse()?;
    let base_db_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db");
    let state = Arc::new(DaemonState::new(base_db_path));

    // Write PID file
    let pid_path = daemon_pid_path(port);
    if let Some(parent) = pid_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let ct = CancellationToken::new();

    // Generic /mcp endpoint — no repo bound; session must `init_project`.
    let generic_service = StreamableHttpService::new(
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

    let web_router = codescope_web::build_multi_web_router(state.clone());
    let mut router = web_router.nest_service("/mcp", generic_service);

    // Per-repo routes. Pre-discover repos from the surreal server and
    // mount each at `/mcp/{repo}`. Each gets its own
    // `StreamableHttpService` (thus its own session manager) so a bug
    // in one repo's sessions can't bleed into another.
    let repos = state.list_server_repos().await;
    let mut mounted: Vec<String> = Vec::new();
    for repo in &repos {
        // Skip underscore-prefixed internal namespaces (_global, meta).
        if repo.starts_with('_') {
            continue;
        }
        let per_repo_service = StreamableHttpService::new(
            {
                let state = state.clone();
                let repo_name = repo.clone();
                move || {
                    Ok(codescope_mcp::server::GraphRagServer::new_daemon_for_repo(
                        state.clone(),
                        repo_name.clone(),
                    ))
                }
            },
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default().with_cancellation_token(ct.child_token()),
        );
        let mount = format!("/mcp/{}", repo);
        router = router.nest_service(&mount, per_repo_service);
        mounted.push(mount);
    }

    eprintln!("Codescope daemon listening on http://{}", addr);
    eprintln!("  Web UI: http://{}/", addr);
    eprintln!(
        "  MCP:    http://{}/mcp  (generic, init_project required)",
        addr
    );
    if mounted.is_empty() {
        eprintln!(
            "  MCP /{{repo}}: no repos discovered on the surreal server — \
             index one first with `codescope index <path> --repo <name>`."
        );
    } else {
        eprintln!(
            "  MCP /{{repo}}: {} pre-bound route(s) — {}",
            mounted.len(),
            mounted.join(", ")
        );
    }

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

pub(crate) fn daemon_pid_path(port: u16) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join(format!("daemon-{}.pid", port))
}

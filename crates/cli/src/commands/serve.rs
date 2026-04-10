//! Daemon mode — MCP + Web UI on single port, multi-project

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

    let web_router = codescope_web::build_multi_web_router(state.clone());
    let router = web_router.nest_service("/mcp", service);

    eprintln!("Codescope daemon listening on http://{}", addr);
    eprintln!("  Web UI: http://{}/", addr);
    eprintln!("  MCP:    http://{}/mcp", addr);

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

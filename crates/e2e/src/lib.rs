//! Shared test fixtures for R3 smoke tests.
//!
//! Every suite needs a running `surreal` server. Rather than asking
//! contributors to pre-launch one, we spawn a short-lived server
//! scoped to the test's lifetime, bound to a free local port against
//! an in-memory store. Tests talk to it like any codescope client
//! would — the transport is identical to production, only the data
//! is ephemeral.
//!
//! Each test's fixture is independent (its own port, its own data)
//! so they can run in parallel without sharing state.

use anyhow::{anyhow, Context, Result};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

/// A live, in-memory surreal server owned by a test.
///
/// Dropping the fixture kills the server via `kill_on_drop(true)` —
/// no PID files, no lingering processes between runs.
pub struct TestServer {
    pub endpoint_ws: String,
    pub endpoint_http: String,
    pub port: u16,
    _child: Child,
}

impl TestServer {
    /// Start a surreal server on a free local port against `memory://`.
    /// The server is ready by the time this returns (health-checked).
    pub async fn start() -> Result<Self> {
        let bin = find_surreal_binary()?;
        let port = pick_free_port()?;
        let mut cmd = Command::new(&bin);
        cmd.args([
            "start",
            "memory",
            "--bind",
            &format!("127.0.0.1:{port}"),
            "--user",
            "root",
            "--pass",
            "root",
            "--log",
            "warn",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
        let child = cmd
            .spawn()
            .with_context(|| format!("spawn surreal test server on {port}"))?;

        // Poll /health. Memory engines come up in <100 ms typically;
        // 10 s cap covers slow CI runners.
        let health = format!("http://127.0.0.1:{port}/health");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(r) = reqwest::get(&health).await {
                if r.status().is_success() {
                    break;
                }
            }
            if Instant::now() >= deadline {
                return Err(anyhow!("test server on {port} never became healthy"));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(Self {
            endpoint_ws: format!("ws://127.0.0.1:{port}"),
            endpoint_http: format!("http://127.0.0.1:{port}"),
            port,
            _child: child,
        })
    }

    /// Set `CODESCOPE_DB_URL` for any child process spawned by the
    /// caller, pointing at this server.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        vec![
            ("CODESCOPE_DB_URL".into(), self.endpoint_ws.clone()),
            ("CODESCOPE_DB_USER".into(), "root".into()),
            ("CODESCOPE_DB_PASS".into(), "root".into()),
        ]
    }
}

/// Look up the pinned surreal binary. Same resolution order as
/// `migrate_to_server`: `~/.codescope/bin/surreal[.exe]`, then PATH.
pub fn find_surreal_binary() -> Result<PathBuf> {
    let exe = if cfg!(windows) {
        "surreal.exe"
    } else {
        "surreal"
    };
    if let Some(home) = dirs::home_dir() {
        let c = home.join(".codescope").join("bin").join(exe);
        if c.is_file() {
            return Ok(c);
        }
    }
    let probe = std::process::Command::new(exe)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match probe {
        Ok(s) if s.success() => Ok(PathBuf::from(exe)),
        _ => Err(anyhow!(
            "surreal binary not found — install it under ~/.codescope/bin/ \
             or put it on PATH for E2E tests"
        )),
    }
}

/// OS-assigned free port. Same trick as migrate_to_server.
pub fn pick_free_port() -> Result<u16> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    let p = l.local_addr()?.port();
    drop(l);
    Ok(p)
}

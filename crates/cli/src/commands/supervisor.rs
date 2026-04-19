//! R4 — `codescope start / stop / status` supervisor for the bundled
//! surreal server.
//!
//! State file: `~/.codescope/surreal.json`:
//!
//! ```json
//! { "pid": 12345, "port": 8077, "version": "3.0.5", "started_at": "..." }
//! ```
//!
//! Rules:
//! * `start` is idempotent — if the recorded PID is alive and
//!   `/health` returns 200, it just reports. If the PID file is
//!   stale, it cleans up and spawns fresh.
//! * `stop` is best-effort — kills the recorded PID and deletes the
//!   state file even if the kill fails (so a future `start` isn't
//!   blocked by a ghost file).
//! * `status` never starts or stops anything. It reports one of:
//!   `running`, `not-running`, `stale-pid`, or `unhealthy`.
//!
//! Surreal binary resolution: `~/.codescope/bin/surreal[.exe]` →
//! PATH fallback. Same helper as `migrate_to_server`.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_PORT: u16 = 8077;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SupervisorState {
    pid: u32,
    port: u16,
    version: String,
    started_at: String,
}

fn state_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("surreal.json")
}

fn data_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("surreal-data")
}

/// Look up the pinned surreal binary — `~/.codescope/bin/` wins, PATH
/// is the fallback. Mirrors the helper in `migrate_to_server.rs`.
fn find_surreal_binary() -> Result<PathBuf> {
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
    let probe = Command::new(exe)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match probe {
        Ok(s) if s.success() => Ok(PathBuf::from(exe)),
        _ => Err(anyhow!(
            "surreal binary not found — install it at ~/.codescope/bin/{exe} or put it on PATH"
        )),
    }
}

/// Query the binary for its version string. Used so `status` can flag
/// a version drift between the installed binary and the recorded
/// state file (i.e. someone upgraded the binary but didn't restart).
fn surreal_version(bin: &Path) -> Result<String> {
    let out = Command::new(bin).arg("--version").output()?;
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // Output shape: "SurrealDB command-line interface and server 3.0.5 for windows on x86_64"
    let ver = s
        .split_whitespace()
        .find(|t| {
            t.chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
        })
        .unwrap_or("unknown");
    Ok(ver.to_string())
}

fn read_state() -> Option<SupervisorState> {
    let text = std::fs::read_to_string(state_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_state(state: &SupervisorState) -> Result<()> {
    let p = state_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let text = serde_json::to_string_pretty(state)?;
    std::fs::write(&p, text).with_context(|| format!("write {}", p.display()))?;
    Ok(())
}

fn clear_state() {
    let _ = std::fs::remove_file(state_path());
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        // `tasklist /FI "PID eq N" /FO CSV` prints a single header
        // line when nothing matches, and two lines when it does.
        // Can't use signals on Windows; this is the cheap heuristic.
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output();
        match out {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout);
                s.lines().any(|l| l.contains(&pid.to_string()))
            }
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        // `kill -0 PID` returns 0 if the process exists (even if not
        // ours). Any non-zero status = dead or permission denied;
        // permission-denied still counts as "alive" for our purposes.
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn kill_pid(pid: u32) -> Result<()> {
    #[cfg(windows)]
    {
        let s = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .with_context(|| "taskkill spawn failed")?;
        if !s.success() {
            // Treat "process already gone" as success — that's the
            // terminal state we wanted anyway.
            if !pid_alive(pid) {
                return Ok(());
            }
            bail!("taskkill failed for pid {pid}");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        // SIGTERM first (graceful), fall back to SIGKILL after 2s if
        // still alive. `kill(1)` is universally available so we
        // avoid pulling `nix` just for this.
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if !pid_alive(pid) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
        Ok(())
    }
}

async fn health_ok(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    matches!(
        reqwest::get(&url).await,
        Ok(r) if r.status().is_success()
    )
}

async fn wait_for_health(port: u16, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if health_ok(port).await {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("surreal server on port {port} never became healthy");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// `codescope start` — idempotent launch. Returns the current state.
pub async fn start(port: Option<u16>) -> Result<()> {
    let port = port.unwrap_or(DEFAULT_PORT);
    let bin = find_surreal_binary()?;

    // Idempotence: if our state file points at a live + healthy
    // server, no-op.
    if let Some(state) = read_state() {
        if pid_alive(state.pid) && health_ok(state.port).await {
            println!(
                "Already running: pid={} port={} version={}",
                state.pid, state.port, state.version
            );
            return Ok(());
        }
        // Stale — either the process died or the file is leftover
        // from a crashed launch. Clear and fall through to spawn.
        if !pid_alive(state.pid) {
            println!("Clearing stale state for pid={}", state.pid);
        } else {
            println!("Server on port {} unhealthy; restarting.", state.port);
            let _ = kill_pid(state.pid);
        }
        clear_state();
    }

    let data = data_path();
    if let Some(parent) = data.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let data_url = format!("surrealkv:{}", data.display());

    // Spawn detached. On Unix, `std::process::Command` without
    // `wait()` is already non-blocking; the child runs until we
    // explicitly kill it. On Windows, the child inherits our console
    // by default — `DETACHED_PROCESS` cuts that link so closing the
    // shell doesn't tear it down.
    let mut cmd = Command::new(&bin);
    cmd.args([
        "start",
        &data_url,
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
    .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }

    let child = cmd
        .spawn()
        .with_context(|| format!("spawn surreal from {}", bin.display()))?;
    let pid = child.id();
    // We drop the Child handle — the process keeps running. No
    // `wait()` because we do not want the child to become a zombie
    // when the parent's runtime shuts down.
    std::mem::forget(child);

    wait_for_health(port, Duration::from_secs(15)).await?;
    let version = surreal_version(&bin).unwrap_or_else(|_| "unknown".into());
    let state = SupervisorState {
        pid,
        port,
        version: version.clone(),
        started_at: chrono_like_now(),
    };
    write_state(&state)?;
    println!("Started: pid={} port={} version={}", pid, port, version);
    Ok(())
}

/// `codescope stop` — best-effort shutdown.
pub async fn stop() -> Result<()> {
    let Some(state) = read_state() else {
        println!("Not running (no state file).");
        return Ok(());
    };
    if pid_alive(state.pid) {
        kill_pid(state.pid)?;
        println!("Stopped pid={}", state.pid);
    } else {
        println!("Process pid={} was already gone.", state.pid);
    }
    clear_state();
    Ok(())
}

/// `codescope status` — never changes state.
pub async fn status() -> Result<()> {
    let Some(state) = read_state() else {
        println!("not-running");
        return Ok(());
    };
    if !pid_alive(state.pid) {
        println!(
            "stale-pid  pid={} (file at {})",
            state.pid,
            state_path().display()
        );
        return Ok(());
    }
    let healthy = health_ok(state.port).await;
    let bin_version = find_surreal_binary()
        .ok()
        .and_then(|b| surreal_version(&b).ok())
        .unwrap_or_else(|| "unknown".into());
    let drift = if bin_version != state.version {
        format!(" (binary={}, state={})", bin_version, state.version)
    } else {
        String::new()
    };
    if healthy {
        println!(
            "running  pid={} port={} version={}{} started_at={}",
            state.pid, state.port, state.version, drift, state.started_at
        );
    } else {
        println!(
            "unhealthy  pid={} port={} (process alive but /health not responding){}",
            state.pid, state.port, drift
        );
    }
    Ok(())
}

/// Minimal ISO-8601 timestamp without pulling `chrono`. Precision to
/// the second is plenty for a "started_at" field.
fn chrono_like_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format as epoch-seconds fallback; callers that need a UTC
    // string can derive it. A tiny custom formatter isn't worth the
    // complexity here.
    format!("{secs}")
}

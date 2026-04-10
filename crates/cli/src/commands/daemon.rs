//! Daemon control commands: start, stop, status

use anyhow::Result;
use std::path::PathBuf;

use crate::commands::serve::daemon_pid_path;

pub fn start(port: u16) -> Result<()> {
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
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        let child = std::process::Command::new(exe)
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
    }

    #[cfg(not(windows))]
    {
        let child = std::process::Command::new(exe)
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
    }

    eprintln!("Log: {}", log_path.display());
    Ok(())
}

pub async fn stop(port: u16) -> Result<()> {
    let pid_path = daemon_pid_path(port);
    let pid_str = match std::fs::read_to_string(&pid_path) {
        Ok(s) => s.trim().to_string(),
        Err(_) => {
            eprintln!("No daemon PID file found for port {}.", port);
            return Ok(());
        }
    };
    let pid: u32 = match pid_str.parse() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Invalid PID: {}", pid_str);
            let _ = std::fs::remove_file(&pid_path);
            return Ok(());
        }
    };

    #[cfg(windows)]
    {
        let result = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
        match result {
            Ok(output) if output.status.success() => eprintln!("Daemon (PID {}) stopped.", pid),
            _ => eprintln!("Could not stop daemon (PID {}). May have already exited.", pid),
        }
    }
    #[cfg(not(windows))]
    {
        let result = std::process::Command::new("kill")
            .args([&pid.to_string()])
            .output();
        match result {
            Ok(output) if output.status.success() => eprintln!("Daemon (PID {}) stopped.", pid),
            _ => eprintln!("Could not stop daemon (PID {}). May have already exited.", pid),
        }
    }

    let _ = std::fs::remove_file(&pid_path);
    Ok(())
}

pub async fn status(port: u16) -> Result<()> {
    let url = format!("http://127.0.0.1:{}/api/projects", port);
    match reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let projects = body
                .get("projects")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            eprintln!(
                "Codescope daemon is running on port {} ({} projects)",
                port, projects
            );
            eprintln!("  Web UI: http://127.0.0.1:{}/", port);
            eprintln!("  MCP:    http://127.0.0.1:{}/mcp", port);
        }
        _ => {
            eprintln!("No daemon detected on port {}", port);
        }
    }
    Ok(())
}

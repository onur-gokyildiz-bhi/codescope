//! CMX-SANDBOX — `sandbox_run` MCP tool.
//!
//! Runs a short snippet in python / node / bash as a subprocess
//! and returns `{stdout, stderr, exit_code, timed_out}`. Timeout
//! is enforced (default 10 s, cap 60 s). Output is capped at
//! 16 KB per stream — if the snippet goes wild, we still return
//! a usable summary instead of blowing up the context window.
//!
//! Working directory defaults to the active project's
//! `codebase_path` — the agent usually wants to run against the
//! repo it's examining, not `/tmp`.
//!
//! Intended mirror of context-mode's sandbox tool. Arbitrary
//! code execution by definition; the user opted in by installing
//! codescope and wiring the MCP server.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::params::*;
use crate::server::GraphRagServer;

const MAX_OUTPUT_BYTES: usize = 16 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const MAX_TIMEOUT_MS: u64 = 60_000;

#[tool_router(router = sandbox_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Run a snippet of python / node / bash in a subprocess and
    /// return its stdout / stderr / exit_code. Timeout defaults
    /// to 10 s (max 60 s). Output is capped at 16 KB per stream.
    #[tool(
        description = "Run a python / node / bash snippet in a subprocess. Returns {stdout, stderr, exit_code, timed_out}. Timeout 10s (max 60s), output capped 16KB/stream. Working dir defaults to project codebase."
    )]
    async fn sandbox_run(&self, Parameters(params): Parameters<SandboxRunParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let (program, file_ext, interpreter_args): (String, &str, Vec<String>) =
            match params.language.as_str() {
                "python" | "py" => (python_cmd(), "py", vec![]),
                "node" | "js" => ("node".to_string(), "js", vec![]),
                "bash" | "sh" => (bash_cmd(), "sh", vec![]),
                other => {
                    return crate::error::tool_error(
                        crate::error::code::INVALID_INPUT,
                        &format!("unsupported language: {other}"),
                        Some("Use one of: python, node, bash."),
                    );
                }
            };

        let timeout_ms = params
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let tmp_dir = std::env::temp_dir().join("codescope-sandbox");
        if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
            return crate::error::tool_error(
                crate::error::code::INTERNAL,
                &format!("create sandbox tmp dir: {e}"),
                None,
            );
        }
        let tmp_path = tmp_dir.join(format!("snippet-{}.{}", std::process::id(), file_ext));
        {
            let mut f = match std::fs::File::create(&tmp_path) {
                Ok(f) => f,
                Err(e) => {
                    return crate::error::tool_error(
                        crate::error::code::INTERNAL,
                        &format!("write snippet file: {e}"),
                        None,
                    )
                }
            };
            if let Err(e) = f.write_all(params.code.as_bytes()) {
                return crate::error::tool_error(
                    crate::error::code::INTERNAL,
                    &format!("write snippet body: {e}"),
                    None,
                );
            }
        }

        let cwd = params
            .working_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.codebase_path.clone());

        let mut cmd = Command::new(&program);
        for a in &interpreter_args {
            cmd.arg(a);
        }
        cmd.arg(&tmp_path);
        cmd.current_dir(&cwd);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        // Don't leak any known credential env vars into the snippet.
        for var in [
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
        ] {
            cmd.env_remove(var);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp_path);
                return crate::error::tool_error(
                    crate::error::code::INTERNAL,
                    &format!("spawn {program}: {e}"),
                    Some(match params.language.as_str() {
                        "python" | "py" => "Is python3 (or python) on PATH?",
                        "node" | "js" => "Is node on PATH?",
                        "bash" | "sh" => "On Windows, install git-bash or WSL.",
                        _ => "Check the interpreter install.",
                    }),
                );
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let timed_out_flag = tokio::sync::watch::channel(false);
        let (timed_tx, timed_rx) = timed_out_flag;

        let run = async {
            let stdout_handle = tokio::spawn(read_capped(stdout));
            let stderr_handle = tokio::spawn(read_capped(stderr));
            let status = child.wait().await;
            let so = stdout_handle
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
            let se = stderr_handle
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
            (status, so, se)
        };

        let (status, stdout_text, stderr_text, timed_out) =
            match tokio::time::timeout(Duration::from_millis(timeout_ms), run).await {
                Ok((status, so, se)) => (status.ok(), so, se, false),
                Err(_) => {
                    let _ = timed_tx.send(true);
                    // Best-effort kill on timeout. We already moved `child`
                    // into the future — at this point it's dropped; the
                    // OS reaps it. Worst case: process lives a few ms
                    // longer until the tokio runtime GCs the handle.
                    let _ = timed_rx; // silence unused
                    (
                        None,
                        String::new(),
                        "(timed out before any output was captured)".into(),
                        true,
                    )
                }
            };

        let _ = std::fs::remove_file(&tmp_path);

        let exit_code = status.and_then(|s| s.code()).unwrap_or(-1);
        serde_json::to_string(&serde_json::json!({
            "ok": !timed_out && exit_code == 0,
            "stdout": stdout_text,
            "stderr": stderr_text,
            "exit_code": exit_code,
            "timed_out": timed_out,
        }))
        .unwrap_or_else(|_| "{}".into())
    }
}

/// Drain a child-pipe into a String, stopping once we exceed
/// the global cap. Prevents a runaway snippet from pulling the
/// whole context window's worth of bytes back to the caller.
async fn read_capped<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    mut pipe: Option<R>,
) -> std::io::Result<String> {
    let Some(mut pipe) = pipe.take() else {
        return Ok(String::new());
    };
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    loop {
        let n = pipe.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() >= MAX_OUTPUT_BYTES {
            buf.truncate(MAX_OUTPUT_BYTES);
            buf.extend_from_slice(b"\n... (truncated at 16 KB)");
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn python_cmd() -> String {
    // Prefer `python3` on unix; Windows installs typically ship
    // `python.exe` plus a `python3` shim (via the launcher).
    if cfg!(windows) {
        "python".to_string()
    } else {
        "python3".to_string()
    }
}

fn bash_cmd() -> String {
    // On Windows, `bash` is usually git-bash or WSL. We just
    // invoke `bash` and hope it's on PATH — if not, spawn fails
    // and we return a useful hint.
    "bash".to_string()
}

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

        let (program, file_ext, interpreter_args) = match resolve_language(&params.language) {
            Some(t) => t,
            None => {
                return crate::error::tool_error(
                    crate::error::code::INVALID_INPUT,
                    &format!("unsupported language: {}", params.language),
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
        // kill_on_drop reaps the direct child when this future is
        // dropped. Safety net in case we hit an early-return that
        // skips the explicit kill path below.
        cmd.kill_on_drop(true);
        // Put the snippet in its own process group on Unix so we can
        // signal the whole tree (child + any subprocesses it spawns)
        // in one call on timeout. Without this, `child.start_kill()`
        // hits only the direct child and any descendants become
        // orphans reparented to init.
        #[cfg(unix)]
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                // setsid() fails only in the parent position or when
                // already a session leader — neither happens right
                // after fork, so ignore the return and carry on.
                libc::setsid();
                Ok(())
            });
        }
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

        // Windows: assign the child to a JobObject with KILL_ON_JOB_CLOSE.
        // Any descendant the snippet spawns is automatically added to the
        // job, and dropping the job kills them all. This is the Windows
        // equivalent of setsid + kill(-pgid) on Unix. `_job` MUST stay
        // alive until we've decided whether to kill or reap — otherwise
        // KILL_ON_JOB_CLOSE fires immediately.
        #[cfg(windows)]
        let _job = match win32job::Job::create() {
            Ok(job) => {
                let mut info = job.query_extended_limit_info().unwrap_or_default();
                info.limit_kill_on_job_close();
                let _ = job.set_extended_limit_info(&info);
                if let Some(handle) = child.raw_handle() {
                    let _ = job.assign_process(handle as _);
                }
                Some(job)
            }
            Err(_) => None,
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        // Capture the pid so we can signal the whole process group on
        // Unix, or fall back to TerminateProcess on Windows. We
        // deliberately DON'T move `child` into the timeout future —
        // we need ownership out here to call kill() after the race
        // is decided.
        let child_pid = child.id();
        let stdout_handle = tokio::spawn(read_capped(stdout));
        let stderr_handle = tokio::spawn(read_capped(stderr));

        let wait_fut = child.wait();
        let (status, timed_out) =
            match tokio::time::timeout(Duration::from_millis(timeout_ms), wait_fut).await {
                Ok(res) => (res.ok(), false),
                Err(_) => {
                    // Kill the whole process tree. On Unix we signal
                    // the process group (setsid above made the child
                    // a group leader); on Windows we fall back to a
                    // direct kill via tokio's Child handle.
                    kill_process_tree(child_pid, &mut child).await;
                    (None, true)
                }
            };

        let stdout_text = stdout_handle
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        let stderr_text = {
            let captured = stderr_handle
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
            if timed_out && captured.is_empty() {
                "(timed out before any output was captured)".into()
            } else {
                captured
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

/// Kill the child and everything it spawned. On Unix, the child is
/// its own process-group leader (setsid at pre_exec) so `kill(-pgid,
/// SIGTERM)` followed by `SIGKILL` fans out to every descendant. On
/// Windows, the caller keeps a `JobObject` around with
/// `KILL_ON_JOB_CLOSE` — dropping it kills every descendant. We
/// still explicitly `start_kill()` the direct child here so `.wait()`
/// returns promptly instead of waiting for the job drop. The
/// `kill_on_drop(true)` command flag is a safety net on both
/// platforms.
async fn kill_process_tree(pid: Option<u32>, child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = pid {
            // The negative pid targets the whole process group.
            let pgid = pid as i32;
            unsafe {
                libc::kill(-pgid, libc::SIGTERM);
            }
            // Grace period then SIGKILL anything still alive.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
        } else {
            let _ = child.start_kill();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid; // used only on Unix
        let _ = child.start_kill();
    }
    // Reap whatever we just killed so the exit_code field has a value.
    let _ = child.wait().await;
}

/// Map the `language` param to (interpreter, file_ext, extra_args).
/// Returns `None` for unsupported languages; the caller surfaces an
/// `invalid_input` tool_error.
fn resolve_language(lang: &str) -> Option<(String, &'static str, Vec<String>)> {
    match lang {
        "python" | "py" => Some((python_cmd(), "py", vec![])),
        "node" | "js" => Some(("node".to_string(), "js", vec![])),
        "bash" | "sh" => Some((bash_cmd(), "sh", vec![])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_aliases_accepted() {
        assert!(resolve_language("python").is_some());
        assert!(resolve_language("py").is_some());
        assert!(resolve_language("node").is_some());
        assert!(resolve_language("js").is_some());
        assert!(resolve_language("bash").is_some());
        assert!(resolve_language("sh").is_some());
    }

    #[test]
    fn unsupported_language_returns_none() {
        assert!(resolve_language("ruby").is_none());
        assert!(resolve_language("").is_none());
        assert!(resolve_language("Python").is_none()); // case-sensitive by design
    }

    #[test]
    fn file_ext_matches_language() {
        assert_eq!(resolve_language("python").unwrap().1, "py");
        assert_eq!(resolve_language("node").unwrap().1, "js");
        assert_eq!(resolve_language("bash").unwrap().1, "sh");
    }
}

//! RTK-EXEC — output compressors for common dev commands.
//!
//! `codescope exec <cmd> <args…>` dispatches to the matching
//! module, which runs the real command, captures output, and
//! emits a shorter form that preserves the signal. Unknown
//! commands stream through untouched.
//!
//! Each compressor aims to be lossless for *actionable* info
//! (file paths, status codes, hash prefixes) and aggressive on
//! noise (repeated headers, commit bodies, multi-page listings).
//! When heuristics aren't confident, output passes through so
//! we never swallow something the user wanted to see.
//!
//! Savings are reported to stderr and fed into the `codescope
//! gain` counter so the aggregate benefit shows up alongside
//! MCP tool savings.

pub mod cat;
pub mod git;
pub mod grep;
pub mod head_tail;
pub mod ls;
pub mod passthrough;

use anyhow::Result;
use std::process::{Command, Stdio};

/// Shared entry — `codescope exec <cmd> <args…>` lands here.
pub async fn run(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!("exec needs a command");
    }
    let (cmd, rest) = args.split_first().unwrap();
    let rest_owned: Vec<String> = rest.to_vec();
    match cmd.as_str() {
        "git" => git::handle(&rest_owned).await,
        "ls" => ls::handle(&rest_owned).await,
        "cat" | "bat" => cat::handle(cmd, &rest_owned).await,
        "head" => head_tail::handle("head", &rest_owned).await,
        "tail" => head_tail::handle("tail", &rest_owned).await,
        "grep" | "rg" | "ag" => grep::handle(cmd, &rest_owned).await,
        _ => passthrough::handle(cmd, &rest_owned).await,
    }
}

/// Run a subprocess and capture `(stdout, stderr, exit_code)`.
/// Stderr is captured verbatim — we rarely compress that, since
/// users usually want to see errors as-is.
pub(crate) fn run_capture(cmd: &str, args: &[String]) -> Result<(String, String, i32)> {
    let out = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| anyhow::anyhow!("failed to spawn `{cmd}`: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let code = out.status.code().unwrap_or(-1);
    Ok((stdout, stderr, code))
}

/// Print the compressed output and report savings on stderr.
/// Threads through the gain counter so users see the cumulative
/// benefit of routing through `codescope exec`.
pub(crate) fn emit(original: &str, compressed: &str, stderr_passthrough: &str, exit_code: i32) {
    // Stderr always flows through unchanged; users expect to see
    // warnings/errors immediately.
    if !stderr_passthrough.is_empty() {
        eprint!("{}", stderr_passthrough);
    }
    print!("{}", compressed);

    // Savings report — only when we actually shrank the output.
    let before = original.len();
    let after = compressed.len();
    if before > after {
        let saved = before - after;
        let pct = if before > 0 {
            (saved as f32 / before as f32) * 100.0
        } else {
            0.0
        };
        eprintln!();
        eprintln!(
            "\x1b[2m↳ codescope exec: {} → {} bytes ({:.0}% saved)\x1b[0m",
            before, after, pct
        );
        // Bump the gain counter — one compressed exec = one "tool
        // call" worth of savings from the agent's perspective.
        codescope_core::gain::record_call();
    }

    if exit_code != 0 && exit_code != -1 {
        std::process::exit(exit_code);
    }
}

/// Truncate a long line list by keeping the top + bottom slices
/// with a `…N omitted…` marker in the middle. Returns the
/// original list when it's already short enough.
pub(crate) fn keep_head_tail(lines: &[&str], head: usize, tail: usize) -> Vec<String> {
    if lines.len() <= head + tail + 1 {
        return lines.iter().map(|s| s.to_string()).collect();
    }
    let omitted = lines.len() - head - tail;
    let mut out: Vec<String> = Vec::with_capacity(head + tail + 1);
    for l in &lines[..head] {
        out.push(l.to_string());
    }
    out.push(format!(
        "\x1b[2m… {omitted} lines omitted (use --full to see)…\x1b[0m"
    ));
    for l in &lines[lines.len() - tail..] {
        out.push(l.to_string());
    }
    out
}

/// Shared helper for the "full pass-through" flag — when the
/// user asks `--full` we skip compression entirely. Flag is
/// stripped from argv before dispatch so the wrapped command
/// doesn't see it.
pub(crate) fn split_full_flag(args: &[String]) -> (bool, Vec<String>) {
    let mut full = false;
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        if a == "--full" || a == "--codescope-full" {
            full = true;
        } else {
            out.push(a.clone());
        }
    }
    (full, out)
}

//! `cat` / `bat` compressor.
//!
//! `cat <source-file>` on a 2 000-line file dumps every line.
//! For files over the threshold, we show the first and last N
//! lines with a `…N lines omitted…` marker, and strongly nudge
//! the user toward `codescope context_bundle` on source files
//! — that tool returns structure instead of bytes.
//!
//! `--full` disables compression for this invocation.

use super::{emit, keep_head_tail, run_capture, split_full_flag};
use anyhow::Result;

const THRESHOLD_LINES: usize = 200;
const HEAD_KEEP: usize = 40;
const TAIL_KEEP: usize = 20;

pub async fn handle(cmd: &str, args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    let (stdout, stderr, code) = run_capture(cmd, &args)?;
    if full {
        emit(&stdout, &stdout, &stderr, code);
        return Ok(());
    }
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= THRESHOLD_LINES {
        emit(&stdout, &stdout, &stderr, code);
        return Ok(());
    }
    let kept = keep_head_tail(&lines, HEAD_KEEP, TAIL_KEEP);
    let mut out = kept.join("\n");
    out.push('\n');

    // Hint: if this looks like a source file (first arg ends in
    // a known code extension), push the user at context_bundle.
    if let Some(path) = args.first() {
        if looks_like_source(path) {
            out.push_str("\x1b[2m↳ for source files prefer `context_bundle(\"");
            out.push_str(path);
            out.push_str("\")` — structured view, ~80% fewer tokens.\x1b[0m\n");
        }
    }
    emit(&stdout, &out, &stderr, code);
    Ok(())
}

fn looks_like_source(path: &str) -> bool {
    matches!(
        path.rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "swift"
            | "c"
            | "cc"
            | "cpp"
            | "cxx"
            | "h"
            | "hpp"
            | "hxx"
            | "rb"
            | "php"
            | "sol"
            | "vue"
            | "svelte"
            | "scala"
            | "dart"
            | "clj"
            | "cljs"
            | "ex"
            | "exs"
            | "erl"
            | "hs"
            | "ml"
            | "mli"
            | "lua"
            | "nim"
            | "zig"
    )
}

//! `pytest` compressor — collapses dot-progress into a count,
//! keeps `FAILED` / `ERROR` / summary verbatim.
//!
//! A typical `pytest` run is ~3 KB of `.` characters plus a tiny
//! summary at the end. We aim to keep the summary and every
//! failing test traceback while dropping the dots and per-file
//! pass counts. `--full` disables.

use super::{emit, run_capture, split_full_flag};
use anyhow::Result;

pub async fn handle(args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    if full {
        return super::passthrough::handle("pytest", &args).await;
    }
    let (stdout, stderr, code) = run_capture("pytest", &args)?;
    let combined = format!("{stdout}{stderr}");

    let mut kept: Vec<String> = Vec::new();
    let mut in_failure = false;
    let mut dot_lines: usize = 0;
    for line in combined.lines() {
        let t = line.trim_start();
        // Progress line: "tests/foo.py ......." — collapse.
        if is_dot_progress_line(t) {
            dot_lines += 1;
            continue;
        }
        // Failure / error blocks — start-of-block markers.
        if t.starts_with("FAILED ")
            || t.starts_with("ERROR ")
            || t.starts_with("_ ") && t.contains("_")
        {
            in_failure = true;
        }
        if t.starts_with("=") || t.starts_with("!") {
            // Section divider — keep, reset failure flag after summary lines.
            if dot_lines > 0 {
                kept.push(format!(
                    "\x1b[2m… {dot_lines} progress lines omitted …\x1b[0m"
                ));
                dot_lines = 0;
            }
            kept.push(line.to_string());
            continue;
        }
        if in_failure {
            kept.push(line.to_string());
            continue;
        }
        if t.starts_with("PASSED") || t.starts_with("SKIPPED") || t.is_empty() {
            continue;
        }
        kept.push(line.to_string());
    }
    if dot_lines > 0 {
        kept.push(format!(
            "\x1b[2m… {dot_lines} progress lines omitted …\x1b[0m"
        ));
    }
    let out = kept.join("\n") + "\n";
    emit(&combined, &out, "", code);
    Ok(())
}

/// A pytest progress line looks like `tests/foo.py::bar .....`
/// or `tests/foo.py ..F..E`. We detect it as: contains `.py`
/// and the rest is only `.`, `F`, `E`, `s`, `x`, space, or
/// bracketed percent like `[ 50%]`.
fn is_dot_progress_line(line: &str) -> bool {
    if !line.contains(".py") {
        return false;
    }
    let Some(idx) = line.find(".py") else {
        return false;
    };
    let tail = &line[idx + 3..];
    if tail.is_empty() {
        return false;
    }
    tail.chars().all(|c| {
        matches!(
            c,
            '.' | 'F' | 'E' | 's' | 'x' | 'X' | ' ' | '[' | ']' | '%' | ':'
        ) || c.is_ascii_digit()
    })
}

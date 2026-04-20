//! `ls` compressor.
//!
//! Default `ls` on a deep node_modules or target/ prints
//! hundreds of entries. We run the real command, then if the
//! line count exceeds a threshold, keep head + tail + emit a
//! `…N omitted…` marker. `--full` disables compression.

use super::{emit, keep_head_tail, run_capture, split_full_flag};
use anyhow::Result;

const THRESHOLD: usize = 60;
const HEAD_KEEP: usize = 20;
const TAIL_KEEP: usize = 10;

pub async fn handle(args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    let (stdout, stderr, code) = run_capture("ls", &args)?;
    if full {
        emit(&stdout, &stdout, &stderr, code);
        return Ok(());
    }
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= THRESHOLD {
        emit(&stdout, &stdout, &stderr, code);
        return Ok(());
    }
    let kept = keep_head_tail(&lines, HEAD_KEEP, TAIL_KEEP);
    let mut out = kept.join("\n");
    out.push('\n');
    emit(&stdout, &out, &stderr, code);
    Ok(())
}

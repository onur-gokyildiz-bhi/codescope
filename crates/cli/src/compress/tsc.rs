//! `tsc` compressor — TypeScript compiler diagnostics.
//!
//! `tsc --noEmit` in a big monorepo can spit the same cross-file
//! import error once per consumer. We dedupe on the full
//! diagnostic line. Error count is preserved via a trailer.
//!
//! Non-error output (usually nothing — tsc is terse) passes through.

use super::{emit, run_capture, split_full_flag};
use anyhow::Result;
use std::collections::HashSet;

pub async fn handle(args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    if full {
        return super::passthrough::handle("tsc", &args).await;
    }
    let (stdout, stderr, code) = run_capture("tsc", &args)?;
    let combined = format!("{stdout}{stderr}");
    let mut seen: HashSet<String> = HashSet::new();
    let mut kept: Vec<String> = Vec::new();
    let mut dupes = 0usize;
    for line in combined.lines() {
        if line.trim().is_empty() {
            kept.push(String::new());
            continue;
        }
        // A TS diagnostic line looks like:
        //   src/foo.ts(10,5): error TS2322: Type 'X' is not assignable to type 'Y'.
        // The location prefix makes each occurrence unique; we key
        // on (error_code, message) to catch real cross-file dupes.
        let key = normalize_ts_line(line);
        if seen.insert(key) {
            kept.push(line.to_string());
        } else {
            dupes += 1;
        }
    }
    if dupes > 0 {
        kept.push(format!(
            "\x1b[2m… {dupes} duplicate TS diagnostics collapsed …\x1b[0m"
        ));
    }
    let out = kept.join("\n") + "\n";
    emit(&combined, &out, "", code);
    Ok(())
}

fn normalize_ts_line(line: &str) -> String {
    // Strip the leading `path(row,col):` so we dedupe same error
    // regardless of where it fires.
    if let Some(idx) = line.find("): ") {
        line[idx + 3..].to_string()
    } else {
        line.to_string()
    }
}

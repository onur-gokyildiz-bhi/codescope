//! `grep` / `rg` / `ag` compressor.
//!
//! When the match count is bounded, stream through. When it
//! blows past a threshold, collapse by file: show the first
//! match per file and the total count, followed by the unique
//! file list.
//!
//! Also nudge the user toward codescope's `search` /
//! `find_function` / `find_callers` when the pattern looks like
//! a symbol name — those tools answer the same question
//! structurally.

use super::{emit, run_capture, split_full_flag};
use anyhow::Result;
use std::collections::BTreeMap;

const THRESHOLD_LINES: usize = 80;

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

    // Group matches by file. Format we expect:
    //   path/to/file.rs:123:    matched line
    //   path/to/file.rs:456:    another match
    // When `--no-filename` or single-file grep we can't split
    // cleanly; fall back to head+tail.
    let mut per_file: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut parse_failed = false;
    for line in &lines {
        if let Some((path, rest)) = line.split_once(':') {
            if path.is_empty() || rest.is_empty() {
                parse_failed = true;
                break;
            }
            let entry = per_file
                .entry(path.to_string())
                .or_insert_with(|| ((*line).to_string(), 0));
            entry.1 += 1;
        } else {
            parse_failed = true;
            break;
        }
    }

    let mut out = String::new();
    if parse_failed {
        // head/tail instead
        use super::keep_head_tail;
        let kept = keep_head_tail(&lines, 30, 15);
        out = kept.join("\n");
        out.push('\n');
    } else {
        out.push_str(&format!(
            "\x1b[2m{} matches across {} files — showing first match per file\x1b[0m\n",
            lines.len(),
            per_file.len()
        ));
        for (path, (first, count)) in &per_file {
            if *count > 1 {
                out.push_str(&format!(
                    "{first}  \x1b[2m(+{} more in {path})\x1b[0m\n",
                    count - 1
                ));
            } else {
                out.push_str(first);
                out.push('\n');
            }
        }
    }

    // Nudge toward structured search for symbol-like patterns.
    if let Some(pattern) = extract_pattern(&args) {
        if looks_like_symbol(&pattern) {
            out.push_str(&format!(
                "\x1b[2m↳ `{}` looks like a symbol — `search(query=\"{}\", mode=fuzzy)` \
                 or `find_function(\"{}\")` returns structured results.\x1b[0m\n",
                pattern, pattern, pattern
            ));
        }
    }

    emit(&stdout, &out, &stderr, code);
    Ok(())
}

/// Pull the pattern arg out of a grep/rg/ag invocation. First
/// positional arg that doesn't start with `-` and isn't a path
/// flag value.
fn extract_pattern(args: &[String]) -> Option<String> {
    let mut skip_next = false;
    for a in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if a == "-e" || a == "--regexp" || a == "-f" || a == "--file" {
            skip_next = true;
            continue;
        }
        if !a.starts_with('-') {
            return Some(a.clone());
        }
    }
    None
}

fn looks_like_symbol(p: &str) -> bool {
    if p.len() < 3 || p.len() > 64 {
        return false;
    }
    p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && p.chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
}

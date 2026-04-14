//! `codescope review <target>` — diff-aware PR review with graph context.
//!
//! Parses a git diff (from a ref range, single commit, or .diff/.patch file),
//! maps changed line ranges to functions in the knowledge graph, and emits a
//! markdown impact report listing callers per changed function. With
//! `--coverage`, also flags functions with no obvious test references.

use anyhow::Result;
use codescope_core::graph::query::{GraphQuery, SearchResult};
use std::path::{Path, PathBuf};

use crate::db::connect_db;

pub async fn run(
    target: String,
    max_callers: usize,
    coverage: bool,
    db_path: Option<PathBuf>,
    repo: &str,
) -> Result<()> {
    // 1. Get diff content — from file if the target path exists, else shell out to git.
    let diff_content = load_diff(&target)?;

    // 2. Parse diff into per-file hunks.
    let changes = parse_diff(&diff_content);

    if changes.is_empty() {
        println!(
            "# Codescope Review: {}\n\n_No changes detected in diff._",
            target
        );
        return Ok(());
    }

    // 3. Connect to graph.
    let db = connect_db(db_path, repo).await?;
    let gq = GraphQuery::new(db.clone());

    // 4. Build report.
    let mut report = String::new();
    report.push_str(&format!("# Codescope Review: {}\n\n", target));
    report.push_str(&format!("Repo: `{}`\n\n", repo));
    report.push_str("## Changes\n\n");

    let mut total_functions_hit = 0usize;

    for change in &changes {
        report.push_str(&format!(
            "### `{}` ({} hunk{})\n\n",
            change.file,
            change.hunks.len(),
            if change.hunks.len() == 1 { "" } else { "s" }
        ));

        // Pull all functions in this file, ordered by start_line.
        let funcs: Vec<SearchResult> = db
            .query(
                "SELECT qualified_name, name, file_path, start_line, end_line, language, signature \
                 FROM `function` WHERE file_path = $path ORDER BY start_line",
            )
            .bind(("path", change.file.clone()))
            .await?
            .take(0)
            .unwrap_or_default();

        let mut any_hit = false;
        for func in &funcs {
            let fname = func.name.as_deref().unwrap_or("?");
            let fstart = func.start_line.unwrap_or(0);
            let fend = func.end_line.unwrap_or(fstart);

            // Check if any hunk overlaps this function's line range.
            let hit = change.hunks.iter().any(|h| {
                let hstart = h.new_start;
                let hend = hstart.saturating_add(h.new_lines.saturating_sub(1));
                !(hend < fstart || hstart > fend)
            });
            if !hit {
                continue;
            }
            any_hit = true;
            total_functions_hit += 1;

            report.push_str(&format!("- **{}** (L{}–{})\n", fname, fstart, fend));

            // Callers via the graph.
            match gq.find_callers(fname).await {
                Ok(callers) if !callers.is_empty() => {
                    report.push_str(&format!("  - {} caller(s):\n", callers.len()));
                    for c in callers.iter().take(max_callers) {
                        report.push_str(&format!(
                            "    - `{}` in {}:{}\n",
                            c.name.as_deref().unwrap_or("?"),
                            c.file_path.as_deref().unwrap_or("?"),
                            c.start_line.unwrap_or(0),
                        ));
                    }
                    if callers.len() > max_callers {
                        report.push_str(&format!(
                            "    - …and {} more\n",
                            callers.len() - max_callers
                        ));
                    }
                }
                Ok(_) => {
                    report.push_str("  - No callers found (may be unused or entry point)\n");
                }
                Err(e) => {
                    report.push_str(&format!("  - _Caller lookup failed: {}_\n", e));
                }
            }

            // Coverage: look for functions in test/spec files whose names reference this one.
            if coverage {
                let tests: Vec<serde_json::Value> = db
                    .query(
                        "SELECT file_path FROM `function` \
                         WHERE (string::contains(file_path, 'test') \
                                OR string::contains(file_path, 'spec')) \
                           AND string::contains(name, $name) \
                         LIMIT 5",
                    )
                    .bind(("name", fname.to_string()))
                    .await?
                    .take(0)
                    .unwrap_or_default();
                if tests.is_empty() {
                    report.push_str(&format!("  - ⚠ No test file references `{}`\n", fname));
                }
            }
        }

        if !any_hit {
            report.push_str("_No indexed functions overlap these hunks._\n");
        }
        report.push('\n');
    }

    // 5. Summary.
    let total_hunks: usize = changes.iter().map(|c| c.hunks.len()).sum();
    report.push_str("## Summary\n");
    report.push_str(&format!("- Files changed: {}\n", changes.len()));
    report.push_str(&format!("- Total hunks: {}\n", total_hunks));
    report.push_str(&format!("- Functions touched: {}\n", total_functions_hit));

    println!("{}", report);
    Ok(())
}

/// Load diff content from either a .diff/.patch file on disk or `git diff <target>`.
fn load_diff(target: &str) -> Result<String> {
    if Path::new(target).exists() {
        Ok(std::fs::read_to_string(target)?)
    } else {
        let output = std::process::Command::new("git")
            .args(["diff", target])
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git diff failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

// -------- Diff parsing ------------------------------------------------------

#[derive(Debug)]
struct FileChange {
    file: String,
    hunks: Vec<Hunk>,
}

#[derive(Debug)]
struct Hunk {
    #[allow(dead_code)]
    old_start: u32,
    #[allow(dead_code)]
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
}

/// Parse a unified diff. Handles `+++ b/<path>`, `+++ /dev/null` (deletion),
/// and hunk headers of the form `@@ -a,b +c,d @@ optional-context`.
fn parse_diff(content: &str) -> Vec<FileChange> {
    let mut changes: Vec<FileChange> = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_hunks: Vec<Hunk> = Vec::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            // Flush previous file.
            if let Some(file) = current_file.take() {
                changes.push(FileChange {
                    file,
                    hunks: std::mem::take(&mut current_hunks),
                });
            }
            // `+++ b/foo.rs` is the common case; `+++ /dev/null` = deleted file (skip).
            let path = rest
                .strip_prefix("b/")
                .or_else(|| rest.strip_prefix("a/"))
                .unwrap_or(rest);
            if path == "/dev/null" {
                current_file = None;
            } else {
                current_file = Some(path.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("@@ ") {
            if current_file.is_some() {
                if let Some(hunk) = parse_hunk_header(rest) {
                    current_hunks.push(hunk);
                }
            }
        }
    }
    if let Some(file) = current_file {
        changes.push(FileChange {
            file,
            hunks: current_hunks,
        });
    }
    changes
}

/// Parse `-a,b +c,d @@ …` (the leading `@@ ` has already been stripped).
fn parse_hunk_header(s: &str) -> Option<Hunk> {
    let parts: Vec<&str> = s.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }
    let old = parts[0].strip_prefix('-')?;
    let new = parts[1].strip_prefix('+')?;
    let (old_start, old_lines) = parse_range(old);
    let (new_start, new_lines) = parse_range(new);
    Some(Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
    })
}

/// Parse `N` or `N,M` → (start, count). Missing count defaults to 1 per unified-diff spec.
fn parse_range(s: &str) -> (u32, u32) {
    let mut it = s.split(',');
    let start = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let lines = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    (start, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_diff() {
        let diff = "\
diff --git a/src/foo.rs b/src/foo.rs
index abc..def 100644
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -10,3 +12,4 @@ fn context
 unchanged
-old
+new
+added
@@ -40,0 +50,2 @@
+line1
+line2
";
        let changes = parse_diff(diff);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].file, "src/foo.rs");
        assert_eq!(changes[0].hunks.len(), 2);
        assert_eq!(changes[0].hunks[0].new_start, 12);
        assert_eq!(changes[0].hunks[0].new_lines, 4);
        assert_eq!(changes[0].hunks[1].new_start, 50);
        assert_eq!(changes[0].hunks[1].new_lines, 2);
    }

    #[test]
    fn parse_multi_file_diff() {
        let diff = "\
--- a/a.rs
+++ b/a.rs
@@ -1 +1,2 @@
 x
+y
--- a/b.rs
+++ b/b.rs
@@ -5,2 +5,3 @@
 a
+b
 c
";
        let changes = parse_diff(diff);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].file, "a.rs");
        assert_eq!(changes[1].file, "b.rs");
    }

    #[test]
    fn parse_deletion_skipped() {
        let diff = "\
--- a/gone.rs
+++ /dev/null
@@ -1,3 +0,0 @@
-a
-b
-c
";
        let changes = parse_diff(diff);
        assert!(changes.is_empty());
    }

    #[test]
    fn parse_range_defaults_to_one() {
        assert_eq!(parse_range("10"), (10, 1));
        assert_eq!(parse_range("10,5"), (10, 5));
        assert_eq!(parse_range(""), (0, 1));
    }
}

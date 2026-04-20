//! `npm` / `pnpm` / `yarn` compressor.
//!
//! Package-manager output is noisy: `npm install` spews deprecation
//! warnings, per-package optional-dep skips, and funding banners
//! that dwarf the one line people actually read ("added N packages").
//! We keep the summary + vulnerability report, drop the chatter.
//!
//! Run scripts (`npm run <x>`) pass through untouched — we can't
//! know the inner command's output format.

use super::{emit, run_capture, split_full_flag};
use anyhow::Result;

pub async fn handle(cmd: &str, args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    if full {
        return super::passthrough::handle(cmd, &args).await;
    }
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "install" | "i" | "ci" | "add" | "update" => install(cmd, &args).await,
        _ => super::passthrough::handle(cmd, &args).await,
    }
}

async fn install(cmd: &str, args: &[String]) -> Result<()> {
    let (stdout, stderr, code) = run_capture(cmd, args)?;
    let combined = format!("{stderr}{stdout}");
    let mut kept: Vec<String> = Vec::new();
    let mut dep_warnings = 0usize;
    let mut funding = 0usize;
    for line in combined.lines() {
        let t = line.trim_start();
        if t.starts_with("npm warn deprecated ")
            || t.starts_with("warning ")
            || t.starts_with("WARN deprecated ")
        {
            dep_warnings += 1;
            continue;
        }
        if t.contains("looking for funding") || t.starts_with("npm fund") {
            funding += 1;
            continue;
        }
        if t.starts_with("npm notice")
            || t.starts_with("npm http ")
            || t.starts_with("npm sill ")
            || t.starts_with("npm verb ")
        {
            continue;
        }
        kept.push(line.to_string());
    }
    if dep_warnings > 0 {
        kept.push(format!(
            "\x1b[2m… {dep_warnings} deprecation warnings omitted (use --full) …\x1b[0m"
        ));
    }
    if funding > 0 {
        kept.push(format!("\x1b[2m… {funding} funding lines omitted …\x1b[0m"));
    }
    let out = kept.join("\n") + "\n";
    emit(&combined, &out, "", code);
    Ok(())
}

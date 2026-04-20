//! `cargo` compressor — build / test / clippy / check.
//!
//! Cargo's default output is ~80% "Compiling foo v0.1.0" noise
//! that no human reads. The signal lives in: warnings, errors,
//! and the final `Finished` / test summary line. We keep exactly
//! those and collapse everything else into a single count line.
//!
//! `cargo test` gets its own pass — dot progress and `test foo
//! … ok` lines collapse to counts; failures stream through in
//! full so the actionable panic output is never lost.
//!
//! `--full` skips compression entirely.

use super::{emit, run_capture, split_full_flag};
use anyhow::Result;

pub async fn handle(args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    if full {
        return super::passthrough::handle("cargo", &args).await;
    }
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "test" | "t" | "nextest" => cargo_test(&args).await,
        "build" | "b" | "check" | "c" | "clippy" | "run" | "r" => cargo_build(&args).await,
        _ => super::passthrough::handle("cargo", &args).await,
    }
}

async fn cargo_build(args: &[String]) -> Result<()> {
    let (stdout, stderr, code) = run_capture("cargo", args)?;
    let combined = format!("{stderr}{stdout}");
    let mut kept: Vec<String> = Vec::new();
    let mut compiling_count: usize = 0;
    let mut in_error_block = false;
    for line in combined.lines() {
        let t = line.trim_start();
        if t.starts_with("Compiling ")
            || t.starts_with("Checking ")
            || t.starts_with("Downloading ")
            || t.starts_with("Downloaded ")
        {
            compiling_count += 1;
            continue;
        }
        if t.starts_with("warning:") || t.starts_with("error") {
            if compiling_count > 0 {
                kept.push(format!(
                    "\x1b[2m… {compiling_count} compile/fetch lines omitted …\x1b[0m"
                ));
                compiling_count = 0;
            }
            in_error_block = true;
            kept.push(line.to_string());
            continue;
        }
        if in_error_block {
            // Keep the diagnostic block (indented lines, --> src/..., = note:, etc.)
            if line.is_empty()
                || line.starts_with(' ')
                || line.starts_with('\t')
                || t.starts_with("= ")
                || t.starts_with("--> ")
            {
                kept.push(line.to_string());
                continue;
            }
            in_error_block = false;
        }
        if t.starts_with("Finished ") || t.starts_with("Running ") || t.starts_with("error:") {
            if compiling_count > 0 {
                kept.push(format!(
                    "\x1b[2m… {compiling_count} compile/fetch lines omitted …\x1b[0m"
                ));
                compiling_count = 0;
            }
            kept.push(line.to_string());
            continue;
        }
        // Anything else — small noise (blank lines, cargo progress bars).
        // Drop silently; they were not useful signal.
    }
    if compiling_count > 0 {
        kept.push(format!(
            "\x1b[2m… {compiling_count} compile/fetch lines omitted …\x1b[0m"
        ));
    }
    let out = kept.join("\n") + "\n";
    emit(&combined, &out, "", code);
    Ok(())
}

async fn cargo_test(args: &[String]) -> Result<()> {
    let (stdout, stderr, code) = run_capture("cargo", args)?;
    let combined = format!("{stderr}{stdout}");
    let mut kept: Vec<String> = Vec::new();
    let mut compiling_count: usize = 0;
    let mut passing_count: usize = 0;
    let mut in_failure_dump = false;
    for line in combined.lines() {
        let t = line.trim_start();
        // Compile noise — same as build.
        if t.starts_with("Compiling ")
            || t.starts_with("Checking ")
            || t.starts_with("Downloading ")
            || t.starts_with("Downloaded ")
        {
            compiling_count += 1;
            continue;
        }
        // `test foo::bar ... ok` — collapse.
        if t.starts_with("test ") && (t.ends_with(" ok") || t.ends_with(" ignored")) {
            passing_count += 1;
            continue;
        }
        // `test foo::bar ... FAILED` — keep verbatim and start listening
        // for the failure block that follows.
        if t.starts_with("test ") && t.contains(" FAILED") {
            flush_compile(&mut kept, &mut compiling_count);
            flush_passes(&mut kept, &mut passing_count);
            kept.push(line.to_string());
            continue;
        }
        // Entering `failures:` dump section at the bottom — keep all of it.
        if t.starts_with("failures:") || t.starts_with("---- ") {
            in_failure_dump = true;
        }
        if in_failure_dump {
            kept.push(line.to_string());
            continue;
        }
        // Summary / framing lines — keep.
        if t.starts_with("running ")
            || t.starts_with("test result:")
            || t.starts_with("Finished ")
            || t.starts_with("Running ")
            || t.starts_with("Doc-tests")
            || t.starts_with("warning:")
            || t.starts_with("error")
        {
            flush_compile(&mut kept, &mut compiling_count);
            flush_passes(&mut kept, &mut passing_count);
            kept.push(line.to_string());
        }
    }
    flush_compile(&mut kept, &mut compiling_count);
    flush_passes(&mut kept, &mut passing_count);
    let out = kept.join("\n") + "\n";
    emit(&combined, &out, "", code);
    Ok(())
}

fn flush_compile(kept: &mut Vec<String>, n: &mut usize) {
    if *n > 0 {
        kept.push(format!("\x1b[2m… {n} compile/fetch lines omitted …\x1b[0m"));
        *n = 0;
    }
}

fn flush_passes(kept: &mut Vec<String>, n: &mut usize) {
    if *n > 0 {
        kept.push(format!(
            "\x1b[2m… {n} passing tests omitted (use --full to see) …\x1b[0m"
        ));
        *n = 0;
    }
}

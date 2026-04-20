//! `git` compressor — status / log / diff.
//!
//! `git status` is already half-compressed in `-s` form, but
//! unfamiliar readers rarely pass that flag. We detect the
//! default porcelain output and rewrite it to short form,
//! collapsing long untracked-files sections into a count.
//!
//! `git log` defaults to full commit bodies. We force `--oneline
//! --abbrev-commit --n=20` unless `--full` is passed. The full
//! context is one flag away.
//!
//! `git diff` defaults to full line-by-line diff. We force
//! `--stat` summary; `--full` opts back into the raw diff.

use super::{emit, run_capture, split_full_flag};
use anyhow::Result;

pub async fn handle(args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("");
    match subcmd {
        "status" => status(&args, full).await,
        "log" => log(&args, full).await,
        "diff" => diff(&args, full).await,
        _ => super::passthrough::handle("git", &args).await,
    }
}

async fn status(args: &[String], full: bool) -> Result<()> {
    if full {
        return super::passthrough::handle("git", args).await;
    }
    // Already short? Pass through. Otherwise inject `-s --branch`.
    let has_short = args.iter().any(|a| a == "-s" || a == "--short");
    let mut invoke: Vec<String> = args.to_vec();
    if !has_short {
        invoke.insert(1, "-s".into());
        invoke.insert(2, "--branch".into());
    }
    let (stdout, stderr, code) = run_capture("git", &invoke)?;
    // If the user's repo has a huge untracked tree (node_modules
    // etc.), the `?? ` lines balloon the output. Collapse when
    // they dominate.
    let lines: Vec<&str> = stdout.lines().collect();
    let untracked: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| l.starts_with("??"))
        .collect();
    let tracked: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.starts_with("??"))
        .collect();
    let mut out = String::new();
    for l in &tracked {
        out.push_str(l);
        out.push('\n');
    }
    if untracked.len() > 8 {
        out.push_str(&format!(
            "?? \x1b[2m({} untracked files omitted — `git status -u` for list)\x1b[0m\n",
            untracked.len()
        ));
    } else {
        for l in &untracked {
            out.push_str(l);
            out.push('\n');
        }
    }
    emit(&stdout, &out, &stderr, code);
    Ok(())
}

async fn log(args: &[String], full: bool) -> Result<()> {
    if full {
        return super::passthrough::handle("git", args).await;
    }
    // Inject `--oneline --abbrev-commit -n 20` unless the user
    // already specified a count or a format. `--pretty` / `-n`
    // presence means they know what they want.
    let has_format = args.iter().any(|a| {
        a == "--oneline"
            || a.starts_with("--pretty")
            || a.starts_with("--format")
            || a == "-p"
            || a == "--patch"
    });
    let has_count = args
        .iter()
        .any(|a| a == "-n" || a.starts_with("--max-count"));
    let mut invoke: Vec<String> = args.to_vec();
    if !has_format {
        invoke.push("--oneline".into());
        invoke.push("--abbrev-commit".into());
    }
    if !has_count {
        invoke.push("-n".into());
        invoke.push("20".into());
    }
    let (stdout, stderr, code) = run_capture("git", &invoke)?;
    emit(&stdout, &stdout, &stderr, code);
    Ok(())
}

async fn diff(args: &[String], full: bool) -> Result<()> {
    if full {
        return super::passthrough::handle("git", args).await;
    }
    let has_stat = args.iter().any(|a| {
        a == "--stat"
            || a == "--shortstat"
            || a == "--numstat"
            || a == "--name-only"
            || a == "--name-status"
    });
    let mut invoke: Vec<String> = args.to_vec();
    if !has_stat {
        invoke.push("--stat".into());
    }
    let (stdout, stderr, code) = run_capture("git", &invoke)?;
    // Hint the full flag at the bottom so users know how to dig deeper.
    let hint = if has_stat {
        String::new()
    } else {
        "\n\x1b[2m↳ --full to see the actual diff\x1b[0m\n".to_string()
    };
    let out = format!("{stdout}{hint}");
    emit(&stdout, &out, &stderr, code);
    Ok(())
}

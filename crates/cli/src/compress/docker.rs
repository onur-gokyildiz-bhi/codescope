//! `docker` compressor — `build` / `ps` / `images`.
//!
//! `docker build` output is dominated by layer caching lines
//! and progress bars. We keep step headers (`Step N/M :`, `#N
//! [internal]`) and the final `Successfully built …` / tag
//! lines, and collapse the middle.
//!
//! `docker ps` / `docker images` already emit a tabular form;
//! we truncate only when the listing exceeds 40 rows.

use super::{emit, keep_head_tail, run_capture, split_full_flag};
use anyhow::Result;

pub async fn handle(args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    if full {
        return super::passthrough::handle("docker", &args).await;
    }
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "build" | "buildx" => build(&args).await,
        "ps" | "images" | "container" | "image" => list(&args).await,
        _ => super::passthrough::handle("docker", &args).await,
    }
}

async fn build(args: &[String]) -> Result<()> {
    let (stdout, stderr, code) = run_capture("docker", args)?;
    let combined = format!("{stderr}{stdout}");
    let mut kept: Vec<String> = Vec::new();
    let mut noise = 0usize;
    for line in combined.lines() {
        let t = line.trim_start();
        // Keep step headers + final status.
        if t.starts_with("Step ")
            || t.starts_with("#")
            || t.starts_with("Successfully built")
            || t.starts_with("Successfully tagged")
            || t.starts_with("ERROR ")
            || t.starts_with("error:")
            || t.starts_with("Sending build context")
        {
            kept.push(line.to_string());
            continue;
        }
        // Layer-progress lines like " ---> abc123" — drop but count.
        if t.starts_with("--->") || t.starts_with("---") {
            noise += 1;
            continue;
        }
        // Random progress bytes (":: DONE", "CACHED", etc.) — keep terminal lines.
        if t.contains("DONE") || t.contains("CACHED") || t.starts_with("exporting ") {
            kept.push(line.to_string());
            continue;
        }
        noise += 1;
    }
    if noise > 0 {
        kept.push(format!(
            "\x1b[2m… {noise} layer-progress lines omitted …\x1b[0m"
        ));
    }
    let out = kept.join("\n") + "\n";
    emit(&combined, &out, "", code);
    Ok(())
}

async fn list(args: &[String]) -> Result<()> {
    let (stdout, stderr, code) = run_capture("docker", args)?;
    let lines: Vec<&str> = stdout.lines().collect();
    let out = if lines.len() > 40 {
        keep_head_tail(&lines, 20, 10).join("\n") + "\n"
    } else {
        stdout.clone()
    };
    emit(&stdout, &out, &stderr, code);
    Ok(())
}

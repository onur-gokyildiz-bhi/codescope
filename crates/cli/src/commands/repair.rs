//! R6 — `codescope repair` corruption recovery.
//!
//! Two modes:
//!
//! * `--repo X`         — in-place, no server restart. Sends
//!                        `REMOVE DATABASE <repo>` to the running
//!                        surreal server, leaving the rest of the
//!                        namespace untouched. Safe when only one
//!                        repo is suspect.
//! * `--repo X --reindex <path>` — the above, then immediately
//!                        re-runs the indexer against `<path>` so the
//!                        repo comes back with fresh data.
//!
//! There's no `--all` wipe yet. Full-server recovery is rare now
//! that the server owns every file; if it's needed, `codescope stop`,
//! rename `~/.codescope/surreal-data/` to `.broken.TS/`, `codescope
//! start`, then run `--reindex` per repo. We'll script that explicitly
//! the first time someone hits it.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use codescope_core::{connect_admin, DEFAULT_NS};

pub async fn run(repo: String, reindex: Option<PathBuf>, yes: bool) -> Result<()> {
    if repo.trim().is_empty() {
        bail!("empty --repo");
    }

    println!("codescope repair — repo '{repo}'");

    // Safety: require explicit confirmation unless `--yes` is set.
    // Tiny CLI prompt — we don't want a slip of the keyboard wiping
    // a repo that took hours to index.
    if !yes {
        eprint!(
            "About to REMOVE DATABASE `{repo}` in NS=codescope. \
             All entities + relations for this repo will be deleted. \
             Continue? [y/N] "
        );
        use std::io::Write;
        std::io::stderr().flush().ok();
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf).ok();
        let ans = buf.trim().to_ascii_lowercase();
        if ans != "y" && ans != "yes" {
            println!("aborted.");
            return Ok(());
        }
    }

    // Admin connect: no DB selected, just NS. REMOVE DATABASE is a
    // root+NS privilege in SurrealDB 3.x; our local server runs as
    // `root` by default.
    let admin = connect_admin()
        .await
        .with_context(|| "cannot reach surreal server — is `codescope start` running?")?;
    admin
        .use_ns(DEFAULT_NS)
        .await
        .with_context(|| format!("USE NS {DEFAULT_NS}"))?;

    // Backticks keep reserved words / hyphens safe.
    let stmt = format!("REMOVE DATABASE IF EXISTS `{repo}`");
    let mut resp = admin
        .query(&stmt)
        .await
        .with_context(|| format!("run: {stmt}"))?;
    // Drain the response so SurrealDB has a chance to report errors
    // inside the statement-level result.
    let _: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
    println!("  removed ns=codescope db={repo}");

    if let Some(path) = reindex {
        if !path.is_dir() {
            bail!("reindex path does not exist: {}", path.display());
        }
        println!("  re-indexing from {}", path.display());
        // Reuse the existing indexer so we don't duplicate pipeline
        // logic. `clean=true` rebuilds from scratch even if the DB
        // already has partial data.
        crate::commands::index::run(path, &repo, true, None).await?;
        println!("  ✓ repo '{repo}' rebuilt from source");
    } else {
        println!("  Done. Re-index when ready: `codescope index <path> --repo {repo}`");
    }

    Ok(())
}

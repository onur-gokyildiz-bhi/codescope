//! `codescope migrate-to-server` — move legacy per-repo SurrealKV
//! directories (`~/.codescope/db/<repo>/`) into the unified `surreal`
//! server under `NS=codescope, DB=<repo>`.
//!
//! ## Approach (2026-04-19 pivot)
//!
//! The previous attempt hand-rolled row copy via the embedded SurrealDB
//! client — it foundered on id-format parsing, relation-vs-CREATE
//! semantics, and surrealkv file locks that outlived the client drop.
//!
//! The new path shells out to the blessed `surreal export` /
//! `surreal import` pair, which already handles every shape edge case
//! (Thing ids, RELATE vs CREATE, indexes, access methods, analyzers).
//!
//! Per repo:
//!
//! 1. Spawn a short-lived `surreal start` server on a free alt port,
//!    backed by `surrealkv:<src>`.
//! 2. Poll `/health` until 200 OK (~10 s cap).
//! 3. Run `surreal export --endpoint http://127.0.0.1:<port> --ns codescope
//!    --db <repo> <tmpfile>`.
//! 4. Run `surreal import --endpoint <main> --ns codescope --db <repo>
//!    <tmpfile>`.
//! 5. Kill the temp server, delete tmpfile, rename `<src>` → `<src>.old.<ts>`.
//!
//! Dry-run keeps the existing embedded-client row counter: it doesn't
//! write anything and avoids the cost of spawning per-repo temp servers.
//!
//! ## Safety
//!
//! * Each repo's temp server is bound to 127.0.0.1 only.
//! * Temp server is killed on both success and error paths
//!   (best-effort — Windows `kill()` is terminate, not graceful, but
//!   the data is already in the target).
//! * If export or import fails we leave the legacy dir intact.
//! * Re-running is idempotent: `surreal import` is CREATE-based; on
//!   duplicate ids Surreal logs and continues — the backup rename on
//!   success makes the second run skip the repo anyway.

use anyhow::{anyhow, bail, Context, Result};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command as TokioCommand};

/// Entity tables (normal records). Used only by the dry-run row counter.
const ENTITY_TABLES: &[&str] = &[
    "file",
    "module",
    "function",
    "class",
    "variable",
    "import_decl",
    "config",
    "doc",
    "api",
    "http_call",
    "skill",
    "db_entity",
    "infra",
    "package",
    "conversation",
    "conv_topic",
    "decision",
    "problem",
    "solution",
    "knowledge",
    "meta",
];

/// Relation tables. Used only by the dry-run row counter.
const RELATION_TABLES: &[&str] = &[
    "contains",
    "calls",
    "imports",
    "inherits",
    "implements",
    "uses",
    "modified_in",
    "depends_on",
    "configures",
    "defines_endpoint",
    "has_field",
    "references",
    "depends_on_package",
    "runs_script",
    "calls_endpoint",
    "links_to",
    "discussed_in",
    "decided_about",
    "solves_for",
    "co_discusses",
    "supports",
    "contradicts",
    "related_to",
];

/// Main server endpoint read from `CODESCOPE_DB_URL` if set, else the
/// default local dev address.
fn target_endpoint() -> String {
    std::env::var("CODESCOPE_DB_URL")
        .ok()
        .and_then(|u| normalize_http(&u))
        .unwrap_or_else(|| "http://127.0.0.1:8077".to_string())
}

/// `surreal export/import` want `http://`; callers commonly paste
/// `ws://` since that's what the client crate expects. Accept either.
fn normalize_http(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("ws://") {
        Some(format!("http://{rest}"))
    } else if let Some(rest) = url.strip_prefix("wss://") {
        Some(format!("https://{rest}"))
    } else if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

pub async fn run(repo: Option<String>, execute: bool, delete_backup: bool) -> Result<()> {
    let db_root = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db");

    if !db_root.is_dir() {
        println!(
            "No legacy DB root at {} — nothing to migrate.",
            db_root.display()
        );
        return Ok(());
    }

    let repos: Vec<String> = match repo {
        Some(r) => vec![r],
        None => discover_legacy_repos(&db_root),
    };

    // Guardrail: no matter how `repo` was provided, reject any name
    // that looks like a migration backup (`*.old.*`) or an internal
    // namespace (`_global` / `_system`). Without this, an explicit
    // `--repo <name>.old.<ts>` can quietly create empty DBs on the
    // surreal server (we call `connect_repo` somewhere in the
    // chain), and the next daemon start picks them up as if they
    // were real projects. Filter at the outermost boundary.
    let repos: Vec<String> = repos
        .into_iter()
        .filter(|n| !n.contains(".old.") && !n.ends_with(".old") && !n.starts_with('_'))
        .collect();
    if repos.is_empty() {
        println!("No eligible repos after filtering backups / internal namespaces.");
        return Ok(());
    }

    if repos.is_empty() {
        println!("No repos found under {}.", db_root.display());
        return Ok(());
    }

    let endpoint = target_endpoint();
    let mode = if execute { "EXECUTE" } else { "DRY RUN" };
    println!("codescope migrate-to-server — mode: {mode}");
    println!("  target: {endpoint}");
    println!("  repos:  {} ({})", repos.len(), repos.join(", "));
    println!();

    if execute {
        let bin = find_surreal_binary()
            .context("cannot locate `surreal` binary — place it at ~/.codescope/bin/surreal[.exe] or on PATH")?;
        println!("  surreal binary: {}", bin.display());
        // Sanity-check the target is reachable before we start spawning
        // temp servers and shuffling data.
        probe_target(&endpoint).await.with_context(|| {
            format!("target server at {endpoint} is not reachable — start it first")
        })?;
        println!();
    }

    let mut ok = 0usize;
    let mut failed = Vec::<(String, String)>::new();

    for repo in &repos {
        let src = db_root.join(repo);
        if !src.is_dir() {
            println!("  {repo}: skip — not a directory at {}", src.display());
            continue;
        }
        println!("[{repo}] source: {}", src.display());
        if !execute {
            let sizes = count_source_rows(&src, repo).await;
            match sizes {
                Ok(summary) => {
                    println!("  planned: {summary}");
                }
                Err(e) => {
                    println!("  ! source probe failed: {e:#}");
                    failed.push((repo.clone(), e.to_string()));
                }
            }
            continue;
        }

        match migrate_one(&src, repo, &endpoint, delete_backup).await {
            Ok(summary) => {
                println!("  ✓ migrated: {summary}");
                ok += 1;
            }
            Err(e) => {
                println!("  ✗ failed: {e:#}");
                failed.push((repo.clone(), e.to_string()));
            }
        }
    }

    println!();
    if execute {
        println!("Done. {} migrated, {} failed.", ok, failed.len());
    } else {
        println!("Dry run complete. Re-run with --execute to perform the migration.");
    }

    if !failed.is_empty() {
        println!("Failed repos:");
        for (r, err) in &failed {
            println!("  - {r}: {err}");
        }
    }

    Ok(())
}

/// Walk the legacy DB root and return every subdirectory name that
/// doesn't start with `.` (those are hidden helpers like `.tmp`) and
/// doesn't already look like a post-migration backup (`.old.*`).
fn discover_legacy_repos(db_root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(db_root) else {
        return out;
    };
    for e in entries.flatten() {
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Some(name) = e.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if name.contains(".old.") || name.ends_with(".old") {
            continue;
        }
        out.push(name);
    }
    out.sort();
    out
}

/// Open the source as embedded SurrealKV and count rows per known
/// table. Used by dry-run to show scope without spawning a temp server.
async fn count_source_rows(src_path: &Path, repo: &str) -> Result<String> {
    let src = open_source_embedded(src_path, repo).await?;
    let mut entity_total = 0u64;
    let mut relation_total = 0u64;

    for table in ENTITY_TABLES {
        entity_total += table_count(&src, table).await.unwrap_or(0);
    }
    for table in RELATION_TABLES {
        relation_total += table_count(&src, table).await.unwrap_or(0);
    }

    Ok(format!(
        "{entity_total} entities + {relation_total} relations"
    ))
}

/// Perform one repo's migration end-to-end. Returns a human-readable
/// summary on success.
async fn migrate_one(
    src_path: &Path,
    repo: &str,
    target_endpoint: &str,
    delete_backup: bool,
) -> Result<String> {
    let bin = find_surreal_binary()?;

    // Spawn a temp surreal server with the source dir as its storage.
    let port = pick_free_port().context("no free local port available for temp server")?;
    let temp_endpoint = format!("http://127.0.0.1:{port}");
    let src_url = format!("surrealkv:{}", src_path.display());

    let mut temp = spawn_temp_server(&bin, &src_url, port).await?;
    // From here on we MUST kill `temp` before returning, even on error.
    let result =
        migrate_one_with_server(&bin, src_path, repo, &temp_endpoint, target_endpoint).await;
    // Best-effort kill; Windows TerminateProcess, Unix SIGKILL.
    let _ = temp.kill().await;
    let _ = temp.wait().await;

    let (imported_bytes, exported_bytes) = result?;

    // Rename legacy dir so future runs skip it and the user can roll
    // back by renaming it back.
    let parent = src_path.parent().unwrap_or_else(|| Path::new("."));
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = parent.join(format!("{repo}.old.{timestamp}"));
    std::fs::rename(src_path, &backup).with_context(|| {
        format!(
            "rename {} → {} (temp server may still hold a lock — retry in a few seconds)",
            src_path.display(),
            backup.display()
        )
    })?;

    if delete_backup {
        if let Err(e) = std::fs::remove_dir_all(&backup) {
            println!(
                "  (warning: backup delete failed — keep manually at {}: {})",
                backup.display(),
                e
            );
        }
    }

    Ok(format!(
        "exported {} bytes, imported {} bytes → ns=codescope db={}",
        exported_bytes, imported_bytes, repo
    ))
}

/// The part of the migration that runs with the temp server live.
/// Returns `(imported_bytes, exported_bytes)`.
async fn migrate_one_with_server(
    bin: &Path,
    src_path: &Path,
    repo: &str,
    temp_endpoint: &str,
    target_endpoint: &str,
) -> Result<(u64, u64)> {
    // Dump to a per-repo tmp file. `tempfile` isn't a dep here, so use
    // env::temp_dir() with a PID-disambiguated filename.
    let tmp_dir = std::env::temp_dir();
    let pid = std::process::id();
    let dump = tmp_dir.join(format!("codescope-migrate-{repo}-{pid}.surql"));

    let export_status = TokioCommand::new(bin)
        .args([
            "export",
            "--endpoint",
            temp_endpoint,
            "--ns",
            "codescope",
            "--db",
            repo,
            "--user",
            "root",
            "--pass",
            "root",
        ])
        .arg(&dump)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .context("spawn `surreal export`")?;
    if !export_status.success() {
        // Clean up partial dump.
        let _ = std::fs::remove_file(&dump);
        bail!(
            "`surreal export` exited with {} for source {}",
            export_status,
            src_path.display()
        );
    }

    let exported_bytes = std::fs::metadata(&dump).map(|m| m.len()).unwrap_or(0);

    // surreal 3.0.x exports record IDs unquoted as `<table>:<id>`. When
    // the table name collides with a reserved SurrealQL keyword
    // (`function`, …) the import parser chokes on the `:`. Rewrite the
    // dump in place to backtick reserved table names wherever they
    // appear at the start of a record id.
    quote_reserved_tables_in_dump(&dump).with_context(|| {
        format!(
            "post-process dump {} to backtick reserved table names",
            dump.display()
        )
    })?;

    let import_status = TokioCommand::new(bin)
        .args([
            "import",
            "--endpoint",
            target_endpoint,
            "--ns",
            "codescope",
            "--db",
            repo,
            "--user",
            "root",
            "--pass",
            "root",
        ])
        .arg(&dump)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .context("spawn `surreal import`")?;
    if !import_status.success() {
        bail!(
            "`surreal import` exited with {} for target ns=codescope db={} \
             (dump preserved at {} for inspection)",
            import_status,
            repo,
            dump.display()
        );
    }

    let imported_bytes = exported_bytes; // import consumes the whole dump
    let _ = std::fs::remove_file(&dump);
    Ok((imported_bytes, exported_bytes))
}

/// SurrealDB reserved words that also appear as table names in the
/// codescope schema. Kept narrow on purpose: if a word turns out not to
/// be reserved, backticking it is still a no-op, but false positives
/// hurt diff readability for debugging.
const RESERVED_TABLE_WORDS: &[&str] = &["function"];

/// Rewrite `<word>:` → `` `<word>`: `` throughout the dump for each
/// reserved word. Works on raw bytes so multi-byte UTF-8 content
/// (escaped-quoted strings, identifier values) passes through untouched.
/// Skips matches preceded by an ASCII word-char (so `my_function:` stays
/// alone) or by a backtick (avoids double-wrapping).
fn quote_reserved_tables_in_dump(dump: &Path) -> Result<()> {
    let input = std::fs::read(dump).with_context(|| format!("read dump {}", dump.display()))?;
    let mut current = input;

    for word in RESERVED_TABLE_WORDS {
        let needle = format!("{word}:").into_bytes();
        let backticked = format!("`{word}`:").into_bytes();
        if !memmem_contains(&current, &needle) {
            continue;
        }
        let mut out = Vec::with_capacity(current.len());
        let mut i = 0;
        while i < current.len() {
            if i + needle.len() <= current.len() && current[i..i + needle.len()] == *needle {
                let prev = if i > 0 { Some(current[i - 1]) } else { None };
                let prev_is_word = prev.map(is_ident_byte).unwrap_or(false);
                let prev_is_backtick = prev == Some(b'`');
                if prev_is_word || prev_is_backtick {
                    out.push(current[i]);
                    i += 1;
                    continue;
                }
                out.extend_from_slice(&backticked);
                i += needle.len();
            } else {
                out.push(current[i]);
                i += 1;
            }
        }
        current = out;
    }

    std::fs::write(dump, &current).with_context(|| format!("write dump {}", dump.display()))?;
    Ok(())
}

fn memmem_contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Spawn a detached surreal server, wait for `/health` to return 200.
async fn spawn_temp_server(bin: &Path, src_url: &str, port: u16) -> Result<Child> {
    let mut cmd = TokioCommand::new(bin);
    cmd.args([
        "start",
        src_url,
        "--bind",
        &format!("127.0.0.1:{port}"),
        "--user",
        "root",
        "--pass",
        "root",
        "--log",
        "warn",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    let child = cmd
        .spawn()
        .with_context(|| format!("spawn temp surreal server on port {port} against {src_url}"))?;

    // Poll /health. SurrealKV open on an existing dir usually completes
    // well inside 3 s; we cap at 15 s to be safe.
    let endpoint = format!("http://127.0.0.1:{port}/health");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(resp) = reqwest::get(&endpoint).await {
            if resp.status().is_success() {
                return Ok(child);
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    // Temp server never came up — kill before bubbling the error.
    let mut child = child;
    let _ = child.kill().await;
    Err(anyhow!(
        "temp surreal server on port {port} did not become healthy within 15s"
    ))
}

/// Find the bundled surreal binary. Prefer the pinned install under
/// `~/.codescope/bin/`, fall back to whatever's on PATH.
fn find_surreal_binary() -> Result<PathBuf> {
    let exe_name = if cfg!(windows) {
        "surreal.exe"
    } else {
        "surreal"
    };
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".codescope").join("bin").join(exe_name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    // Fall back to PATH lookup — spawning `surreal --version` tells us
    // the OS resolver can find it.
    let probe = std::process::Command::new(exe_name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match probe {
        Ok(s) if s.success() => Ok(PathBuf::from(exe_name)),
        _ => Err(anyhow!("surreal binary not found")),
    }
}

/// Ask the OS for a free local port by binding to port 0, then dropping
/// the listener. TOCTOU is fine here — the window between release and
/// the child reopening is microseconds and we only run locally.
fn pick_free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Hit the target server's `/health` endpoint once; bail with a useful
/// hint if it's down.
async fn probe_target(endpoint: &str) -> Result<()> {
    let url = format!("{}/health", endpoint.trim_end_matches('/'));
    let resp = reqwest::get(&url)
        .await
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        bail!(
            "{url} returned {} — is the main surreal server up?",
            resp.status()
        );
    }
    Ok(())
}

/// Open a legacy per-repo directory as an embedded SurrealKV database.
/// Only used for dry-run counts; migration itself goes through spawned
/// binary.
async fn open_source_embedded(src_path: &Path, repo: &str) -> Result<codescope_core::DbHandle> {
    let url = format!("surrealkv:{}", src_path.display());
    let db = surrealdb::engine::any::connect(&url)
        .await
        .with_context(|| format!("open legacy DB at {url}"))?;
    db.use_ns("codescope")
        .use_db(repo)
        .await
        .with_context(|| format!("use ns codescope db {repo} on legacy source"))?;
    Ok(db)
}

/// Count rows in a single table, returning 0 if the table doesn't exist
/// (expected for repos that never saw some entity kinds).
async fn table_count(db: &codescope_core::DbHandle, table: &str) -> Result<u64> {
    let q = format!("SELECT count() FROM `{table}` GROUP ALL");
    let mut resp = db.query(&q).await?;
    let rows: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
    Ok(rows
        .first()
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0))
}

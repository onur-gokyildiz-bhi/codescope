//! Unified database-connection layer.
//!
//! As of R1-v2 (2026-04-18), codescope no longer opens SurrealKV files directly.
//! Instead we connect as a client to a bundled `surreal` server running on
//! `127.0.0.1:8077`. This eliminates exclusive-lock contention between the
//! CLI, MCP stdio, web daemon, and LSP — they are all independent clients of
//! the same server, and SurrealDB's own concurrency model handles multi-reader
//! / single-writer semantics correctly (which SurrealKV's file-level lock
//! could not).
//!
//! Entry points:
//! * [`connect_repo`] — open a handle for a named repo inside the codescope
//!   namespace. Every consumer should call this.
//! * [`DbHandle`] — the type alias every other crate imports. It abstracts
//!   over the transport (WS today, possibly UDS or embedded later).
//!
//! Environment knobs:
//! * `CODESCOPE_DB_URL` — override the default `ws://127.0.0.1:8077`.
//! * `CODESCOPE_DB_USER` / `CODESCOPE_DB_PASS` — override the default
//!   `root`/`root` credentials. These are only meaningful for local
//!   bundled-server usage; production deployments are out of scope for v2.
//! * `CODESCOPE_DB_NS` — override the default `codescope` namespace. Mostly
//!   useful for integration tests.

use anyhow::{Context, Result};
use std::time::Duration;
use surrealdb::engine::any::{connect, Any};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

/// Default namespace for all repos. Each repo is its own `db` inside this
/// namespace, so cross-repo isolation is a SurrealDB boundary, not a
/// filesystem boundary.
pub const DEFAULT_NS: &str = "codescope";

/// Default endpoint — the bundled `surreal` server's WebSocket port. Kept in
/// sync with the supervisor (`codescope start`).
pub const DEFAULT_URL: &str = "ws://127.0.0.1:8077";

/// Default credentials for the local bundled server. The server is only ever
/// bound to loopback, so this is safe. Production multi-user deployments are
/// deferred.
pub const DEFAULT_USER: &str = "root";
pub const DEFAULT_PASS: &str = "root";

/// The one database handle every other crate uses.
///
/// `Any` abstracts over WebSocket, HTTP, memory, or local engines. The
/// transport is selected at runtime from `CODESCOPE_DB_URL` — so tests can
/// switch to `memory://` without touching call sites.
pub type DbHandle = Surreal<Any>;

/// How long we wait for the server to accept a connection before giving up.
/// 5 s is generous for localhost but covers cold-start after `codescope start`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Open a [`DbHandle`] scoped to `repo` inside the codescope namespace.
///
/// The caller is responsible for running migrations and schema init on the
/// returned handle (see `graph::migrations` / `graph::schema`).
pub async fn connect_repo(repo: &str) -> Result<DbHandle> {
    let url = std::env::var("CODESCOPE_DB_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let ns = std::env::var("CODESCOPE_DB_NS").unwrap_or_else(|_| DEFAULT_NS.to_string());

    let db = tokio::time::timeout(CONNECT_TIMEOUT, connect(&url))
        .await
        .with_context(|| {
            format!("timed out connecting to SurrealDB at {url} — is `codescope start` running?")
        })?
        .with_context(|| format!("failed to connect to SurrealDB at {url}"))?;

    // Authenticate only when the URL implies a network server; embedded or
    // in-memory engines don't need signin (and will reject it).
    if is_networked(&url) {
        let user = std::env::var("CODESCOPE_DB_USER").unwrap_or_else(|_| DEFAULT_USER.to_string());
        let pass = std::env::var("CODESCOPE_DB_PASS").unwrap_or_else(|_| DEFAULT_PASS.to_string());
        db.signin(Root {
            username: user,
            password: pass,
        })
        .await
        .with_context(|| "SurrealDB auth failed — check CODESCOPE_DB_USER/PASS")?;
    }

    // SurrealDB v3 auto-creates the namespace + database on first `use_db`.
    // When two clients hit a cold server simultaneously that DDL transaction
    // can race with itself and surface as "Transaction write conflict". The
    // conflict is transient and retry-safe, so bounce a few times before
    // giving up. Once the NS/DB exist this path is idempotent and cheap.
    let mut attempt = 0u32;
    loop {
        match db.use_ns(&ns).use_db(repo).await {
            Ok(_) => break,
            Err(e) => {
                let msg = e.to_string();
                let transient = msg.contains("Transaction")
                    || msg.contains("write conflict")
                    || msg.contains("retried");
                attempt += 1;
                if !transient || attempt > 5 {
                    return Err(anyhow::Error::from(e))
                        .with_context(|| format!("use ns '{ns}' db '{repo}' failed"));
                }
                let backoff = Duration::from_millis(25 * (1 << attempt)); // 50,100,200,400,800ms
                tokio::time::sleep(backoff).await;
            }
        }
    }

    Ok(db)
}

/// Migration shim: the legacy call sites all held a per-repo filesystem path
/// like `~/.codescope/db/<repo>/`. The R1-v2 server model only cares about
/// the repo *name*, so here we extract the last path component and delegate
/// to [`connect_repo`]. Existing `Surreal::new::<SurrealKv>(path)` calls can
/// be rewritten mechanically to `connect_path(path)`.
pub async fn connect_path(db_path: impl AsRef<std::path::Path>) -> Result<DbHandle> {
    let p = db_path.as_ref();
    let repo = p
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("cannot derive repo name from path {:?}", p))?;
    connect_repo(repo).await
}

/// Open a [`DbHandle`] without binding to a specific database. Useful for
/// admin tasks (`INFO FOR KV`, listing databases).
pub async fn connect_admin() -> Result<DbHandle> {
    let url = std::env::var("CODESCOPE_DB_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let db = tokio::time::timeout(CONNECT_TIMEOUT, connect(&url))
        .await
        .with_context(|| format!("timed out connecting to SurrealDB at {url}"))?
        .with_context(|| format!("failed to connect to SurrealDB at {url}"))?;

    if is_networked(&url) {
        let user = std::env::var("CODESCOPE_DB_USER").unwrap_or_else(|_| DEFAULT_USER.to_string());
        let pass = std::env::var("CODESCOPE_DB_PASS").unwrap_or_else(|_| DEFAULT_PASS.to_string());
        db.signin(Root {
            username: user,
            password: pass,
        })
        .await
        .with_context(|| "SurrealDB auth failed")?;
    }
    Ok(db)
}

fn is_networked(url: &str) -> bool {
    url.starts_with("ws://")
        || url.starts_with("wss://")
        || url.starts_with("http://")
        || url.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn networked_url_detection() {
        assert!(is_networked("ws://127.0.0.1:8077"));
        assert!(is_networked("wss://db.example.com"));
        assert!(is_networked("http://localhost:8000"));
        assert!(!is_networked("memory"));
        assert!(!is_networked("surrealkv:/path"));
        assert!(!is_networked("rocksdb:/path"));
    }
}

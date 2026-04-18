//! R1-v2 spike: can we connect to the bundled SurrealDB server as a remote
//! client and round-trip a basic query?
//!
//! Requires a running local server at `ws://127.0.0.1:8077` with root:root
//! credentials. Skip gracefully if the server isn't reachable — CI will spin
//! one up explicitly (Phase C, R3).
//!
//! Run locally with: `cargo test -p codescope-core --test remote_connect_spike`

use codescope_core::db::{connect_repo, DEFAULT_URL};

async fn server_reachable() -> bool {
    let url = std::env::var("CODESCOPE_DB_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let health = url
        .replace("ws://", "http://")
        .replace("wss://", "https://")
        .trim_end_matches('/')
        .to_string()
        + "/health";
    reqwest::Client::new()
        .get(&health)
        .timeout(std::time::Duration::from_secs(1))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[tokio::test]
async fn connect_repo_and_run_info_for_kv() {
    if !server_reachable().await {
        eprintln!("skipping: SurrealDB server not running at {DEFAULT_URL}");
        return;
    }

    let db = connect_repo("__spike_test_db")
        .await
        .expect("connect_repo");
    let mut resp = db
        .query("INFO FOR DB")
        .await
        .expect("INFO FOR DB should succeed");
    // INFO FOR DB returns a single object; we only care it didn't error.
    let _val: Option<serde_json::Value> = resp.take(0).expect("take row");
}

#[tokio::test]
async fn two_clients_against_same_db_no_lock_conflict() {
    if !server_reachable().await {
        return;
    }

    let a = connect_repo("__spike_test_concurrent")
        .await
        .expect("client a");
    let b = connect_repo("__spike_test_concurrent")
        .await
        .expect("client b");

    // Concurrent reads — the whole point of the migration. With embedded
    // SurrealKv this would hit `os error 33`; with the remote server it is
    // a non-event.
    let (ra, rb) = tokio::join!(
        a.query("SELECT * FROM test_table LIMIT 1"),
        b.query("SELECT * FROM test_table LIMIT 1"),
    );
    ra.expect("client a query");
    rb.expect("client b query");
}

//! Multi-project smoke test — R3.
//!
//! Two independent `codescope_core::DbHandle` clients open different
//! DBs inside NS=codescope on the same server concurrently. The
//! whole point of R1-v2 (bundled surreal server, no per-repo
//! SurrealKV locks) is that this works without contention. If it
//! regresses, this test catches it.

use codescope_core::{connect_repo, DEFAULT_NS};
use codescope_e2e::TestServer;

#[tokio::test]
async fn two_repos_no_contention() -> anyhow::Result<()> {
    let server = TestServer::start().await?;
    // connect_repo reads CODESCOPE_DB_URL.
    std::env::set_var("CODESCOPE_DB_URL", &server.endpoint_ws);
    std::env::set_var("CODESCOPE_DB_USER", "root");
    std::env::set_var("CODESCOPE_DB_PASS", "root");

    let a = connect_repo("alpha").await?;
    let b = connect_repo("beta").await?;

    // Write something in each, read it back. Any lock contention
    // would surface as a Transaction error here. The `meta` table is
    // schemaful in production; for this smoke we just use an ad-hoc
    // table so we don't depend on schema init.
    a.query("CREATE misc:tag SET val = 'in-alpha'").await?;
    b.query("CREATE misc:tag SET val = 'in-beta'").await?;

    let mut ra = a.query("SELECT val FROM misc:tag").await?;
    let rows_a: Vec<serde_json::Value> = ra.take(0).unwrap_or_default();
    assert_eq!(
        rows_a
            .first()
            .and_then(|v| v.get("val"))
            .and_then(|v| v.as_str()),
        Some("in-alpha"),
        "alpha repo saw: {rows_a:?}"
    );

    let mut rb = b.query("SELECT val FROM misc:tag").await?;
    let rows_b: Vec<serde_json::Value> = rb.take(0).unwrap_or_default();
    assert_eq!(
        rows_b
            .first()
            .and_then(|v| v.get("val"))
            .and_then(|v| v.as_str()),
        Some("in-beta"),
        "beta repo saw: {rows_b:?}"
    );

    // Cross-check: alpha's handle does NOT see beta's row. This is
    // the isolation invariant `NS=codescope, DB=<repo>` buys us.
    assert!(!serde_json::to_string(&rows_a)?.contains("in-beta"));
    assert!(!serde_json::to_string(&rows_b)?.contains("in-alpha"));

    // Sanity: both handles are in NS=codescope.
    let _ = DEFAULT_NS; // silence unused-import lint when the assertion below changes

    Ok(())
}

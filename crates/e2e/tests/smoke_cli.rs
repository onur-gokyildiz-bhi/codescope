//! CLI smoke test — R3.
//!
//! Exercises the codescope binary end-to-end against a fresh
//! ephemeral server: run `query` + `stats` with `CODESCOPE_DB_URL`
//! pointing at the fixture. This is the surface that panels /
//! scripts drive, and regressions here (CLI hangs, argparse
//! breakage, structured-error shape drift) tend to be invisible to
//! unit tests.

use codescope_e2e::TestServer;
use std::process::Command;

fn codescope_bin() -> std::path::PathBuf {
    // `CARGO_BIN_EXE_codescope` is set by cargo test for binaries in
    // the `cli` crate — fall back to the workspace debug target when
    // running through `cargo test -p codescope-e2e`.
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_codescope") {
        return p.into();
    }
    // `CARGO_MANIFEST_DIR` is crates/e2e/; binary lives two levels up.
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("target");
    p.push("debug");
    p.push(if cfg!(windows) {
        "codescope.exe"
    } else {
        "codescope"
    });
    p
}

#[tokio::test]
async fn cli_query_round_trips_through_server() {
    let server = TestServer::start().await.expect("start server");
    let bin = codescope_bin();
    if !bin.exists() {
        // cargo test doesn't guarantee the bin is built; make the
        // miss actionable instead of a cryptic IO error.
        panic!(
            "codescope binary not built at {} — run `cargo build -p codescope` first",
            bin.display()
        );
    }

    let out = Command::new(&bin)
        .args(["--repo", "e2e", "query", "INFO FOR NS"])
        .env("CODESCOPE_DB_URL", &server.endpoint_ws)
        .env("CODESCOPE_DB_USER", "root")
        .env("CODESCOPE_DB_PASS", "root")
        .output()
        .expect("run codescope query");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "codescope query exited non-zero.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // `INFO FOR NS` returns a JSON-like object — we don't care about
    // the exact schema, just that we got *something* structured back.
    assert!(
        stdout.contains("databases") || stdout.contains("accesses") || stdout.contains("db"),
        "expected INFO FOR NS output, got:\n{stdout}"
    );
}

#[tokio::test]
async fn cli_emits_structured_error_on_connect_refused() {
    let bin = codescope_bin();
    if !bin.exists() {
        panic!(
            "codescope binary not built at {} — run `cargo build -p codescope` first",
            bin.display()
        );
    }

    // Point at a definitely-dead port.
    let out = Command::new(&bin)
        .args(["--repo", "missing", "query", "SELECT * FROM file LIMIT 1"])
        .env("CODESCOPE_DB_URL", "ws://127.0.0.1:1")
        .output()
        .expect("run codescope query");

    assert!(!out.status.success(), "should have failed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    // R2 contract — stderr body parses as JSON with ok:false +
    // error.code. We don't assert which code (db_unreachable vs
    // internal) since the classifier is heuristic; only the shape.
    let parsed: Result<serde_json::Value, _> = stderr
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .map(serde_json::from_str)
        .unwrap_or_else(|| Err(serde_json::from_str::<serde_json::Value>("").unwrap_err()));
    let v = parsed.unwrap_or_else(|e| panic!("stderr not JSON ({e}):\n{stderr}"));
    assert_eq!(v.get("ok"), Some(&serde_json::Value::Bool(false)));
    assert!(v.get("error").is_some(), "missing `error`: {v}");
}

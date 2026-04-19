//! Bare-minimum smoke test — the only R3 test that MUST pass for the
//! green/red signal. Everything else (CLI, MCP HTTP, multi-project)
//! rides on top of this: if the spawn-and-health-check primitive is
//! broken, there's no point running the rest.

use codescope_e2e::TestServer;

#[tokio::test]
async fn server_starts_and_health_returns_ok() {
    let server = TestServer::start()
        .await
        .expect("start ephemeral surreal server");

    let r = reqwest::get(format!("{}/health", server.endpoint_http))
        .await
        .expect("GET /health");
    assert!(r.status().is_success(), "health returned {:?}", r.status());
}

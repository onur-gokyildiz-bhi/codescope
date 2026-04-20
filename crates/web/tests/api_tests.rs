//! Smoke tests for the codescope-web HTTP API.
//! Uses tower::ServiceExt::oneshot to invoke axum routes directly,
//! no real network sockets needed.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use codescope_core::graph::schema::init_schema;
use codescope_web::build_web_router;
use surrealdb::engine::any;
use tower::ServiceExt;

async fn setup_router() -> axum::Router {
    // Post R1-v2 helpers consume `DbHandle = Surreal<Any>`;
    // `any::connect("memory")` gives us the matching type without
    // a real server spawn.
    let db = any::connect("memory").await.unwrap();
    db.use_ns("codescope").use_db("test").await.unwrap();
    init_schema(&db).await.unwrap();
    build_web_router(db)
}

#[tokio::test]
async fn root_serves_html() {
    let app = setup_router().await;
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    // Should be 200 OK or 404 (if frontend not built) — anything but 500
    assert!(
        response.status() != StatusCode::INTERNAL_SERVER_ERROR,
        "root should not 500"
    );
}

#[tokio::test]
async fn stats_endpoint_returns_json() {
    let app = setup_router().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    // Empty DB should still return zero counts, not crash
    assert!(json.is_object());
}

#[tokio::test]
async fn search_endpoint_empty_query_returns_empty_array() {
    let app = setup_router().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/search")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(json.is_array(), "should return JSON array");
}

#[tokio::test]
async fn search_endpoint_with_query_string() {
    let app = setup_router().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/search?q=test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn nonexistent_route_returns_404() {
    let app = setup_router().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/this-does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

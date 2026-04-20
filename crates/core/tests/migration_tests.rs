//! Tests for the schema migration system.
use codescope_core::graph::migrations::migrate_to_current;
use codescope_core::graph::schema::{
    get_schema_version, init_schema, set_schema_version, SCHEMA_VERSION,
};
use codescope_core::DbHandle;
use surrealdb::engine::any;

async fn setup_empty_db() -> DbHandle {
    let db = any::connect("memory")
        .await
        .expect("Failed to create in-memory DB");
    db.use_ns("codescope")
        .use_db("test_migrations")
        .await
        .expect("Failed to set namespace");
    init_schema(&db).await.expect("Failed to init schema");
    db
}

#[tokio::test]
async fn fresh_db_starts_at_version_zero() {
    // `init_schema` must NOT stamp a version — that's migrate_to_current's job.
    let db = setup_empty_db().await;
    let v = get_schema_version(&db).await.expect("get_schema_version");
    assert_eq!(v, 0, "fresh DB should report version 0 before migration");
}

#[tokio::test]
async fn migrate_to_current_upgrades_fresh_db() {
    let db = setup_empty_db().await;
    let final_version = migrate_to_current(&db).await.expect("migrate_to_current");
    assert_eq!(
        final_version, SCHEMA_VERSION,
        "migrate_to_current should return SCHEMA_VERSION"
    );
    let persisted = get_schema_version(&db).await.expect("get_schema_version");
    assert_eq!(
        persisted, SCHEMA_VERSION,
        "schema_version should be persisted to the meta table"
    );
}

#[tokio::test]
async fn migrate_to_current_is_idempotent() {
    let db = setup_empty_db().await;
    let first = migrate_to_current(&db).await.expect("first migration");
    let second = migrate_to_current(&db).await.expect("second migration");
    assert_eq!(first, SCHEMA_VERSION);
    assert_eq!(second, SCHEMA_VERSION);
    let persisted = get_schema_version(&db).await.expect("get_schema_version");
    assert_eq!(persisted, SCHEMA_VERSION);
}

#[tokio::test]
async fn set_and_get_schema_version_roundtrip() {
    let db = setup_empty_db().await;
    set_schema_version(&db, 42).await.expect("set");
    let v = get_schema_version(&db).await.expect("get");
    assert_eq!(v, 42);
}

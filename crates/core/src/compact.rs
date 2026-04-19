//! RTK-06 — response compaction.
//!
//! Tools return JSON that often carries fields the LLM doesn't
//! need: embedding vectors (many KB each), content hashes,
//! qualified paths that duplicate `name + file_path`, etc.
//! Stripping them before the model sees them drops another 30–50%
//! off the wire.
//!
//! Two levels, env-driven:
//!
//! * `CODESCOPE_COMPACT=1` (alias: `compact`) — strip the heavy
//!   internal fields (`embedding`, `binary_embedding`,
//!   `content_hash`, `embedding_model`). Same semantics the LLM
//!   expects; never needed at the tool surface.
//! * `CODESCOPE_COMPACT=ultra` — also strips timestamps,
//!   `qualified_name`, and collapses absolute file paths to their
//!   last three segments. Lossier — use when the savings matter
//!   more than the provenance.
//!
//! The helper is recursive and idempotent: calling it twice on
//! the same value is a no-op.

use serde_json::{Map, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    None,
    Compact,
    Ultra,
}

impl Level {
    pub fn from_env() -> Self {
        match std::env::var("CODESCOPE_COMPACT").as_deref().unwrap_or("") {
            "" | "0" | "off" | "false" => Level::None,
            "ultra" => Level::Ultra,
            _ => Level::Compact,
        }
    }
}

/// Fields stripped at `Compact` level. Internal book-keeping that
/// the model never needs.
const STRIP_COMPACT: &[&str] = &[
    "embedding",
    "binary_embedding",
    "embedding_model",
    "content_hash",
    "signature_hash",
];

/// Additional fields stripped at `Ultra` level. These are useful
/// for humans reading the web UI but redundant at the tool
/// surface: `qualified_name` duplicates `name + file_path`, the
/// timestamps are already covered by commit info, etc.
const STRIP_ULTRA: &[&str] = &[
    "qualified_name",
    "created_at",
    "updated_at",
    "indexed_at",
    "last_seen",
];

/// In-place strip on an arbitrary JSON value. Walks arrays and
/// objects; leaves primitives untouched.
pub fn apply(value: &mut Value, level: Level) {
    if level == Level::None {
        return;
    }
    match value {
        Value::Object(map) => {
            prune_map(map, level);
            for v in map.values_mut() {
                apply(v, level);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                apply(v, level);
            }
        }
        _ => {}
    }
}

fn prune_map(map: &mut Map<String, Value>, level: Level) {
    for key in STRIP_COMPACT {
        map.remove(*key);
    }
    if level == Level::Ultra {
        for key in STRIP_ULTRA {
            map.remove(*key);
        }
        // Absolute paths → last three segments. Saves bytes when
        // projects live deep in a home dir (e.g. on Windows:
        // `C:\Users\…\Documents\foo\bar.rs` → `…/foo/bar.rs`).
        if let Some(Value::String(s)) = map.get_mut("file_path") {
            if let Some(short) = shorten_path(s) {
                *s = short;
            }
        }
    }
}

fn shorten_path(p: &str) -> Option<String> {
    let norm: String = p.replace('\\', "/");
    let segments: Vec<&str> = norm.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() <= 3 {
        return None;
    }
    let tail = &segments[segments.len() - 3..];
    Some(format!("…/{}", tail.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_strips_embedding() {
        let mut v = serde_json::json!({
            "name": "foo",
            "embedding": [0.1, 0.2, 0.3],
            "content_hash": "abc",
        });
        apply(&mut v, Level::Compact);
        assert!(v.get("embedding").is_none());
        assert!(v.get("content_hash").is_none());
        assert_eq!(v["name"], "foo");
    }

    #[test]
    fn ultra_strips_timestamps_and_shortens_paths() {
        let mut v = serde_json::json!({
            "name": "foo",
            "file_path": "C:/Users/me/Projects/a/b/c/d.rs",
            "created_at": "2026-04-20T00:00:00Z",
            "qualified_name": "crate::foo",
        });
        apply(&mut v, Level::Ultra);
        assert!(v.get("created_at").is_none());
        assert!(v.get("qualified_name").is_none());
        assert_eq!(v["file_path"], "…/b/c/d.rs");
    }

    #[test]
    fn idempotent() {
        let mut v = serde_json::json!({ "name": "foo" });
        apply(&mut v, Level::Ultra);
        let first = v.clone();
        apply(&mut v, Level::Ultra);
        assert_eq!(first, v);
    }

    #[test]
    fn none_is_noop() {
        let mut v = serde_json::json!({ "embedding": [0.1], "name": "foo" });
        let before = v.clone();
        apply(&mut v, Level::None);
        assert_eq!(before, v);
    }

    #[test]
    fn walks_nested_arrays() {
        let mut v = serde_json::json!({
            "items": [ { "name": "a", "embedding": [0.0] } ]
        });
        apply(&mut v, Level::Compact);
        assert!(v["items"][0].get("embedding").is_none());
    }
}

//! Unit tests for pure helper functions in mcp-server/src/helpers.rs.
//! These do not require a database — they test path/string manipulation logic.

use codescope_mcp::helpers::{derive_scope_from_file_path, find_claude_project_dir};
use std::path::PathBuf;

// ── derive_scope_from_file_path ─────────────────────────────────
//
// NOTE: This function's doc examples are misleading. The current
// implementation drops trailing depth (it `take`s parts.len()-3 after
// skipping the first segment). These tests document the actual behavior.

#[test]
fn scope_crates_layout_drops_filename_and_one_more_dir() {
    // crates/core/src/graph/builder.rs → "core" (NOT "core::graph")
    assert_eq!(
        derive_scope_from_file_path("crates/core/src/graph/builder.rs"),
        "core"
    );
}

#[test]
fn scope_crates_4_segments() {
    // crates/mcp-server/src/server.rs → "mcp-server"
    assert_eq!(
        derive_scope_from_file_path("crates/mcp-server/src/server.rs"),
        "mcp-server"
    );
}

#[test]
fn scope_two_part_src_path() {
    // src/main.rs has only 2 parts → falls into the >=2 branch
    // returns parts[..1].join("::") = "src"
    assert_eq!(derive_scope_from_file_path("src/main.rs"), "src");
}

#[test]
fn scope_normalizes_windows_paths() {
    // Same as forward slashes
    assert_eq!(
        derive_scope_from_file_path("crates\\core\\src\\graph\\builder.rs"),
        "core"
    );
}

#[test]
fn scope_single_file() {
    // Just a filename — no directory → root
    assert_eq!(derive_scope_from_file_path("README.md"), "root");
}

#[test]
fn scope_two_part_path() {
    // dir/file → dir as scope
    assert_eq!(derive_scope_from_file_path("docs/api.md"), "docs");
}

#[test]
fn scope_lib_layout_4_segments() {
    // lib/foo/bar/baz.rs → "foo" (4 parts, take(1), filter src)
    assert_eq!(derive_scope_from_file_path("lib/foo/bar/baz.rs"), "foo");
}

// ── find_claude_project_dir ─────────────────────────────────────

#[test]
fn project_dir_returns_under_claude_projects() {
    // Smoke test: just ensures the function doesn't panic and returns a path
    // containing both .claude and projects.
    let path = find_claude_project_dir(&PathBuf::from("/some/codebase"), "test-repo");
    let str_path = path.to_string_lossy();
    assert!(
        str_path.contains(".claude") && str_path.contains("projects"),
        "Expected .claude/projects in path, got: {}",
        str_path
    );
}

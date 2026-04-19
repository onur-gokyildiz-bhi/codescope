//! R2 — structured error shape for MCP tool responses.
//!
//! Tool handlers historically returned plain-text error strings
//! ("No project initialized…"). The R2 contract says every failure
//! body carries `{ok: false, error: {code, message, hint}}` — same
//! shape the web layer emits. Tools return `String`, so we just
//! serialise the contract into a string and let the caller display
//! it. Claude Code parses the JSON and shows the hint.

use serde::Serialize;

pub mod code {
    pub const DB_UNREACHABLE: &str = "db_unreachable";
    pub const REPO_NOT_FOUND: &str = "repo_not_found";
    pub const INVALID_INPUT: &str = "invalid_input";
    pub const TIMEOUT: &str = "timeout";
    pub const INTERNAL: &str = "internal";
    pub const INDEX_NOT_READY: &str = "index_not_ready";
    pub const NO_PROJECT: &str = "no_project";
}

#[derive(Serialize)]
struct Body<'a> {
    ok: bool,
    error: Inner<'a>,
}

#[derive(Serialize)]
struct Inner<'a> {
    code: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<&'a str>,
}

/// Build a JSON-encoded error body. Never panics — on serialisation
/// failure (vanishingly unlikely) falls back to a bare message so the
/// caller still gets *something*.
pub fn tool_error(code: &str, message: &str, hint: Option<&str>) -> String {
    let body = Body {
        ok: false,
        error: Inner {
            code,
            message,
            hint,
        },
    };
    serde_json::to_string(&body).unwrap_or_else(|_| {
        format!(
            "{{\"ok\":false,\"error\":{{\"code\":\"{code}\",\"message\":{:?}}}}}",
            message
        )
    })
}

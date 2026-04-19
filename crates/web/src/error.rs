//! R2 — structured error contract for HTTP responses.
//!
//! Every non-2xx body should serialise to:
//!
//! ```json
//! { "ok": false, "error": { "code": "...", "message": "...", "hint": "..." } }
//! ```
//!
//! The frontend has a single handler that reads `.error.message` +
//! `.error.hint` and renders a toast with an optional action button
//! (e.g. "Run `codescope start`"). Don't emit bare strings; don't emit
//! `Json({"error": "..."})` with a nested string — both break the
//! contract and confuse the handler.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Well-known error codes. Keep in sync with
/// `docs/architecture/REFACTOR-R1-R6.md §2.6`. Adding a new code is
/// cheap; renaming an existing one breaks the frontend handler.
pub mod code {
    pub const DB_UNREACHABLE: &str = "db_unreachable";
    pub const DB_VERSION_DRIFT: &str = "db_version_drift";
    pub const DB_CORRUPT: &str = "db_corrupt";
    pub const REPO_NOT_FOUND: &str = "repo_not_found";
    pub const INVALID_INPUT: &str = "invalid_input";
    pub const TIMEOUT: &str = "timeout";
    pub const INTERNAL: &str = "internal";
}

/// Structured error body. Constructed via helper associated functions
/// (e.g. [`ApiError::db_unreachable`]) or via the [`From`] impls for
/// common error types.
#[derive(Debug, Serialize)]
pub struct ApiError {
    #[serde(skip)]
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    // --- Common constructors ---

    pub fn repo_not_found(repo: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            code::REPO_NOT_FOUND,
            format!("Project DB '{repo}' not found"),
        )
        .with_hint(format!(
            "Index a codebase first: `codescope index <path> --repo {repo}`"
        ))
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code::INVALID_INPUT, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, code::INTERNAL, message)
    }

    pub fn db_unreachable(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            code::DB_UNREACHABLE,
            message,
        )
        .with_hint("Is the surreal server up? Run `codescope start`.")
    }

    /// Inspect a generic backend/DB error and classify it. Falls back
    /// to `internal` when no known signature matches. This is the main
    /// bridge from `anyhow::Error` / string errors into the contract.
    pub fn from_db_err(repo: &str, err: impl std::fmt::Display) -> Self {
        let msg = err.to_string();
        let low = msg.to_lowercase();
        if low.contains("connection refused")
            || low.contains("tcp connect")
            || low.contains("io error")
            || low.contains("refused")
        {
            return Self::db_unreachable(format!("Can't reach surreal server: {msg}"));
        }
        if low.contains("timed out") || low.contains("timeout") {
            return Self::new(
                StatusCode::GATEWAY_TIMEOUT,
                code::TIMEOUT,
                format!("DB timeout for '{repo}': {msg}"),
            );
        }
        if low.contains("not found") || low.contains("no such") {
            return Self::repo_not_found(repo);
        }
        if low.contains("corrupt") || low.contains("broken") {
            return Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                code::DB_CORRUPT,
                format!("Project DB '{repo}' is corrupt: {msg}"),
            )
            .with_hint(format!(
                "Run `codescope repair --repo {repo}` to rebuild it."
            ));
        }
        Self::internal(format!("DB error for '{repo}': {msg}"))
    }
}

#[derive(Serialize)]
struct Envelope<'a> {
    ok: bool,
    error: &'a ApiError,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Envelope {
            ok: false,
            error: &self,
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<(StatusCode, String)> for ApiError {
    fn from(pair: (StatusCode, String)) -> Self {
        // Back-compat bridge: many handlers still build `(status, msg)`.
        // Map common status codes to the closest contract code.
        let (status, message) = pair;
        let code = match status {
            StatusCode::NOT_FOUND => code::REPO_NOT_FOUND,
            StatusCode::BAD_REQUEST => code::INVALID_INPUT,
            StatusCode::SERVICE_UNAVAILABLE => code::DB_UNREACHABLE,
            StatusCode::GATEWAY_TIMEOUT => code::TIMEOUT,
            _ => code::INTERNAL,
        };
        Self::new(status, code, message)
    }
}

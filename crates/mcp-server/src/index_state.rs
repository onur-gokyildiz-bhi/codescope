//! Shared indexing state for readiness gating.
//!
//! Problem: MCP tool handlers were answering queries while the background
//! auto-index was still running (or had failed), producing empty results
//! that looked like "no data in this codebase" to the LLM. Empty results
//! cause the agent to fall back to Read/Grep — defeating the whole point
//! of codescope.
//!
//! Solution: every tool handler checks `IndexState` before querying the
//! graph. If the index is mid-build, tools return a structured "indexing
//! in progress" JSON payload so the agent knows to retry. If the index
//! failed, the reason is surfaced.
//!
//! This state is shared via `Arc<IndexState>` on `GraphRagServer`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// High-level phase of indexing for a single project.
#[derive(Debug, Clone)]
pub enum Phase {
    /// No indexing started. Index may or may not exist from a previous run.
    Idle,
    /// Background indexing is actively running.
    Indexing,
    /// Indexing completed successfully — tool handlers proceed normally.
    Ready,
    /// Indexing failed. `reason` is surfaced in tool responses.
    Failed { reason: String },
}

/// A single parse error, kept as `(file, error_string)`. Full list is
/// logged; tool handlers expose only the count to avoid blowing context.
pub type ParseError = (PathBuf, String);

/// Shared mutable indexing state. Cheap to clone (Arc inside).
#[derive(Clone)]
pub struct IndexState {
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    phase: Phase,
    started_at: Option<Instant>,
    files_total: usize,
    files_done: usize,
    files_skipped: usize,
    errors: Vec<ParseError>,
}

impl Default for IndexState {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                phase: Phase::Idle,
                started_at: None,
                files_total: 0,
                files_done: 0,
                files_skipped: 0,
                errors: Vec::new(),
            })),
        }
    }

    /// Transition to `Indexing` and reset counters. Called at the very
    /// start of the pipeline.
    pub async fn start(&self) {
        let mut g = self.inner.write().await;
        g.phase = Phase::Indexing;
        g.started_at = Some(Instant::now());
        g.files_total = 0;
        g.files_done = 0;
        g.files_skipped = 0;
        g.errors.clear();
    }

    /// Record the total file count discovered during walk.
    pub async fn set_total(&self, total: usize) {
        self.inner.write().await.files_total = total;
    }

    /// Increment the done counter. Called per-file by phase2_insert.
    pub async fn inc_done(&self) {
        self.inner.write().await.files_done += 1;
    }

    /// Increment the skipped counter (e.g. unreadable files).
    pub async fn inc_skipped(&self) {
        self.inner.write().await.files_skipped += 1;
    }

    /// Record a parse error. Stored as (path, message) and logged.
    pub async fn push_error(&self, path: PathBuf, err: String) {
        tracing::warn!("Parse error {}: {}", path.display(), err);
        let mut g = self.inner.write().await;
        // Cap at 1000 errors to avoid unbounded memory growth on pathological
        // repos. The count is still tracked by `errors_count()` so callers
        // know the true error count even after cap.
        if g.errors.len() < 1000 {
            g.errors.push((path, err));
        }
    }

    /// Transition to `Ready`. Called when the pipeline completes successfully.
    pub async fn mark_ready(&self) {
        self.inner.write().await.phase = Phase::Ready;
    }

    /// Transition to `Failed` with a human-readable reason. Used when
    /// phase2 insert fails after phase0 wiped the DB.
    pub async fn mark_failed(&self, reason: impl Into<String>) {
        let reason = reason.into();
        tracing::error!("Index failed: {}", reason);
        self.inner.write().await.phase = Phase::Failed { reason };
    }

    /// Current phase (cloned for short-term use).
    pub async fn phase(&self) -> Phase {
        self.inner.read().await.phase.clone()
    }

    /// Snapshot for `index_status` tool.
    pub async fn snapshot(&self) -> IndexStatusSnapshot {
        let g = self.inner.read().await;
        let elapsed = g.started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        IndexStatusSnapshot {
            state: match &g.phase {
                Phase::Idle => "idle".into(),
                Phase::Indexing => "indexing".into(),
                Phase::Ready => "ready".into(),
                Phase::Failed { reason } => format!("failed: {}", reason),
            },
            files_total: g.files_total,
            files_indexed: g.files_done,
            files_skipped: g.files_skipped,
            errors_count: g.errors.len(),
            running_time_secs: elapsed,
        }
    }

    /// Readiness-gate check intended for tool handlers.
    ///
    /// Returns `Some(response_string)` if the tool should short-circuit
    /// with that response (because index is mid-build or failed).
    /// Returns `None` when the index is Ready or Idle (legacy DB from
    /// a prior session — we optimistically allow tool calls since the
    /// graph may already have data).
    pub async fn gate(&self) -> Option<String> {
        let g = self.inner.read().await;
        match &g.phase {
            Phase::Ready | Phase::Idle => None,
            Phase::Indexing => {
                let elapsed = g.started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
                Some(format!(
                    "{{\"status\":\"indexing\",\"progress\":\"{}/{}\",\"elapsed_secs\":{},\"errors_count\":{},\"message\":\"Index in progress — retry in a few seconds. Call index_status() for details.\"}}",
                    g.files_done, g.files_total, elapsed, g.errors.len()
                ))
            }
            Phase::Failed { reason } => Some(format!(
                "{{\"status\":\"failed\",\"reason\":{},\"message\":\"Index build failed — call index_status() for details, or re-run index_codebase to retry.\"}}",
                serde_json::to_string(reason).unwrap_or_else(|_| "\"(unprintable)\"".into())
            )),
        }
    }
}

/// Public-facing snapshot used by `index_status` tool output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexStatusSnapshot {
    pub state: String,
    pub files_total: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub errors_count: usize,
    pub running_time_secs: u64,
}

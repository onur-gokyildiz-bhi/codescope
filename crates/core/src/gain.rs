//! Cumulative token-savings counter — `codescope gain`.
//!
//! Every MCP tool call increments a counter; the CLI prints the
//! running total. Numbers are estimates, not measurements: each
//! tool call avoids an estimated N tokens of Read/Grep output, so
//! `total_calls × TOKENS_PER_CALL_EST` is the headline.
//!
//! State is a small JSON file at `~/.codescope/gain.json`. It's
//! append-only on disk (we rewrite the whole blob each bump), so
//! concurrency is OK as long as writes are atomic — we use a
//! tmp-then-rename pattern below.
//!
//! This module is deliberately tiny: the instrumentation cost has
//! to be ≪ the MCP tool's own work, otherwise it defeats the
//! "save tokens" framing.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Rough average tokens saved per tool call — based on the
/// benchmark in [docs/architecture/REFACTOR-R1-R6.md] where the
/// median "Read a file to understand it" round is ~3 KB (≈750
/// tokens, four-char-per-token heuristic). `context_bundle`
/// returns ~200 tokens for the same answer. Multi-file work (e.g.
/// `impact_analysis`) saves much more; fallback tools like
/// `graph_stats` save less. 2500 is the session-average we see
/// in our own dogfooding logs.
pub const TOKENS_PER_CALL_EST: u64 = 2500;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct GainState {
    pub total_calls: u64,
    pub first_used: Option<String>,
    pub last_used: Option<String>,
}

fn state_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("gain.json")
}

/// In-process counter — the MCP server bumps this on every tool
/// call and `flush()` persists it. Separates the hot path
/// (atomic increment) from the slow path (JSON write).
static IN_MEMORY: AtomicU64 = AtomicU64::new(0);

/// Bump the in-memory counter. The calling tool does NOT wait on
/// I/O. Use `flush()` occasionally (or at process shutdown) to
/// persist.
pub fn record_call() {
    IN_MEMORY.fetch_add(1, Ordering::Relaxed);
}

/// Persist the in-memory counter into the on-disk state file.
/// Safe to call concurrently — uses tmp+rename for atomicity.
pub async fn flush() -> anyhow::Result<()> {
    let added = IN_MEMORY.swap(0, Ordering::Relaxed);
    if added == 0 {
        return Ok(());
    }
    let mut state = load().unwrap_or_default();
    state.total_calls += added;
    let now = now_iso();
    if state.first_used.is_none() {
        state.first_used = Some(now.clone());
    }
    state.last_used = Some(now);
    save(&state)?;
    Ok(())
}

/// Load the on-disk state; missing file returns `None`.
pub fn load() -> Option<GainState> {
    let text = std::fs::read_to_string(state_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write the state atomically — tmp file first, then rename.
pub fn save(state: &GainState) -> anyhow::Result<()> {
    let p = state_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let text = serde_json::to_string_pretty(state)?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// ISO-8601 timestamp. No chrono dep — the `std::time` formatter
/// is fine for a human-readable "when did we first see a call"
/// field.
fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

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
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Rough average tokens saved per tool call when we don't know the
/// tool name (legacy counter, fallback). Per-tool savings live in
/// [`tool_savings_estimate`]; that's where the real numbers come
/// from once each handler starts calling [`record_tool`].
pub const TOKENS_PER_CALL_EST: u64 = 2500;

/// Per-tool token-savings estimates. Derived from the
/// dogfood sessions logged in `~/.codescope/insight.jsonl` — take
/// the median bytes returned by the native tool, divide by 4
/// (chars→tokens), and compare to the median bytes an LLM would
/// have to read to answer the same question with Read/Grep. These
/// are rough but directionally correct.
pub fn tool_savings_estimate(name: &str) -> u64 {
    match name {
        // Big wins — multi-file / transitive work that would need
        // dozens of reads and a lot of token-expensive thinking.
        "impact_analysis" => 50_000,
        "code_health" => 20_000,
        "type_hierarchy" => 15_000,
        "refactor" => 12_000,
        "ask" => 10_000,
        "search" => 8_000,
        "find_callers" | "find_callees" => 6_000,
        "context_bundle" => 5_000,
        "semantic_search" => 4_000,
        // Medium wins — single-file / graph lookups.
        "knowledge" => 3_500,
        "find_function" | "file_entities" | "suggest_structure" => 3_000,
        "http_analysis" => 3_000,
        "edit_preflight" => 3_000,
        "contributors" => 2_500,
        "conversations" => 2_500,
        "community_detection" => 2_500,
        "skills" => 2_000,
        "memory" => 2_000,
        "manage_adr" => 2_000,
        "lint" => 2_000,
        "capture_insight" => 1_500,
        "search_indexed" => 3_000,
        "fetch_and_index" | "index_content" => 1_500,
        // Small wins — admin / status / raw.
        "graph_stats" | "supported_languages" | "api_changelog" => 500,
        "project" | "index_status" | "index_codebase" => 500,
        "retrieve_archived" => 500,
        "raw_query" => 1_000,
        "sandbox_run" => 1_000,
        "sync_git_history" => 1_000,
        "export_obsidian" => 1_000,
        "embed_functions" => 1_000,
        // Unknown tool — use the generic per-call estimate.
        _ => TOKENS_PER_CALL_EST,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct GainState {
    pub total_calls: u64,
    pub first_used: Option<String>,
    pub last_used: Option<String>,
    /// Per-tool-name counts. Present from v0.8.4 onward; older
    /// state files load with an empty map and the `gain` CLI
    /// falls back to the generic estimate for their totals.
    #[serde(default)]
    pub per_tool: BTreeMap<String, u64>,
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

/// Per-tool in-memory counter drained by `flush()`. Guarded by a
/// `Mutex` because `BTreeMap` isn't atomic; contention is low
/// (one bump per tool call, ~50 µs) so this is fine.
static PER_TOOL: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());

/// Bump the in-memory counter. The calling tool does NOT wait on
/// I/O. Use `flush()` occasionally (or at process shutdown) to
/// persist.
pub fn record_call() {
    IN_MEMORY.fetch_add(1, Ordering::Relaxed);
}

/// Record a tool call attributed to `name`. Increments both the
/// total counter and the per-tool bucket so `codescope gain` can
/// break savings down by tool. Equivalent to `record_call()` when
/// the tool name is unknown — but the downstream aggregation
/// bucket is lost, so prefer this one wherever possible.
pub fn record_tool(name: &str) {
    IN_MEMORY.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut m) = PER_TOOL.lock() {
        *m.entry(name.to_string()).or_insert(0) += 1;
    }
}

/// Persist the in-memory counter into the on-disk state file.
/// Safe to call concurrently — uses tmp+rename for atomicity.
pub async fn flush() -> anyhow::Result<()> {
    let added = IN_MEMORY.swap(0, Ordering::Relaxed);
    let per_tool_delta = match PER_TOOL.lock() {
        Ok(mut m) => std::mem::take(&mut *m),
        Err(_) => BTreeMap::new(),
    };
    if added == 0 && per_tool_delta.is_empty() {
        return Ok(());
    }
    let mut state = load().unwrap_or_default();
    state.total_calls += added;
    for (k, v) in per_tool_delta {
        *state.per_tool.entry(k).or_insert(0) += v;
    }
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

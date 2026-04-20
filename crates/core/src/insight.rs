//! Per-call insight log — `codescope insight`.
//!
//! Every MCP tool call appends one JSON line to
//! `~/.codescope/insight.jsonl`. Aggregates (calls per repo,
//! calls per hour, top tools if we know them) are computed on
//! read, so we keep the hot-path cost to a single buffered
//! write. Rotation kicks in at 50 MB — we keep one `.1` backup.
//!
//! Granularity: we record `{ts, repo}` at the `ctx()` entry point.
//! Per-tool name isn't available from `ctx()` without touching
//! all 52 handlers; that's v2. For v1 the repo + hour histogram
//! is already enough to draw the "15+ metrics" dashboard
//! context-mode makes look valuable.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

/// What lives on one line of `insight.jsonl`. Backward-compatible:
/// older events without a `kind` / `session_id` / `detail` default
/// via serde so the v0 logs still parse.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    pub ts: u64,
    pub repo: String,
    /// CMX-02 — event kind. Omitted on legacy events; defaults to
    /// `tool_call` to match the original semantics.
    #[serde(default = "default_kind")]
    pub kind: EventKind,
    /// Stable per-process id so the UI can group events by session
    /// without heuristic gap-detection. Generated once on MCP
    /// server startup and reused for every event in that process.
    #[serde(default)]
    pub session_id: String,
    /// Free-form context the kind wants to carry (file path for
    /// `file_edit`, error message for `error`, …). Kept short —
    /// anything bigger belongs in SurrealDB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    ToolCall,
    FileEdit,
    Error,
}

fn default_kind() -> EventKind {
    EventKind::ToolCall
}

/// Return a process-stable session id. Computed once from PID +
/// boot-ns on first call, cached for reuse. No UUID dep needed.
pub fn session_id() -> &'static str {
    use std::sync::OnceLock;
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        format!("{pid:x}-{nanos:x}")
    })
}

fn log_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("insight.jsonl")
}

const ROTATE_BYTES: u64 = 50 * 1024 * 1024; // 50 MB

/// Append a `tool_call` event. Kept for backward compatibility
/// with the v0 call sites; new code should use
/// [`record_kind`] so the event kind is explicit.
pub fn record_event(repo: impl Into<String>) {
    record_kind(repo, EventKind::ToolCall, None);
}

/// Append an event of any kind. Fail-soft on I/O — insight is
/// observability, not correctness.
pub fn record_kind(repo: impl Into<String>, kind: EventKind, detail: Option<String>) {
    let ev = Event {
        ts: now_secs(),
        repo: repo.into(),
        kind,
        session_id: session_id().to_string(),
        detail,
    };
    let Ok(line) = serde_json::to_string(&ev) else {
        return;
    };
    let p = log_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Rotate if the file has outgrown the limit. Cheap stat call;
    // happens after every write but matters in practice maybe once
    // every few weeks per user.
    if let Ok(meta) = std::fs::metadata(&p) {
        if meta.len() > ROTATE_BYTES {
            let backup = p.with_extension("jsonl.1");
            let _ = std::fs::rename(&p, &backup);
        }
    }
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
    else {
        return;
    };
    let _ = writeln!(f, "{line}");
}

/// Iterate every event from both the current log and the single
/// rotated backup (if present). Returns oldest-first.
pub fn load_all() -> Vec<Event> {
    let mut events = Vec::new();
    let backup = log_path().with_extension("jsonl.1");
    for p in [backup, log_path()] {
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(ev) = serde_json::from_str::<Event>(line) {
                events.push(ev);
            }
        }
    }
    events.sort_by_key(|e| e.ts);
    events
}

/// Pre-computed rollups used by the CLI and web view.
#[derive(Serialize, Debug, Default)]
pub struct Summary {
    pub total_calls: u64,
    pub repos: BTreeMap<String, u64>,
    pub hours: BTreeMap<String, u64>,
    pub first_ts: Option<u64>,
    pub last_ts: Option<u64>,
    /// CMX-02 — counts by event kind, so the Insight / Session
    /// views can split tool_call vs file_edit vs error.
    pub by_kind: BTreeMap<String, u64>,
}

/// Aggregate the raw event list into headline metrics.
pub fn summarise(events: &[Event]) -> Summary {
    let mut s = Summary::default();
    for ev in events {
        s.total_calls += 1;
        *s.repos.entry(ev.repo.clone()).or_insert(0) += 1;
        let hour_key = bucket_hour(ev.ts);
        *s.hours.entry(hour_key).or_insert(0) += 1;
        let kind_key = match ev.kind {
            EventKind::ToolCall => "tool_call",
            EventKind::FileEdit => "file_edit",
            EventKind::Error => "error",
        };
        *s.by_kind.entry(kind_key.into()).or_insert(0) += 1;
        s.first_ts.get_or_insert(ev.ts);
        s.last_ts = Some(ev.ts);
    }
    s
}

/// CMX-02 — pre-rolled "what happened in the last few sessions"
/// for the Dream / Session UI. Each session is a contiguous run
/// of events sharing the same `session_id`; we emit the most
/// recent `n` sessions oldest-first inside each (so playback
/// feels like a tape).
#[derive(Serialize, Debug, Default, Clone)]
pub struct SessionRecap {
    pub session_id: String,
    pub started_at: u64,
    pub ended_at: u64,
    pub event_count: u64,
    pub repos: Vec<String>,
    pub kinds: BTreeMap<String, u64>,
    /// Last 20 events verbatim — enough to draw a timeline card
    /// without dragging tens of thousands of rows into the UI.
    pub tail: Vec<Event>,
}

/// Build `n` most-recent session recaps from the raw event list.
/// Events without a session_id (legacy v0 logs) are bucketed
/// under the pseudo-session `"legacy"` so they don't disappear.
pub fn recent_sessions(events: &[Event], n: usize) -> Vec<SessionRecap> {
    if events.is_empty() {
        return Vec::new();
    }
    let mut by_session: BTreeMap<String, SessionRecap> = BTreeMap::new();
    for ev in events {
        let sid = if ev.session_id.is_empty() {
            "legacy".to_string()
        } else {
            ev.session_id.clone()
        };
        let recap = by_session
            .entry(sid.clone())
            .or_insert_with(|| SessionRecap {
                session_id: sid,
                started_at: ev.ts,
                ended_at: ev.ts,
                event_count: 0,
                repos: Vec::new(),
                kinds: BTreeMap::new(),
                tail: Vec::new(),
            });
        recap.event_count += 1;
        if ev.ts < recap.started_at {
            recap.started_at = ev.ts;
        }
        if ev.ts > recap.ended_at {
            recap.ended_at = ev.ts;
        }
        if !recap.repos.contains(&ev.repo) {
            recap.repos.push(ev.repo.clone());
        }
        let kk = match ev.kind {
            EventKind::ToolCall => "tool_call",
            EventKind::FileEdit => "file_edit",
            EventKind::Error => "error",
        };
        *recap.kinds.entry(kk.into()).or_insert(0) += 1;
        recap.tail.push(ev.clone());
    }
    // Keep only the last 20 events per session for UI payload; if
    // the caller wants everything they can still read load_all().
    for r in by_session.values_mut() {
        if r.tail.len() > 20 {
            let start = r.tail.len() - 20;
            r.tail = r.tail[start..].to_vec();
        }
    }
    let mut out: Vec<SessionRecap> = by_session.into_values().collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.ended_at));
    out.truncate(n);
    out
}

fn bucket_hour(ts: u64) -> String {
    // Keep the format exactly 13 chars (YYYY-MM-DDTHH) so the
    // BTreeMap's natural sort = chronological. Cheaper than parsing
    // a date type just for a bucket key.
    let (y, mo, d, h, _, _) = epoch_to_ymdhms(ts);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Epoch seconds → (Y, M, D, H, M, S) UTC. Same Hinnant
/// civil_from_days as the supervisor/gain modules — keep a single
/// private copy each so they stay dep-free.
pub fn epoch_to_ymdhms(ts: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (ts / 86_400) as i64;
    let rem = ts % 86_400;
    let h = (rem / 3600) as u32;
    let m = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mo = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = y + if mo <= 2 { 1 } else { 0 };
    (y, mo, d, h, m, s)
}

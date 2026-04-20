//! `codescope session` — CMX-02 session recap.
//!
//! Reads the insight event log and prints the most recent N
//! sessions: start/end time, duration, repos touched, per-kind
//! counts, and the last few events as a timeline. Pair with
//! `codescope insight` for the aggregate view.

use anyhow::Result;
use codescope_core::insight::{self, EventKind};

pub async fn run(limit: usize) -> Result<()> {
    let n = limit.clamp(1, 20);
    let events = insight::load_all();
    if events.is_empty() {
        println!("No session events yet. Make a tool call via an MCP client first.");
        return Ok(());
    }
    let recaps = insight::recent_sessions(&events, n);
    if recaps.is_empty() {
        println!("No session data.");
        return Ok(());
    }

    println!();
    println!(
        "  \x1b[1mcodescope session\x1b[0m  (last {} of {})",
        recaps.len(),
        n
    );
    println!("  ──────────────────");

    for r in &recaps {
        let started = ymdhm(r.started_at);
        let ended = ymdhm(r.ended_at);
        let dur = format_duration(r.ended_at.saturating_sub(r.started_at));
        println!();
        println!(
            "  \x1b[35m{}\x1b[0m  {} → {} ({} dur)",
            r.session_id, started, ended, dur
        );
        println!(
            "    events: {:>4}   repos: {}",
            r.event_count,
            if r.repos.is_empty() {
                "—".into()
            } else {
                r.repos.join(", ")
            }
        );
        let mut kinds: Vec<(&String, &u64)> = r.kinds.iter().collect();
        kinds.sort_by(|a, b| b.1.cmp(a.1));
        if !kinds.is_empty() {
            let kstr = kinds
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("  ");
            println!("    by kind: {kstr}");
        }
        if !r.tail.is_empty() {
            println!("    recent:");
            for ev in &r.tail {
                let icon = match ev.kind {
                    EventKind::ToolCall => "›",
                    EventKind::FileEdit => "✎",
                    EventKind::Error => "✗",
                };
                let detail = ev
                    .detail
                    .as_deref()
                    .map(|s| format!("  {s}"))
                    .unwrap_or_default();
                println!("      {icon} {}  {}{}", hms(ev.ts), ev.repo, detail);
            }
        }
    }
    println!();
    Ok(())
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 3600 {
        return format!("{}m{}s", secs / 60, secs % 60);
    }
    let h = secs / 3600;
    let rem = secs % 3600;
    format!("{h}h{}m", rem / 60)
}

fn ymdhm(ts: u64) -> String {
    let (y, mo, d, h, m, _) = insight::epoch_to_ymdhms(ts);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}")
}

fn hms(ts: u64) -> String {
    let (_, _, _, h, m, s) = insight::epoch_to_ymdhms(ts);
    format!("{h:02}:{m:02}:{s:02}")
}

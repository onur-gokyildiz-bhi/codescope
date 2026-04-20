//! `codescope insight` — formatted view of the per-call log.
//!
//! Reads `~/.codescope/insight.jsonl` (oldest-first), aggregates
//! via `codescope_core::insight::summarise`, and prints:
//!
//! * Headline: total calls + window (first → last).
//! * Per-repo breakdown, sorted by calls desc.
//! * Hourly histogram (last 24 non-empty hours) as a mini
//!   sparkline so you see when you actually use codescope.
//!
//! No flags for v1 — the `--limit`/`--since` knobs can come when
//! someone complains.

use anyhow::Result;
use codescope_core::insight;

pub async fn run() -> Result<()> {
    let events = insight::load_all();
    if events.is_empty() {
        println!("No insight events yet. Make a tool call via an MCP client first.");
        return Ok(());
    }
    let s = insight::summarise(&events);

    println!();
    println!("  \x1b[1mcodescope insight\x1b[0m");
    println!("  ──────────────────");
    println!("  Total tool calls:  {}", format_num(s.total_calls));
    if let (Some(first), Some(last)) = (s.first_ts, s.last_ts) {
        let span_days = (last - first) / 86_400;
        println!("  First call:        {}", ymdhm(first));
        println!("  Last call:         {}", ymdhm(last));
        if span_days > 0 {
            println!(
                "  Calls per day:     ~{}",
                format_num(s.total_calls / span_days.max(1))
            );
        }
    }

    // Per-repo breakdown
    println!();
    println!("  \x1b[1mBy repo\x1b[0m");
    let mut repos: Vec<(&String, &u64)> = s.repos.iter().collect();
    repos.sort_by(|a, b| b.1.cmp(a.1));
    let max = *repos.first().map(|(_, n)| *n).unwrap_or(&1);
    for (name, n) in repos.iter().take(20) {
        let bar = bar(**n, max, 18);
        println!(
            "  {:24}  {:>6}  {}",
            truncate(name, 24),
            format_num(**n),
            bar
        );
    }

    // Hourly sparkline — last 24 non-empty hours
    let mut hours: Vec<(&String, &u64)> = s.hours.iter().collect();
    hours.sort_by_key(|(k, _)| (*k).clone());
    let tail: Vec<_> = hours.iter().rev().take(24).collect();
    if !tail.is_empty() {
        println!();
        println!("  \x1b[1mRecent activity\x1b[0m (last 24 active hours, oldest → newest)");
        let spark_max = tail.iter().map(|(_, n)| **n).max().unwrap_or(1);
        let mut line = String::new();
        for (_, n) in tail.iter().rev() {
            line.push(spark_char(**n, spark_max));
        }
        println!("  {line}");
    }
    println!();
    Ok(())
}

fn format_num(n: u64) -> String {
    let s = n.to_string();
    let chars: Vec<char> = s.chars().rev().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(*c);
    }
    out.chars().rev().collect()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n - 1).collect();
    out.push('…');
    out
}

fn bar(value: u64, max: u64, width: usize) -> String {
    let filled = ((value as f64 / max.max(1) as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    let mut out = String::with_capacity(width);
    out.push_str(&"█".repeat(filled));
    out.push_str(&"░".repeat(width - filled));
    out
}

fn spark_char(value: u64, max: u64) -> char {
    const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if value == 0 {
        return ' ';
    }
    let idx = ((value as f64 / max.max(1) as f64) * (LEVELS.len() as f64 - 1.0)).round() as usize;
    LEVELS[idx.min(LEVELS.len() - 1)]
}

fn ymdhm(ts: u64) -> String {
    let (y, mo, d, h, m, _) = insight::epoch_to_ymdhms(ts);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

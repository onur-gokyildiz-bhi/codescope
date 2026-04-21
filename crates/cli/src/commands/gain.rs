//! `codescope gain` — show cumulative token-savings from MCP tool
//! calls. Reads `~/.codescope/gain.json` written by the MCP server.

use anyhow::Result;

pub async fn run() -> Result<()> {
    // Flush any in-process counter first — useful if this binary
    // happens to be the MCP process too (stdio mode with a brief
    // run). Normally a no-op.
    let _ = codescope_core::gain::flush().await;

    let Some(state) = codescope_core::gain::load() else {
        println!("No gain data yet. Make a tool call via an MCP client first.");
        return Ok(());
    };
    let calls = state.total_calls;

    // Per-tool breakdown — each tool has its own token-savings
    // estimate (impact_analysis ≈ 50 000, graph_stats ≈ 500). The
    // unattributed fallback uses the generic TOKENS_PER_CALL_EST.
    let attributed_calls: u64 = state.per_tool.values().sum();
    let unattributed = calls.saturating_sub(attributed_calls);
    let per_tool_saved: u64 = state
        .per_tool
        .iter()
        .map(|(name, c)| c * codescope_core::gain::tool_savings_estimate(name))
        .sum();
    let unattributed_saved = unattributed * codescope_core::gain::TOKENS_PER_CALL_EST;
    let estimated = per_tool_saved + unattributed_saved;

    println!();
    println!("  \x1b[1mcodescope gain\x1b[0m");
    println!("  ───────────────");
    println!("  Total tool calls:         {}", format_num(calls));
    println!(
        "  Estimated tokens saved:   \x1b[32m~{}\x1b[0m",
        format_num(estimated),
    );
    if let Some(first) = state.first_used.as_deref() {
        println!("  Active since:             {}", human_time(first));
    }
    if let Some(last) = state.last_used.as_deref() {
        println!("  Last call:                {}", human_time(last));
    }

    if !state.per_tool.is_empty() {
        // Sort by savings contribution, descending — the top lines
        // are where the value is coming from.
        let mut rows: Vec<(&String, u64, u64, u64)> = state
            .per_tool
            .iter()
            .map(|(name, count)| {
                let per = codescope_core::gain::tool_savings_estimate(name);
                (name, *count, per, count * per)
            })
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.3));

        println!();
        println!("  \x1b[1mBy tool\x1b[0m");
        println!(
            "  \x1b[2m{:<24} {:>6} {:>8} {:>12}\x1b[0m",
            "tool", "calls", "≈ /call", "total saved"
        );
        for (name, count, per, saved) in rows.iter().take(15) {
            println!(
                "  {:<24} {:>6} {:>8} {:>12}",
                truncate(name, 24),
                format_num(*count),
                format_num(*per),
                format_num(*saved)
            );
        }
        if rows.len() > 15 {
            println!("  \x1b[2m… {} more tools\x1b[0m", rows.len() - 15);
        }
        if unattributed > 0 {
            println!(
                "  \x1b[2m{:<24} {:>6} {:>8} {:>12}\x1b[0m",
                "(legacy unattributed)",
                format_num(unattributed),
                format_num(codescope_core::gain::TOKENS_PER_CALL_EST),
                format_num(unattributed_saved)
            );
        }
    }

    println!();
    println!("  \x1b[2mEstimate = Σ (per-tool calls × per-tool token estimate).\x1b[0m");
    println!(
        "  \x1b[2mPer-tool numbers come from `gain::tool_savings_estimate` — rough but directional.\x1b[0m"
    );
    println!();
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
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

/// The stored timestamp is epoch-seconds (see gain.rs). Convert to
/// a local human-ish string without pulling chrono. UTC with
/// second precision is enough for "active since".
fn human_time(s: &str) -> String {
    let Ok(secs) = s.parse::<u64>() else {
        return s.to_string();
    };
    // Handcrafted UTC formatter — "DATE TIME UTC".
    // 86400 = seconds/day. Approximate year / month without leap
    // handling is fine for display purposes.
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let (y, mo, d) = epoch_days_to_ymd(days as i64);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

/// Days since 1970-01-01 → (year, month, day). Uses Howard
/// Hinnant's civil_from_days algorithm — correct through 9999 CE.
fn epoch_days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

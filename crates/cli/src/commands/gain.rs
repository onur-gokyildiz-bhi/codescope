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
    let estimated = calls * codescope_core::gain::TOKENS_PER_CALL_EST;

    println!();
    println!("  \x1b[1mcodescope gain\x1b[0m");
    println!("  ───────────────");
    println!("  Total tool calls:         {}", format_num(calls));
    println!(
        "  Estimated tokens saved:   \x1b[32m~{}\x1b[0m  (≈ {} / call)",
        format_num(estimated),
        codescope_core::gain::TOKENS_PER_CALL_EST
    );
    if let Some(first) = state.first_used.as_deref() {
        println!("  Active since:             {}", human_time(first));
    }
    if let Some(last) = state.last_used.as_deref() {
        println!("  Last call:                {}", human_time(last));
    }
    println!();
    println!(
        "  \x1b[2mEstimate = total_calls × {}. See `docs/llms-full.txt` for the derivation.\x1b[0m",
        codescope_core::gain::TOKENS_PER_CALL_EST
    );
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

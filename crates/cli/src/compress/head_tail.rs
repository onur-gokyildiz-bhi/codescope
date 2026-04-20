//! `head` / `tail` compressor.
//!
//! The user has already asked for "first N" / "last N" lines,
//! so we don't trim further. We do cap output at a byte limit
//! when the user piped something pathological (e.g.
//! `tail -f /var/log/syslog` accidentally without a count) —
//! prevent a context flood when the command runs amok.

use super::{emit, run_capture, split_full_flag};
use anyhow::Result;

const MAX_BYTES: usize = 64 * 1024; // 64 KB

pub async fn handle(cmd: &str, args: &[String]) -> Result<()> {
    let (full, args) = split_full_flag(args);
    let (stdout, stderr, code) = run_capture(cmd, &args)?;
    if full || stdout.len() <= MAX_BYTES {
        emit(&stdout, &stdout, &stderr, code);
        return Ok(());
    }
    // Truncate at the last newline before MAX_BYTES so we don't
    // cut a multi-byte char or a half-line. Report how much was
    // cut off.
    let cut = stdout[..MAX_BYTES]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(MAX_BYTES);
    let mut out = stdout[..cut].to_string();
    out.push_str(&format!(
        "\x1b[2m… {} more bytes omitted (use --full) …\x1b[0m\n",
        stdout.len() - cut
    ));
    emit(&stdout, &out, &stderr, code);
    Ok(())
}

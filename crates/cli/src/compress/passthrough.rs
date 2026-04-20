//! Fallback for commands we don't specialise on.
//!
//! Runs the command with inherited stdio so output streams live
//! and interactive tools (editors, paginated viewers) behave
//! normally. We don't touch exit codes either — whatever the
//! child does is what `codescope exec` returns.

use anyhow::Result;

pub async fn handle(cmd: &str, args: &[String]) -> Result<()> {
    let status = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| anyhow::anyhow!("failed to spawn `{cmd}`: {e}"))?;
    if let Some(code) = status.code() {
        if code != 0 {
            std::process::exit(code);
        }
    }
    Ok(())
}

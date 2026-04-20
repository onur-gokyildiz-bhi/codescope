//! `codescope hook install [--agent claude-code]` — wires the
//! bash-suggest hook into the chosen agent's settings file.
//! Right now only Claude Code uses a unified PreToolUse hook
//! shape; Cursor / Gemini / Codex each carry their own.
//!
//! The hook scripts themselves live at `hooks/codescope-bash-
//! suggest.{sh,ps1}` in the repo (and in the release archive).
//! This command does two things:
//!
//! 1. Make sure the script is on disk at `~/.codescope/bin/`
//!    (same home the surreal binary uses, so we don't pollute a
//!    general `~/.local/bin/`).
//! 2. Merge a hook entry into `~/.claude/settings.json` that
//!    points at that script. JSON is parsed, updated, and
//!    rewritten — existing entries are preserved.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

const HOOK_SH: &str = include_str!("../../../../hooks/codescope-bash-suggest.sh");
const HOOK_PS1: &str = include_str!("../../../../hooks/codescope-bash-suggest.ps1");

pub async fn run(agent: &str, uninstall: bool) -> Result<()> {
    let agent = agent.to_ascii_lowercase();
    if agent != "claude-code" && agent != "claude" {
        bail!(
            "hook install is only wired for claude-code today; \
             other agents (cursor, gemini-cli, vscode-copilot…) ship hook \
             templates in their own shape. Open an issue if you want one wired."
        );
    }

    let script_path = install_script()?;
    if uninstall {
        uninstall_from_claude_settings(&script_path)?;
        println!("  Uninstalled bash-suggest hook from ~/.claude/settings.json");
        return Ok(());
    }

    install_into_claude_settings(&script_path)?;
    println!();
    println!("  ✓ bash-suggest hook installed");
    println!("    script: {}", script_path.display());
    println!("    settings: ~/.claude/settings.json (PreToolUse → Bash)");
    println!();
    println!("  Restart Claude Code to pick up the hook.");
    println!("  Set CODESCOPE_HOOK_BLOCK=1 in the session env to make matched patterns hard-fail.");
    Ok(())
}

fn install_script() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("bin");
    std::fs::create_dir_all(&dir).ok();
    let path = if cfg!(windows) {
        dir.join("codescope-bash-suggest.ps1")
    } else {
        dir.join("codescope-bash-suggest.sh")
    };
    let body = if cfg!(windows) { HOOK_PS1 } else { HOOK_SH };
    std::fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms)?;
    }
    Ok(path)
}

fn claude_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("settings.json")
}

/// The command string a PreToolUse hook runs. On Windows we wrap
/// the PowerShell invocation so Claude Code's shell dispatcher
/// sees a single executable + args string.
fn hook_command(script: &std::path::Path) -> String {
    if cfg!(windows) {
        format!("pwsh -NoProfile -File \"{}\"", script.display())
    } else {
        script.display().to_string()
    }
}

fn install_into_claude_settings(script: &std::path::Path) -> Result<()> {
    let path = claude_settings_path();
    std::fs::create_dir_all(path.parent().unwrap()).ok();

    let mut root: serde_json::Value = match std::fs::read_to_string(&path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };
    if !root.is_object() {
        root = serde_json::json!({});
    }

    let cmd_str = hook_command(script);
    // Structure: `hooks.PreToolUse[] = { matcher, hooks: [{type, command}] }`.
    let obj = root.as_object_mut().unwrap();
    let hooks = obj
        .entry("hooks".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !hooks.is_object() {
        *hooks = serde_json::json!({});
    }
    let hooks_obj = hooks.as_object_mut().unwrap();
    let pre = hooks_obj
        .entry("PreToolUse".to_string())
        .or_insert_with(|| serde_json::json!([]));
    if !pre.is_array() {
        *pre = serde_json::json!([]);
    }
    let arr = pre.as_array_mut().unwrap();

    // Remove any prior codescope-bash-suggest entry so rerun is
    // idempotent. We match on "codescope-bash-suggest" substring.
    arr.retain(|entry| !entry.to_string().contains("codescope-bash-suggest"));

    let new_entry = serde_json::json!({
        "matcher": "Bash",
        "hooks": [
            { "type": "command", "command": cmd_str }
        ]
    });
    arr.push(new_entry);

    let text = serde_json::to_string_pretty(&root)? + "\n";
    std::fs::write(&path, text)?;
    Ok(())
}

fn uninstall_from_claude_settings(_script: &std::path::Path) -> Result<()> {
    let path = claude_settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(()); // nothing to do
    };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    if let Some(pre) = root
        .get_mut("hooks")
        .and_then(|h| h.get_mut("PreToolUse"))
        .and_then(|p| p.as_array_mut())
    {
        pre.retain(|entry| !entry.to_string().contains("codescope-bash-suggest"));
    }
    std::fs::write(&path, serde_json::to_string_pretty(&root)? + "\n")?;
    Ok(())
}

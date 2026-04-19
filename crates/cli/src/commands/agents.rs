//! Per-agent MCP config writers.
//!
//! `codescope init --agent <agent>` picks one entry from this
//! table and drops the right config at the right path so the chosen
//! agent sees codescope without the user copy-pasting JSON.
//!
//! For each agent we write two things:
//! * **MCP config** — the stdio or HTTP wiring. Path and format vary
//!   per agent (JSON here, TOML there).
//! * **Routing nudge** (optional, only if no hook support) — a small
//!   markdown file (`CLAUDE.md` / `AGENTS.md` / `GEMINI.md`) that
//!   the agent auto-loads and that reminds the model to prefer
//!   codescope tools. Agents with hook support get routing injected
//!   at MCP init via `ServerInfo.instructions` (see CMX-04).
//!
//! Adding a new agent is two steps: append a variant to [`Agent`]
//! and add its `write_config` branch below. Keep the schema close
//! to what the upstream docs publish so refactors don't drift.

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, ValueEnum)]
#[clap(rename_all = "kebab_case")]
pub enum Agent {
    /// Anthropic Claude Code — `.mcp.json` in project root.
    ClaudeCode,
    /// Cursor — `.cursor/mcp.json` in project root.
    Cursor,
    /// Gemini CLI — `~/.gemini/settings.json` (global, merges per project).
    GeminiCli,
    /// VS Code Copilot — `.vscode/mcp.json` in project root.
    VscodeCopilot,
    /// OpenAI Codex CLI — `~/.codex/config.toml`.
    Codex,
    /// Windsurf (Codeium) — `~/.codeium/windsurf/mcp_config.json`.
    Windsurf,
}

impl Agent {
    /// Human-friendly name for logs.
    pub fn display(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "Claude Code",
            Agent::Cursor => "Cursor",
            Agent::GeminiCli => "Gemini CLI",
            Agent::VscodeCopilot => "VS Code Copilot",
            Agent::Codex => "Codex CLI",
            Agent::Windsurf => "Windsurf",
        }
    }
}

/// Per-agent config target — the file we write + a summary line.
pub struct WriteOutcome {
    pub path: PathBuf,
    pub note: String,
}

/// Write the agent's MCP config so `codescope-mcp` is picked up.
///
/// `project_path` is the codebase dir, `repo_name` becomes the
/// `--repo` arg for stdio mode, `mcp_binary` is the resolved
/// `codescope-mcp` path (stdio) or `None` (HTTP/daemon mode).
/// `daemon_port` is only used when `mcp_binary` is None.
pub fn write_config(
    agent: Agent,
    project_path: &Path,
    repo_name: &str,
    mcp_binary: Option<&Path>,
    daemon_port: u16,
) -> Result<WriteOutcome> {
    match agent {
        Agent::ClaudeCode => {
            // `.mcp.json` — Claude Code auto-reads it.
            let path = project_path.join(".mcp.json");
            let body = stdio_or_http_json(mcp_binary, project_path, repo_name, daemon_port);
            write_atomic(&path, &body)?;
            Ok(WriteOutcome {
                path,
                note: "Claude Code reads .mcp.json automatically. Restart the session.".into(),
            })
        }
        Agent::Cursor => {
            // Prefer project-local config so the user can commit it.
            let path = project_path.join(".cursor").join("mcp.json");
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            let body = stdio_or_http_json(mcp_binary, project_path, repo_name, daemon_port);
            write_atomic(&path, &body)?;
            Ok(WriteOutcome {
                path,
                note: "Cursor: restart / reload with Cmd+Shift+P → \"MCP: Reload\".".into(),
            })
        }
        Agent::VscodeCopilot => {
            let path = project_path.join(".vscode").join("mcp.json");
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            // VS Code expects the `servers` key (not `mcpServers`).
            let body = vscode_servers_json(mcp_binary, project_path, repo_name, daemon_port);
            write_atomic(&path, &body)?;
            Ok(WriteOutcome {
                path,
                note: "VS Code Copilot: reload window (Ctrl+Shift+P → \"Reload Window\").".into(),
            })
        }
        Agent::GeminiCli => {
            // Global config — Gemini CLI merges per-project.
            let path = home()?.join(".gemini").join("settings.json");
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            let body = merge_json_object(
                &path,
                "mcpServers",
                "codescope",
                &mcp_entry_json(mcp_binary, project_path, repo_name, daemon_port),
            )?;
            write_atomic(&path, &body)?;
            Ok(WriteOutcome {
                path,
                note: "Gemini CLI: restart. Verify with `/mcp list`.".into(),
            })
        }
        Agent::Codex => {
            // Codex uses TOML, not JSON.
            let path = home()?.join(".codex").join("config.toml");
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            let body = codex_toml(mcp_binary, project_path, repo_name, daemon_port, &path)?;
            write_atomic(&path, &body)?;
            Ok(WriteOutcome {
                path,
                note: "Codex CLI: restart. See docs for `codex_hooks` flag if you want hook-based routing.".into(),
            })
        }
        Agent::Windsurf => {
            let path = home()?
                .join(".codeium")
                .join("windsurf")
                .join("mcp_config.json");
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            let body = stdio_or_http_json(mcp_binary, project_path, repo_name, daemon_port);
            write_atomic(&path, &body)?;
            Ok(WriteOutcome {
                path,
                note: "Windsurf: Cascade → Settings → reload MCP config.".into(),
            })
        }
    }
}

// ── body builders ───────────────────────────────────────────────

/// The `{ command, args }` object for one codescope MCP server
/// entry. Reused by agents that want just the inner entry (Gemini).
fn mcp_entry_json(
    mcp_binary: Option<&Path>,
    project_path: &Path,
    repo_name: &str,
    daemon_port: u16,
) -> String {
    if let Some(bin) = mcp_binary {
        let bin_s = bin.to_string_lossy().replace('\\', "\\\\");
        let path_s = project_path.to_string_lossy().replace('\\', "\\\\");
        format!(
            r#"{{
  "command": "{bin_s}",
  "args": ["{path_s}", "--repo", "{repo_name}", "--auto-index"]
}}"#
        )
    } else {
        format!(
            r#"{{
  "type": "http",
  "url": "http://127.0.0.1:{daemon_port}/mcp/{repo_name}"
}}"#
        )
    }
}

/// Standard `{"mcpServers": { "codescope": {…} }}` shape — what
/// every Claude-family agent expects.
fn stdio_or_http_json(
    mcp_binary: Option<&Path>,
    project_path: &Path,
    repo_name: &str,
    daemon_port: u16,
) -> String {
    let entry = mcp_entry_json(mcp_binary, project_path, repo_name, daemon_port);
    format!("{{\n  \"mcpServers\": {{\n    \"codescope\": {entry}\n  }}\n}}\n")
}

/// VS Code variant — uses `servers` not `mcpServers`.
fn vscode_servers_json(
    mcp_binary: Option<&Path>,
    project_path: &Path,
    repo_name: &str,
    daemon_port: u16,
) -> String {
    let entry = mcp_entry_json(mcp_binary, project_path, repo_name, daemon_port);
    format!("{{\n  \"servers\": {{\n    \"codescope\": {entry}\n  }}\n}}\n")
}

/// Codex's TOML config at `~/.codex/config.toml`. If the file
/// already exists we append a new `[mcp_servers.codescope]` block
/// rather than stomp the whole file.
fn codex_toml(
    mcp_binary: Option<&Path>,
    project_path: &Path,
    repo_name: &str,
    daemon_port: u16,
    out_path: &Path,
) -> Result<String> {
    let existing = std::fs::read_to_string(out_path).unwrap_or_default();
    // Drop any previous codescope block so re-running init doesn't
    // accumulate duplicates. A real TOML edit would use `toml_edit`;
    // we take the narrow shortcut because our marker is stable.
    const MARKER: &str = "[mcp_servers.codescope]";
    let mut out = String::with_capacity(existing.len() + 400);
    let mut skipping = false;
    for line in existing.lines() {
        if line.trim_start().starts_with(MARKER) {
            skipping = true;
            continue;
        }
        if skipping {
            // A new top-level section ends the skip.
            if line.starts_with('[') {
                skipping = false;
                out.push_str(line);
                out.push('\n');
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(MARKER);
    out.push('\n');
    match mcp_binary {
        Some(bin) => {
            let bin_s = bin.to_string_lossy().replace('\\', "/");
            let path_s = project_path.to_string_lossy().replace('\\', "/");
            out.push_str(&format!("command = \"{bin_s}\"\n"));
            out.push_str(&format!(
                "args = [\"{path_s}\", \"--repo\", \"{repo_name}\", \"--auto-index\"]\n"
            ));
        }
        None => {
            // Codex TOML MCP supports command-only; HTTP mode is
            // not a first-class Codex feature yet. Fall back to a
            // stdio command that talks HTTP via a thin wrapper:
            // users can override. For now, comment it out.
            out.push_str(&format!(
                "# HTTP mode unsupported by Codex — set daemon_port={daemon_port} manually.\n"
            ));
            out.push_str("command = \"codescope-mcp\"\n");
        }
    }
    Ok(out)
}

/// Merge a new `{ key: {child: value_json} }` entry into an
/// existing JSON settings file (used for Gemini's shared
/// `~/.gemini/settings.json`). Parses + rewrites; if parsing fails
/// we fall back to writing a fresh minimal file.
fn merge_json_object(
    path: &Path,
    root_key: &str,
    child_key: &str,
    child_json: &str,
) -> Result<String> {
    let existing = std::fs::read_to_string(path).unwrap_or_else(|_| "{}".into());
    let mut root: serde_json::Value =
        serde_json::from_str(existing.trim().trim_start_matches('\u{feff}'))
            .unwrap_or(serde_json::json!({}));
    if !root.is_object() {
        root = serde_json::json!({});
    }
    let child: serde_json::Value = serde_json::from_str(child_json)
        .with_context(|| format!("parse child entry for {child_key}"))?;

    let obj = root.as_object_mut().unwrap();
    let entry = obj
        .entry(root_key.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(o) = entry.as_object_mut() {
        o.insert(child_key.to_string(), child);
    }
    Ok(serde_json::to_string_pretty(&root)? + "\n")
}

fn write_atomic(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename into {}", path.display()))?;
    Ok(())
}

fn home() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no home dir"))
}

/// Guard against accidental misuse from code that converts a
/// positional name string. Accepts the CLI values from `--agent`.
pub fn parse_name(name: &str) -> Result<Agent> {
    match name.to_ascii_lowercase().as_str() {
        "claude-code" | "claude" => Ok(Agent::ClaudeCode),
        "cursor" => Ok(Agent::Cursor),
        "gemini-cli" | "gemini" => Ok(Agent::GeminiCli),
        "vscode-copilot" | "copilot" | "vscode" => Ok(Agent::VscodeCopilot),
        "codex" => Ok(Agent::Codex),
        "windsurf" => Ok(Agent::Windsurf),
        _ => bail!("unknown agent '{name}'"),
    }
}

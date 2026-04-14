use anyhow::Result;
use codescope_core::graph::builder::GraphBuilder;
use codescope_core::parser::CodeParser;
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(
    project_path: PathBuf,
    repo_name: &str,
    db_path: Option<PathBuf>,
    use_daemon: bool,
    daemon_port: u16,
) -> Result<()> {
    use std::time::Instant;

    let project_path =
        std::fs::canonicalize(&project_path).unwrap_or_else(|_| project_path.clone());
    // Strip Windows extended-length prefix (\\?\)
    let project_path = {
        let s = project_path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            PathBuf::from(stripped)
        } else {
            project_path
        }
    };

    let repo_name = project_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| repo_name.to_string());

    println!("🔧 Initializing Codescope for '{}'...\n", repo_name);

    // Step 1: Detect or start daemon, OR find stdio binary
    let mcp_json_path = project_path.join(".mcp.json");
    let project_path_str = project_path.to_string_lossy().replace('\\', "\\\\");

    let daemon_running = is_daemon_running(daemon_port);

    let mcp_json = if use_daemon || daemon_running {
        // Daemon mode — HTTP MCP config
        if !daemon_running {
            println!("🚀 Starting codescope daemon on port {}...", daemon_port);
            if let Err(e) = crate::commands::daemon::start(daemon_port) {
                eprintln!("⚠ Failed to start daemon: {e}. Falling back to stdio mode.");
                return run_stdio_init(project_path, repo_name, db_path).await;
            }
            // Wait for daemon to be ready
            for _ in 0..20 {
                std::thread::sleep(std::time::Duration::from_millis(250));
                if is_daemon_running(daemon_port) {
                    break;
                }
            }
        } else {
            println!(
                "✓ Daemon already running on port {} — using HTTP MCP config",
                daemon_port
            );
        }

        format!(
            r#"{{
  "mcpServers": {{
    "codescope": {{
      "type": "http",
      "url": "http://127.0.0.1:{}/mcp"
    }}
  }}
}}
"#,
            daemon_port
        )
    } else {
        // Stdio mode (default)
        let mcp_binary = find_mcp_binary();
        if mcp_binary.is_none() {
            eprintln!("⚠ codescope-mcp binary not found. Run 'codescope install' first,");
            eprintln!("  or build with: cargo build --release -p codescope-mcp");
        }
        let mcp_cmd = mcp_binary
            .as_deref()
            .unwrap_or_else(|| std::path::Path::new("codescope-mcp"));
        let mcp_cmd_str = mcp_cmd.to_string_lossy().replace('\\', "\\\\");

        format!(
            r#"{{
  "mcpServers": {{
    "codescope": {{
      "command": "{}",
      "args": ["{}", "--repo", "{}", "--auto-index"]
    }}
  }}
}}
"#,
            mcp_cmd_str, project_path_str, repo_name
        )
    };

    if mcp_json_path.exists() {
        println!("📄 .mcp.json already exists — updating...");
    } else {
        println!("📄 Creating .mcp.json...");
    }
    std::fs::write(&mcp_json_path, &mcp_json)?;
    println!("   {}", mcp_json_path.display());

    // Step 3: Create .claude/rules/codescope-mandatory.md so Claude Code
    // is required to use codescope MCP tools instead of Read/Grep.
    let rules_dir = project_path.join(".claude").join("rules");
    let rule_path = rules_dir.join("codescope-mandatory.md");
    if !rule_path.exists() {
        std::fs::create_dir_all(&rules_dir)?;
        std::fs::write(
            &rule_path,
            include_str!("../../../../.claude/rules/codescope-mandatory.md"),
        )?;
        println!("📏 Created .claude/rules/codescope-mandatory.md");
    } else {
        println!("📏 .claude/rules/codescope-mandatory.md already exists");
    }

    // Step 3b: Ensure .claude/rules/ is not gitignored (but rest of .claude is).
    // If .gitignore has ".claude/" but not "!.claude/rules/", add the negation.
    let gitignore_path_check = project_path.join(".gitignore");
    if gitignore_path_check.exists() {
        let gi_content = std::fs::read_to_string(&gitignore_path_check).unwrap_or_default();
        if gi_content.contains(".claude") && !gi_content.contains("!.claude/rules") {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&gitignore_path_check)?;
            use std::io::Write;
            writeln!(
                f,
                "\n# Allow Claude Code rules to be committed\n!.claude/rules/"
            )?;
            println!("📝 Added !.claude/rules/ to .gitignore");
        }
    }

    // Step 4: Add .mcp.json to .gitignore if not already there
    let gitignore_path = project_path.join(".gitignore");
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
        if !content.contains(".mcp.json") {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&gitignore_path)?;
            use std::io::Write;
            writeln!(
                f,
                "\n# Codescope MCP config (user-specific paths)\n.mcp.json"
            )?;
            println!("📝 Added .mcp.json to .gitignore");
        }
    }

    // Step 4: First index
    println!("\n📊 Indexing codebase...");
    let start = Instant::now();
    let db = connect_db(db_path, &repo_name).await?;
    let builder = GraphBuilder::new(db.clone());
    let parser = CodeParser::new();

    // Discover files using ignore crate (respects .gitignore)
    let walker = ignore::WalkBuilder::new(&project_path)
        .hidden(false)
        .git_ignore(true)
        .build();

    let all_files: Vec<PathBuf> = walker
        .flatten()
        .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|e| {
            let fp = e.path();
            let ext = fp.extension().and_then(|e| e.to_str()).unwrap_or("");
            let fname = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            (parser.supports_extension(ext) || parser.supports_filename(fname))
                && !codescope_core::parser::should_skip_file(fp)
        })
        .map(|e| e.into_path())
        .collect();

    let mut file_count = 0;
    let mut entity_count = 0;
    let mut relation_count = 0;

    for file_path in &all_files {
        let rel_path = file_path.strip_prefix(&project_path).unwrap_or(file_path);
        let rel_str = rel_path.to_string_lossy().replace('\\', "/");

        if let Ok((entities, relations)) = parser.parse_source(
            std::path::Path::new(&rel_str),
            &std::fs::read_to_string(file_path).unwrap_or_default(),
            &repo_name,
        ) {
            if let Err(e) = builder.insert_entities(&entities).await {
                tracing::warn!("Entity insert failed: {e}");
            }
            if let Err(e) = builder.insert_relations(&relations).await {
                tracing::warn!("Relation insert failed: {e}");
            }
            entity_count += entities.len();
            relation_count += relations.len();
            file_count += 1;
        }

        if file_count % 100 == 0 && file_count > 0 {
            eprint!("\r   ... {} files indexed", file_count);
        }
    }
    if file_count >= 100 {
        eprintln!();
    }

    // Resolve call targets
    if let Err(e) = builder.resolve_call_targets(&repo_name).await {
        tracing::warn!("Resolve call targets failed: {e}");
    }

    let elapsed = start.elapsed();
    println!(
        "   {} files, {} entities, {} relations ({:.1}s)",
        file_count,
        entity_count,
        relation_count,
        elapsed.as_secs_f64()
    );

    // Step 5: Summary
    println!("\n✅ Codescope initialized!\n");
    if use_daemon || is_daemon_running(daemon_port) {
        println!("   🚀 Daemon mode — MCP + Web UI share one process (no lock conflicts)");
        println!("   Web UI: http://localhost:{}/", daemon_port);
        println!("   MCP:    http://localhost:{}/mcp", daemon_port);
        println!("   Stop:   codescope stop --port {}", daemon_port);
    } else {
        println!("   Next time you open this project in Claude Code,");
        println!("   Codescope starts automatically with 57 MCP tools.");
        println!("   Tip: use --daemon flag to avoid lock conflicts between web UI and MCP.");
    }
    println!("\n   Manual commands:");
    println!("     codescope search <query> --repo {}", repo_name);
    println!("     codescope stats --repo {}", repo_name);

    Ok(())
}

/// Check if a codescope daemon is running on the given port.
fn is_daemon_running(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        std::time::Duration::from_millis(300),
    )
    .is_ok()
}

/// Fallback: stdio mode init if daemon start fails.
async fn run_stdio_init(
    project_path: PathBuf,
    repo_name: String,
    db_path: Option<PathBuf>,
) -> Result<()> {
    // Re-call run() with daemon=false, but the outer run() already wrote .mcp.json…
    // This fallback is only for the case daemon was requested but failed to start.
    // We recursively call run() with use_daemon=false.
    Box::pin(run(project_path, &repo_name, db_path, false, 9877)).await
}

/// Find the codescope-mcp binary — check PATH, common locations, and sibling dir.
pub(crate) fn find_mcp_binary() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        "codescope-mcp.exe"
    } else {
        "codescope-mcp"
    };

    // Check platform-specific install dir
    if cfg!(windows) {
        let win_path = std::env::var("LOCALAPPDATA").ok().map(|d| {
            PathBuf::from(d)
                .join("codescope")
                .join("bin")
                .join(exe_name)
        });
        if let Some(ref p) = win_path {
            if p.exists() {
                return Some(p.clone());
            }
        }
    }
    let local_bin = dirs::home_dir().map(|h| h.join(".local").join("bin").join(exe_name));
    if let Some(ref p) = local_bin {
        if p.exists() {
            return Some(p.clone());
        }
    }

    // Check same directory as current executable
    if let Ok(current) = std::env::current_exe() {
        let sibling = current.parent().map(|p| p.join(exe_name));
        if let Some(ref p) = sibling {
            if p.exists() {
                return Some(p.clone());
            }
        }
    }

    // Check if in PATH
    if let Ok(output) = std::process::Command::new("which").arg(exe_name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    // Windows: try where.exe
    if cfg!(windows) {
        if let Ok(output) = std::process::Command::new("where.exe")
            .arg(exe_name)
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !path.is_empty() {
                    return Some(PathBuf::from(path));
                }
            }
        }
    }

    None
}

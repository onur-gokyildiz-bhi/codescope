//! `codescope doctor` — diagnose and fix common setup issues.
//!
//! Checks every moving part of the codescope stack and reports pass/fail
//! with actionable fix instructions. Optionally auto-fixes what it can.

use anyhow::Result;
use std::path::PathBuf;

struct Check {
    name: &'static str,
    status: Status,
    detail: String,
    fix: Option<String>,
}

enum Status {
    Pass,
    Warn,
    Fail,
}

impl Check {
    fn icon(&self) -> &str {
        match self.status {
            Status::Pass => "✓",
            Status::Warn => "⚠",
            Status::Fail => "✗",
        }
    }
    fn color_code(&self) -> &str {
        match self.status {
            Status::Pass => "\x1b[32m",
            Status::Warn => "\x1b[33m",
            Status::Fail => "\x1b[31m",
        }
    }
}

pub async fn run(project_path: PathBuf, auto_fix: bool) -> Result<()> {
    let project_path =
        std::fs::canonicalize(&project_path).unwrap_or_else(|_| project_path.clone());
    // Strip Windows extended-length prefix
    let project_path = {
        let s = project_path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            PathBuf::from(stripped)
        } else {
            project_path
        }
    };

    println!("\n  Codescope Doctor");
    println!("  ================\n");
    println!("  Project: {}\n", project_path.display());

    let mut checks = Vec::new();
    let mut fixes_applied = 0;

    // 1. Binary on PATH
    checks.push(check_binary("codescope"));
    checks.push(check_binary("codescope-mcp"));

    // 2. .mcp.json exists
    let mcp_json = project_path.join(".mcp.json");
    if mcp_json.exists() {
        let content = std::fs::read_to_string(&mcp_json).unwrap_or_default();
        if content.contains("codescope") {
            checks.push(Check {
                name: ".mcp.json",
                status: Status::Pass,
                detail: "exists with codescope entry".into(),
                fix: None,
            });
        } else {
            checks.push(Check {
                name: ".mcp.json",
                status: Status::Fail,
                detail: "exists but missing codescope entry".into(),
                fix: Some("Run: codescope init".into()),
            });
        }
    } else {
        let fix_msg = "Run: codescope init".to_string();
        if auto_fix {
            // Will be handled by codescope init later
        }
        checks.push(Check {
            name: ".mcp.json",
            status: Status::Fail,
            detail: "missing".into(),
            fix: Some(fix_msg),
        });
    }

    // 3. .claude/rules/codescope-mandatory.md
    let rule_path = project_path
        .join(".claude")
        .join("rules")
        .join("codescope-mandatory.md");
    if rule_path.exists() {
        checks.push(Check {
            name: "Claude rule (codescope-mandatory)",
            status: Status::Pass,
            detail: ".claude/rules/codescope-mandatory.md present".into(),
            fix: None,
        });
    } else if auto_fix {
        let rules_dir = project_path.join(".claude").join("rules");
        let _ = std::fs::create_dir_all(&rules_dir);
        let _ = std::fs::write(
            &rule_path,
            include_str!("../../../../.claude/rules/codescope-mandatory.md"),
        );
        checks.push(Check {
            name: "Claude rule (codescope-mandatory)",
            status: Status::Pass,
            detail: "FIXED — created .claude/rules/codescope-mandatory.md".into(),
            fix: None,
        });
        fixes_applied += 1;
    } else {
        checks.push(Check {
            name: "Claude rule (codescope-mandatory)",
            status: Status::Fail,
            detail: "missing — Claude Code won't use codescope tools".into(),
            fix: Some("Run: codescope doctor --fix\nOr: codescope init".into()),
        });
    }

    // 4. CLAUDE.md has codescope instructions
    let claude_md = project_path.join("CLAUDE.md");
    if claude_md.exists() {
        let content = std::fs::read_to_string(&claude_md).unwrap_or_default();
        if content.contains("codescope") || content.contains("Codescope") {
            checks.push(Check {
                name: "CLAUDE.md",
                status: Status::Pass,
                detail: "contains codescope instructions".into(),
                fix: None,
            });
        } else {
            checks.push(Check {
                name: "CLAUDE.md",
                status: Status::Warn,
                detail: "exists but no codescope instructions — agent may ignore MCP tools".into(),
                fix: Some(
                    "Add codescope tool reference to CLAUDE.md (see docs/quickstart.md)".into(),
                ),
            });
        }
    } else {
        checks.push(Check {
            name: "CLAUDE.md",
            status: Status::Warn,
            detail: "missing — consider adding codescope instructions as fallback".into(),
            fix: Some("Run: codescope init (creates .claude/rules/ which is equivalent)".into()),
        });
    }

    // 5. Database directory
    let repo_name = project_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "default".into());
    let db_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db")
        .join(&repo_name);

    if db_path.exists() {
        // Check if DB has data
        let db_size: u64 = std::fs::read_dir(&db_path)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
                    .sum()
            })
            .unwrap_or(0);

        if db_size > 1000 {
            checks.push(Check {
                name: "Database",
                status: Status::Pass,
                detail: format!(
                    "{} ({:.1} MB)",
                    db_path.display(),
                    db_size as f64 / 1_000_000.0
                ),
                fix: None,
            });
        } else {
            checks.push(Check {
                name: "Database",
                status: Status::Warn,
                detail: format!(
                    "{} exists but looks empty ({} bytes)",
                    db_path.display(),
                    db_size
                ),
                fix: Some("Run: codescope init (re-indexes the codebase)".into()),
            });
        }
    } else {
        checks.push(Check {
            name: "Database",
            status: Status::Fail,
            detail: "not indexed yet".into(),
            fix: Some("Run: codescope init".into()),
        });
    }

    // 6. Check for stale processes (potential DB lock)
    let stale = check_stale_processes();
    checks.push(stale);

    // 7. .gitignore has !.claude/rules/
    let gitignore = project_path.join(".gitignore");
    if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore).unwrap_or_default();
        if content.contains(".claude") && !content.contains("!.claude/rules") {
            if auto_fix {
                let mut f = std::fs::OpenOptions::new().append(true).open(&gitignore)?;
                use std::io::Write;
                writeln!(
                    f,
                    "\n# Allow Claude Code rules to be committed\n!.claude/rules/"
                )?;
                checks.push(Check {
                    name: ".gitignore",
                    status: Status::Pass,
                    detail: "FIXED — added !.claude/rules/".into(),
                    fix: None,
                });
                fixes_applied += 1;
            } else {
                checks.push(Check {
                    name: ".gitignore",
                    status: Status::Warn,
                    detail: ".claude is ignored but rules/ not excluded — rules won't be committed"
                        .into(),
                    fix: Some("Run: codescope doctor --fix".into()),
                });
            }
        } else {
            checks.push(Check {
                name: ".gitignore",
                status: Status::Pass,
                detail: "ok".into(),
                fix: None,
            });
        }
    }

    // Print results
    let mut fails = 0;
    let mut warns = 0;
    for c in &checks {
        println!(
            "  {}{} {}\x1b[0m — {}",
            c.color_code(),
            c.icon(),
            c.name,
            c.detail
        );
        if let Some(fix) = &c.fix {
            for line in fix.lines() {
                println!("      → {}", line);
            }
        }
        match c.status {
            Status::Fail => fails += 1,
            Status::Warn => warns += 1,
            Status::Pass => {}
        }
    }

    println!();
    if fixes_applied > 0 {
        println!("  {} issues auto-fixed.", fixes_applied);
    }
    if fails > 0 {
        println!(
            "  \x1b[31m{} failures\x1b[0m, {} warnings. Run the suggested fixes above.",
            fails, warns
        );
        println!("  Or: codescope doctor --fix (auto-fix what's possible)\n");
    } else if warns > 0 {
        println!(
            "  \x1b[32mNo failures.\x1b[0m {} warnings (non-critical).\n",
            warns
        );
    } else {
        println!("  \x1b[32mAll checks passed.\x1b[0m Codescope is ready.\n");
    }

    Ok(())
}

fn check_binary(name: &str) -> Check {
    // Check via which/where
    let found = if cfg!(windows) {
        std::process::Command::new("where.exe")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        std::process::Command::new("which")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };

    if found {
        // Get version
        let version = std::process::Command::new(name)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default()
            .trim()
            .to_string();

        Check {
            name: if name == "codescope" {
                "Binary: codescope"
            } else {
                "Binary: codescope-mcp"
            },
            status: Status::Pass,
            detail: if version.is_empty() {
                "on PATH".into()
            } else {
                version.clone()
            },
            fix: None,
        }
    } else {
        Check {
            name: if name == "codescope" {
                "Binary: codescope"
            } else {
                "Binary: codescope-mcp"
            },
            status: Status::Fail,
            detail: format!("{} not found on PATH", name),
            fix: Some(
                "Install: curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash"
                    .into(),
            ),
        }
    }
}

fn check_stale_processes() -> Check {
    let count = if cfg!(windows) {
        std::process::Command::new("tasklist")
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter(|l| l.contains("codescope"))
                    .count()
            })
            .unwrap_or(0)
    } else {
        std::process::Command::new("pgrep")
            .args(["-c", "-f", "codescope"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8(o.stdout)
                    .ok()?
                    .trim()
                    .parse::<usize>()
                    .ok()
            })
            .unwrap_or(0)
    };

    if count == 0 {
        Check {
            name: "Running processes",
            status: Status::Pass,
            detail: "no stale codescope processes".into(),
            fix: None,
        }
    } else if count <= 2 {
        Check {
            name: "Running processes",
            status: Status::Pass,
            detail: format!(
                "{} codescope process(es) running (likely MCP server)",
                count
            ),
            fix: None,
        }
    } else {
        Check {
            name: "Running processes",
            status: Status::Warn,
            detail: format!(
                "{} codescope processes running — possible stale instances holding DB locks",
                count
            ),
            fix: Some(
                "Kill stale: pkill -f codescope (Linux) or taskkill /f /im codescope.exe (Windows)"
                    .into(),
            ),
        }
    }
}

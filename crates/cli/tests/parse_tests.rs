//! Clap parser tests for the codescope CLI.
//! Verifies that subcommands parse correctly with expected defaults.

use clap::Parser;
use codescope_cli::{Cli, Commands, HistoryAction};

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["codescope"];
    full.extend_from_slice(args);
    Cli::try_parse_from(&full).unwrap_or_else(|e| panic!("parse failed: {}", e))
}

// ── Index ───────────────────────────────────────────────────────

#[test]
fn index_with_path() {
    let cli = parse(&["index", "/some/path"]);
    match cli.command {
        Commands::Index { path, clean } => {
            assert_eq!(path.to_string_lossy(), "/some/path");
            assert!(!clean);
        }
        _ => panic!("expected Index"),
    }
}

#[test]
fn index_with_clean_flag() {
    let cli = parse(&["index", ".", "--clean"]);
    match cli.command {
        Commands::Index { clean, .. } => assert!(clean),
        _ => panic!(),
    }
}

// ── Search ──────────────────────────────────────────────────────

#[test]
fn search_default_limit_20() {
    let cli = parse(&["search", "foo"]);
    match cli.command {
        Commands::Search { query, limit } => {
            assert_eq!(query, "foo");
            assert_eq!(limit, 20);
        }
        _ => panic!(),
    }
}

#[test]
fn search_custom_limit() {
    let cli = parse(&["search", "foo", "--limit", "50"]);
    match cli.command {
        Commands::Search { limit, .. } => assert_eq!(limit, 50),
        _ => panic!(),
    }
}

// ── Embed ───────────────────────────────────────────────────────

#[test]
fn embed_default_provider_fastembed() {
    let cli = parse(&["embed"]);
    match cli.command {
        Commands::Embed {
            provider,
            batch_size,
            ..
        } => {
            assert_eq!(provider, "fastembed");
            assert_eq!(batch_size, 100);
        }
        _ => panic!(),
    }
}

#[test]
fn embed_with_ollama() {
    let cli = parse(&["embed", "--provider", "ollama", "--batch-size", "200"]);
    match cli.command {
        Commands::Embed {
            provider,
            batch_size,
            ..
        } => {
            assert_eq!(provider, "ollama");
            assert_eq!(batch_size, 200);
        }
        _ => panic!(),
    }
}

// ── MCP / Web / Serve ────────────────────────────────────────────

#[test]
fn mcp_defaults_to_current_dir() {
    let cli = parse(&["mcp"]);
    match cli.command {
        Commands::Mcp { path, auto_index } => {
            assert_eq!(path.to_string_lossy(), ".");
            assert!(!auto_index);
        }
        _ => panic!(),
    }
}

#[test]
fn mcp_with_auto_index() {
    let cli = parse(&["mcp", "/some/path", "--auto-index"]);
    match cli.command {
        Commands::Mcp { path, auto_index } => {
            assert_eq!(path.to_string_lossy(), "/some/path");
            assert!(auto_index);
        }
        _ => panic!(),
    }
}

#[test]
fn web_default_port_9876() {
    let cli = parse(&["web"]);
    match cli.command {
        Commands::Web { port, .. } => assert_eq!(port, 9876),
        _ => panic!(),
    }
}

#[test]
fn serve_default_port_9877() {
    let cli = parse(&["serve"]);
    match cli.command {
        Commands::Serve { port, bind } => {
            assert_eq!(port, 9877);
            assert_eq!(bind, "127.0.0.1");
        }
        _ => panic!(),
    }
}

#[test]
fn serve_custom_bind() {
    let cli = parse(&["serve", "--bind", "0.0.0.0", "--port", "8000"]);
    match cli.command {
        Commands::Serve { port, bind } => {
            assert_eq!(port, 8000);
            assert_eq!(bind, "0.0.0.0");
        }
        _ => panic!(),
    }
}

// ── History sub-subcommands ─────────────────────────────────────

#[test]
fn history_commits() {
    let cli = parse(&["history", "/repo", "commits", "--limit", "5"]);
    match cli.command {
        Commands::History { action, .. } => match action {
            HistoryAction::Commits { limit } => assert_eq!(limit, 5),
            _ => panic!("expected Commits"),
        },
        _ => panic!(),
    }
}

#[test]
fn history_churn_default_limit() {
    let cli = parse(&["history", ".", "churn"]);
    match cli.command {
        Commands::History { action, .. } => match action {
            HistoryAction::Churn { limit } => assert_eq!(limit, 20),
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn history_contributors() {
    let cli = parse(&["history", ".", "contributors"]);
    match cli.command {
        Commands::History { action, .. } => {
            assert!(matches!(action, HistoryAction::Contributors));
        }
        _ => panic!(),
    }
}

// ── Global flags ────────────────────────────────────────────────

#[test]
fn global_repo_flag() {
    let cli = parse(&["--repo", "myrepo", "search", "foo"]);
    assert_eq!(cli.repo.as_deref(), Some("myrepo"));
}

#[test]
fn global_db_path_flag() {
    let cli = parse(&["--db-path", "/tmp/db", "stats"]);
    assert_eq!(
        cli.db_path
            .as_deref()
            .map(|p| p.to_string_lossy().to_string()),
        Some("/tmp/db".to_string())
    );
}

// ── Daemon control ──────────────────────────────────────────────

#[test]
fn daemon_lifecycle_commands_parse() {
    let _ = parse(&["start"]);
    let _ = parse(&["stop"]);
    let _ = parse(&["status"]);
}

#[test]
fn install_command_parses() {
    let _ = parse(&["install"]);
}

#[test]
fn languages_command_parses() {
    let _ = parse(&["languages"]);
}

#[test]
fn version_flag_works() {
    // -V / --version should exit cleanly (clap returns DisplayVersion error)
    let result = Cli::try_parse_from(["codescope", "--version"]);
    let err = result
        .err()
        .expect("--version should produce DisplayVersion error");
    assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
}

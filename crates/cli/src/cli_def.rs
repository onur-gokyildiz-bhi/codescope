//! CLI definitions extracted from main.rs so they can be unit-tested.
//! Phase 3 will move command handlers into a `commands/` sub-module.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codescope")]
#[command(about = "Codescope — Rust-native code intelligence engine")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Repository name for DB isolation (default: current directory name)
    #[arg(long, global = true)]
    pub repo: Option<String>,

    /// Override database path (default: ~/.codescope/db/<repo>/)
    #[arg(long, global = true)]
    pub db_path: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Index a codebase into the knowledge graph
    Index {
        path: PathBuf,
        #[arg(long)]
        clean: bool,
    },

    /// Search the code graph
    Search {
        query: String,
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Query the graph with raw SurrealQL
    Query { surql: String },

    /// Show graph statistics
    Stats,

    /// Analyze git history
    History {
        path: PathBuf,
        #[command(subcommand)]
        action: HistoryAction,
    },

    /// Generate embeddings for indexed functions
    Embed {
        #[arg(long, default_value = "fastembed")]
        provider: String,
        #[arg(long, default_value = "100")]
        batch_size: usize,
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        #[arg(long, default_value = "nomic-embed-text")]
        model: String,
    },

    /// Semantic search using embeddings
    SemanticSearch {
        query: String,
        #[arg(long, default_value = "10")]
        limit: usize,
        #[arg(long, default_value = "fastembed")]
        provider: String,
        #[arg(long, default_value = "http://localhost:11434")]
        ollama_url: String,
        #[arg(long, default_value = "nomic-embed-text")]
        model: String,
    },

    /// Sync git history into the graph database
    SyncHistory {
        path: PathBuf,
        #[arg(long, default_value = "200")]
        limit: usize,
    },

    /// Detect code hotspots (high complexity + high churn)
    Hotspots,

    /// List supported languages
    Languages,

    /// Initialize Codescope in current project (creates .mcp.json + first index)
    Init {
        path: Option<PathBuf>,
        /// Use daemon mode: start background daemon, HTTP .mcp.json (no DB lock conflicts)
        #[arg(long)]
        daemon: bool,
        /// Daemon port (only with --daemon, default 9877)
        #[arg(long, default_value = "9877")]
        daemon_port: u16,
    },

    /// Diagnose and fix common setup issues
    Doctor {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Auto-fix issues that can be resolved automatically
        #[arg(long)]
        fix: bool,
    },

    /// Install codescope binary to ~/.local/bin (adds to PATH)
    Install,

    /// Start MCP server (for AI agent integration)
    Mcp {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        auto_index: bool,
    },

    /// Start web visualization dashboard
    Web {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value = "9876")]
        port: u16,
        /// Network address to bind (0.0.0.0 = LAN, 127.0.0.1 = localhost only)
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        #[arg(long)]
        auto_index: bool,
    },

    /// Start Language Server Protocol bridge (stdio) — editor-agnostic
    Lsp {
        /// Workspace path (repo name is derived from the directory name).
        /// If omitted, the current directory is used.
        path: Option<PathBuf>,
    },

    /// Start daemon (MCP + Web UI on single port, multi-project)
    Serve {
        #[arg(long, default_value = "9877")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },

    /// Start daemon in background
    Start {
        #[arg(long, default_value = "9877")]
        port: u16,
    },

    /// Stop running daemon
    Stop {
        #[arg(long, default_value = "9877")]
        port: u16,
    },

    /// Check daemon status
    Status {
        #[arg(long, default_value = "9877")]
        port: u16,
    },
}

#[derive(Subcommand)]
pub enum HistoryAction {
    /// Show recent commits
    Commits {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show file churn (most changed files)
    Churn {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show change coupling (files changed together)
    Coupling {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show contributor map
    Contributors,
}

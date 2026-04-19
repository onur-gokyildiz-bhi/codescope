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
        /// Which agent to wire codescope into. Default: claude-code.
        /// Accepts: claude-code | cursor | gemini-cli | vscode-copilot | codex | windsurf.
        #[arg(long, default_value = "claude-code")]
        agent: String,
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

    /// Start the codescope daemon (web + MCP on port 9877) in
    /// background. For the underlying surreal server, use
    /// `codescope start` instead.
    DaemonStart {
        #[arg(long, default_value = "9877")]
        port: u16,
    },

    /// Stop the codescope daemon.
    DaemonStop {
        #[arg(long, default_value = "9877")]
        port: u16,
    },

    /// Check the codescope daemon status.
    DaemonStatus {
        #[arg(long, default_value = "9877")]
        port: u16,
    },

    /// Review a git diff with graph context — impact analysis of changes
    Review {
        /// Git ref range (e.g. "main..HEAD"), commit SHA, or path to .diff file
        target: String,
        /// Max callers per function to show (default 10)
        #[arg(long, default_value = "10")]
        max_callers: usize,
        /// Show functions with no tests
        #[arg(long)]
        coverage: bool,
    },

    /// Apply pending schema migrations to the repo DB
    Migrate {
        /// Override the repo name (defaults to current directory / --repo flag)
        repo: Option<String>,
    },

    /// Migrate legacy per-repo SurrealKV dirs (~/.codescope/db/<repo>/) into
    /// the unified `surreal` server. Dry-run by default; pass --execute to
    /// actually perform the copy. Originals are preserved as `<repo>.old/`.
    MigrateToServer {
        /// Migrate only this repo. Default: every dir under ~/.codescope/db/.
        #[arg(long)]
        repo: Option<String>,
        /// Actually perform the migration (default: dry-run, prints plan only).
        #[arg(long)]
        execute: bool,
        /// Delete the `.old/` backup after verify succeeds. Default: keep.
        #[arg(long)]
        delete_backup: bool,
    },

    /// Show cumulative token-savings from MCP tool calls. Reads the
    /// counter at `~/.codescope/gain.json` (written by the MCP
    /// server every 30 s). Number is an estimate: total_calls ×
    /// average tokens-saved-per-call.
    Gain,

    /// Per-call insight dashboard — counts by repo + hourly
    /// sparkline of tool activity. Reads `~/.codescope/insight.jsonl`.
    Insight,

    /// Self-update — download the latest GitHub release for the
    /// host triple and replace the installed binaries in-place.
    /// `--yes` skips the confirmation prompt.
    Upgrade {
        #[arg(long)]
        yes: bool,
    },

    /// Rebuild a corrupted repo's DB. Drops NS=codescope DB=<repo>
    /// on the running surreal server (no files touched on disk —
    /// the server handles that), then optionally re-indexes from
    /// source if `--reindex <path>` is given. Prompts unless `--yes`
    /// is set.
    Repair {
        /// Repo name to rebuild (as used by `codescope index --repo`).
        #[arg(long)]
        repo: String,
        /// Absolute path to the codebase — if set, `repair` invokes
        /// `codescope index` after the drop so the repo ends up
        /// populated again.
        #[arg(long)]
        reindex: Option<PathBuf>,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },

    /// Start the bundled surreal server (idempotent). Writes a state
    /// file at `~/.codescope/surreal.json` with pid / port / version so
    /// subsequent `start`/`stop`/`status` commands know what's running.
    Start {
        /// Port to bind to. Default: 8077.
        #[arg(long)]
        port: Option<u16>,
    },

    /// Stop the bundled surreal server. No-op if nothing is recorded.
    Stop,

    /// Report the surreal server's current state without changing it.
    /// Exits 0 with a single-line status keyword, suitable for
    /// scripting: `running`, `not-running`, `stale-pid`, `unhealthy`.
    Status,

    /// Bulk-ingest Claude Code conversation transcripts (.jsonl) into the
    /// knowledge graph. Walks a directory tree for *.jsonl, parses each via
    /// the conversation classifier (decisions, problems, solutions, topics),
    /// and inserts into the global DB (default) or a specific project DB.
    /// Incremental by default — skips files already indexed (hash match).
    IngestConversations {
        /// Directory to scan recursively. Default: ~/.claude/projects
        #[arg(long)]
        dir: Option<PathBuf>,
        /// DB target: "global" (cross-project) or "project" (needs --repo).
        #[arg(long, default_value = "global")]
        scope: String,
        /// Project repo name (required when scope=project).
        #[arg(long)]
        repo: Option<String>,
        /// Re-parse every file regardless of prior hash match.
        #[arg(long)]
        full: bool,
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

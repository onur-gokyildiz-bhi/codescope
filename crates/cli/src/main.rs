use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use codescope_cli::commands;
use codescope_cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    match run().await {
        Ok(()) => {}
        Err(e) => {
            // R2 contract — emit structured error shape on stderr so
            // tooling that wraps the CLI (panels, bots, CI) can parse
            // `.error.message` / `.error.hint` instead of scraping
            // anyhow's prose.
            let (code, hint) = classify_cli_error(&e);
            let body = serde_json::json!({
                "ok": false,
                "error": {
                    "code": code,
                    "message": e.to_string(),
                    "hint": hint,
                }
            });
            eprintln!("{body}");
            std::process::exit(1);
        }
    }
}

fn classify_cli_error(e: &anyhow::Error) -> (&'static str, Option<String>) {
    let msg = format!("{e:#}").to_lowercase();
    if msg.contains("connection refused")
        || msg.contains("is `codescope start` running")
        || msg.contains("timed out connecting")
    {
        return (
            "db_unreachable",
            Some("Run `codescope start` to launch the surreal server.".into()),
        );
    }
    if msg.contains("timeout") || msg.contains("timed out") {
        return ("timeout", None);
    }
    if msg.contains("not found") {
        return ("invalid_input", None);
    }
    ("internal", None)
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    // Global repo name: --repo flag > current directory name
    let global_repo = cli.repo.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".into())
    });

    match cli.command {
        Commands::Index { path, clean } => {
            let repo_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| global_repo.clone());
            commands::index::run(path, &repo_name, clean, cli.db_path).await?;
        }
        Commands::Search { query, limit } => {
            commands::search::run(&query, limit, &global_repo, cli.db_path).await?;
        }
        Commands::Query { surql } => {
            commands::query::run(&surql, &global_repo, cli.db_path).await?;
        }
        Commands::Stats => {
            commands::stats::run(&global_repo, cli.db_path).await?;
        }
        Commands::History { path, action } => {
            commands::history::run(path, action)?;
        }
        Commands::SyncHistory { path, limit } => {
            commands::sync_history::run(path, &global_repo, limit, cli.db_path).await?;
        }
        Commands::Hotspots => {
            commands::hotspots::run(&global_repo, cli.db_path).await?;
        }
        Commands::Embed {
            provider,
            batch_size,
            ollama_url,
            model,
        } => {
            commands::embed::run(
                &provider,
                batch_size,
                &ollama_url,
                &model,
                &global_repo,
                cli.db_path,
            )
            .await?;
        }
        Commands::SemanticSearch {
            query,
            limit,
            provider,
            ollama_url,
            model,
        } => {
            commands::semantic_search::run(
                &query,
                limit,
                &provider,
                &ollama_url,
                &model,
                &global_repo,
                cli.db_path,
            )
            .await?;
        }
        Commands::Languages => {
            commands::languages::run();
        }
        Commands::Init {
            path,
            daemon,
            daemon_port,
            agent,
        } => {
            let project_path = path
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let agent_parsed = commands::agents::parse_name(&agent)?;
            commands::init::run(
                project_path,
                &global_repo,
                agent_parsed,
                cli.db_path,
                daemon,
                daemon_port,
            )
            .await?;
        }
        Commands::Install => {
            commands::install::run()?;
        }
        Commands::Doctor { path, fix } => {
            commands::doctor::run(path, fix).await?;
        }
        Commands::Mcp { path, auto_index } => {
            // Derive repo from target path, not CWD
            let repo = path
                .canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));
            codescope_mcp::run_stdio(path, repo, auto_index).await?;
        }
        Commands::Web {
            path,
            port,
            host,
            auto_index,
        } => {
            // Derive repo from target path, not CWD
            let repo = path
                .canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));
            codescope_web::run_web(path, repo, port, auto_index, cli.db_path, host).await?;
        }
        Commands::Lsp { path } => {
            // Change into the workspace dir so the LSP infers the right repo
            // name from the directory. Keep this simple — the LSP itself also
            // re-derives repo from the `initialize` params when the editor
            // provides a workspace root, so this only matters if the editor
            // doesn't send one.
            if let Some(p) = path.as_ref() {
                std::env::set_current_dir(p)?;
            }
            use tower_lsp::{LspService, Server};
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();
            let (service, socket) = LspService::new(codescope_lsp::Backend::new);
            Server::new(stdin, stdout, socket).serve(service).await;
        }
        Commands::Serve { port, bind } => {
            commands::serve::run(&bind, port).await?;
        }
        Commands::DaemonStart { port } => {
            commands::daemon::start(port)?;
        }
        Commands::DaemonStop { port } => {
            commands::daemon::stop(port).await?;
        }
        Commands::DaemonStatus { port } => {
            commands::daemon::status(port).await?;
        }
        Commands::Review {
            target,
            max_callers,
            coverage,
        } => {
            commands::review::run(target, max_callers, coverage, cli.db_path, &global_repo).await?;
        }
        Commands::Migrate { repo } => {
            let repo_name = repo.unwrap_or_else(|| global_repo.clone());
            commands::migrate::run(&repo_name, cli.db_path).await?;
        }
        Commands::MigrateToServer {
            repo,
            execute,
            delete_backup,
        } => {
            commands::migrate_to_server::run(repo, execute, delete_backup).await?;
        }
        Commands::Gain => {
            commands::gain::run().await?;
        }
        Commands::Repair { repo, reindex, yes } => {
            commands::repair::run(repo, reindex, yes).await?;
        }
        Commands::Start { port } => {
            commands::supervisor::start(port).await?;
        }
        Commands::Stop => {
            commands::supervisor::stop().await?;
        }
        Commands::Status => {
            commands::supervisor::status().await?;
        }
        Commands::IngestConversations {
            dir,
            scope,
            repo,
            full,
        } => {
            commands::ingest_conversations::run(dir, scope, repo, full).await?;
        }
    }

    Ok(())
}

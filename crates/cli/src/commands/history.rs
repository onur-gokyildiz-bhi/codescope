use anyhow::Result;
use codescope_core::temporal::GitAnalyzer;
use std::path::PathBuf;

use crate::HistoryAction;

pub fn run(path: PathBuf, action: HistoryAction) -> Result<()> {
    let analyzer = GitAnalyzer::open(&path)?;

    match action {
        HistoryAction::Commits { limit } => {
            let commits = analyzer.recent_commits(limit)?;
            for c in &commits {
                println!(
                    "{} {} — {} ({} files)",
                    &c.hash[..8],
                    c.author,
                    c.message.lines().next().unwrap_or(""),
                    c.files_changed.len()
                );
            }
        }
        HistoryAction::Churn { limit } => {
            let churn = analyzer.file_churn(limit)?;
            for (file, count) in &churn {
                println!("{:>4}  {}", count, file);
            }
        }
        HistoryAction::Coupling { limit } => {
            let coupling = analyzer.change_coupling(limit)?;
            for (a, b, count) in &coupling {
                println!("{:>4}  {} <-> {}", count, a, b);
            }
        }
        HistoryAction::Contributors => {
            let map = analyzer.contributor_map()?;
            for (author, files) in &map {
                println!("{} ({} files touched):", author, files.len());
                for (file, count) in files.iter().take(5) {
                    println!("  {:>4}  {}", count, file);
                }
                if files.len() > 5 {
                    println!("  ... and {} more", files.len() - 5);
                }
            }
        }
    }

    Ok(())
}

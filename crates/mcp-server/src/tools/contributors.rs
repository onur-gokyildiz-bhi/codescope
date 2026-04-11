//! Contributor analysis tools: contributor_map, suggest_reviewers.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = contributors_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Get contributor expertise map — who knows which parts of the codebase
    #[tool(
        description = "Get a contributor expertise map showing who has the most knowledge about which files. Useful for finding the right reviewer for a change."
    )]
    async fn contributor_map(&self) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let git_path = ctx.codebase_path.clone();

        match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.contributor_map()
        })
        .await
        {
            Ok(Ok(map)) => {
                let mut output = "## Contributor Expertise Map\n\n".to_string();
                for (author, files) in &map {
                    output.push_str(&format!("### {} ({} files)\n", author, files.len()));
                    for (file, count) in files.iter().take(10) {
                        output.push_str(&format!("  - {} ({}x)\n", file, count));
                    }
                    if files.len() > 10 {
                        output.push_str(&format!("  ... and {} more\n", files.len() - 10));
                    }
                    output.push('\n');
                }
                output
            }
            Ok(Err(e)) => format!("Error: {}", e),
            Err(e) => format!("Task error: {}", e),
        }
    }

    /// Suggest reviewers for changed files based on git history
    #[tool(
        description = "Suggest code reviewers for a set of changed files based on who has the most expertise with those files."
    )]
    async fn suggest_reviewers(&self, Parameters(params): Parameters<DiffReviewParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let git_path = ctx.codebase_path.clone();
        let base_ref = params.base_ref.clone();
        let head_ref_str = params
            .head_ref
            .clone()
            .unwrap_or_else(|| "HEAD".to_string());

        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<String>, std::collections::HashMap<String, Vec<(String, usize)>>)> {
            let repo = git2::Repository::open(&git_path)?;
            let base = repo.revparse_single(&base_ref)?;
            let head = repo.revparse_single(&head_ref_str)?;
            let base_tree = base.peel_to_tree()?;
            let head_tree = head.peel_to_tree()?;
            let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;

            let mut changed_files = Vec::new();
            diff.foreach(
                &mut |delta, _| {
                    if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                        changed_files.push(path.to_string_lossy().to_string());
                    }
                    true
                },
                None, None, None,
            )?;

            let analyzer = codescope_core::temporal::GitAnalyzer::open(repo.path().parent().unwrap_or(repo.path()))?;
            let contributor_map = analyzer.contributor_map()?;

            Ok((changed_files, contributor_map))
        }).await;

        let (changed_files, contributor_map) = match result {
            Ok(Ok((cf, cm))) => (cf, cm),
            Ok(Err(e)) => return format!("Error: {}", e),
            Err(e) => return format!("Task error: {}", e),
        };

        let mut reviewer_scores: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (author, files) in &contributor_map {
            for (file, count) in files {
                if changed_files
                    .iter()
                    .any(|cf| file.contains(cf) || cf.contains(file))
                {
                    *reviewer_scores.entry(author.clone()).or_insert(0) += count;
                }
            }
        }

        let mut reviewers: Vec<_> = reviewer_scores.into_iter().collect();
        reviewers.sort_by(|a, b| b.1.cmp(&a.1));

        let head_display = params.head_ref.as_deref().unwrap_or("HEAD");
        let mut output = format!(
            "## Suggested Reviewers for {} → {}\n\n**{} files changed**\n\n",
            params.base_ref,
            head_display,
            changed_files.len()
        );

        if reviewers.is_empty() {
            output.push_str("No reviewer suggestions (no git history for changed files).\n");
        } else {
            output.push_str("| Reviewer | Expertise Score |\n|----------|----------------|\n");
            for (reviewer, score) in reviewers.iter().take(5) {
                output.push_str(&format!("| {} | {} |\n", reviewer, score));
            }
        }

        output
    }
}

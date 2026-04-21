//! Temporal/git history analysis tools: sync_git_history (action),
//! code_health (consolidated analysis: hotspots, churn, coupling, review_diff).

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::helpers::maybe_archive;
use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = temporal_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Sync git commit history into the graph database for temporal analysis
    #[tool(description = "Sync git commits into graph. Enables hotspots and change coupling.")]
    async fn sync_git_history(&self, Parameters(params): Parameters<SyncHistoryParams>) -> String {
        let ctx = match self.gated_ctx_named("sync_git_history").await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let git_path = params
            .git_path
            .map(|p| ctx.codebase_path.join(p))
            .unwrap_or_else(|| ctx.codebase_path.clone());
        let limit = params.limit.unwrap_or(200);

        let commits = match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.recent_commits(limit)
        })
        .await
        {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => return format!("Error reading git history: {}", e),
            Err(e) => return format!("Task error: {}", e),
        };

        let sync = codescope_core::temporal::TemporalGraphSync::new(ctx.db);
        match sync.sync_commit_data(&commits, &ctx.repo_name).await {
            Ok(count) => format!("Synced {} commits into the graph database", count),
            Err(e) => format!("Error syncing commits: {}", e),
        }
    }

    /// Consolidated code health analysis: hotspots, churn, coupling, review_diff
    #[tool(
        description = "Code health analysis (hotspots, churn, coupling, review_diff) from git + graph."
    )]
    async fn code_health(&self, Parameters(params): Parameters<CodeHealthParams>) -> String {
        let ctx = match self.gated_ctx_named("code_health").await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let mode = params.mode.clone();
        let output = match params.mode.as_str() {
            "hotspots" => {
                let sync = codescope_core::temporal::TemporalGraphSync::new(ctx.db);
                match sync.calculate_hotspots(&ctx.repo_name).await {
                    Ok(hotspots) => {
                        if hotspots.is_empty() {
                            return "No hotspots found. Make sure to sync git history first with sync_git_history.".into();
                        }
                        let min_score = params.min_score.map(|s| s as i64).unwrap_or(0);
                        let filtered: Vec<_> = hotspots
                            .iter()
                            .filter(|h| h.risk_score.unwrap_or(0) >= min_score)
                            .collect();

                        let mut output = format!("## Code Hotspots ({})\n\n", filtered.len());
                        output.push_str("| Function | File | Size | Churn | Risk Score |\n");
                        output.push_str("|----------|------|------|-------|------------|\n");
                        for h in &filtered {
                            output.push_str(&format!(
                                "| {} | {} | {} | {} | {} |\n",
                                h.name.as_deref().unwrap_or("?"),
                                h.file_path.as_deref().unwrap_or("?"),
                                h.size.unwrap_or(0),
                                h.churn.unwrap_or(0),
                                h.risk_score.unwrap_or(0),
                            ));
                        }
                        output
                    }
                    Err(e) => format!("Error calculating hotspots: {}", e),
                }
            }
            "churn" => {
                let limit = params.limit.unwrap_or(20);
                let git_path = ctx.codebase_path.clone();

                match tokio::task::spawn_blocking(move || {
                    let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
                    analyzer.file_churn(limit)
                })
                .await
                {
                    Ok(Ok(churn)) => {
                        let mut output = "## File Churn (Most Changed Files)\n\n".to_string();
                        output.push_str("| Changes | File |\n|---------|------|\n");
                        for (file, count) in &churn {
                            output.push_str(&format!("| {} | {} |\n", count, file));
                        }
                        output
                    }
                    Ok(Err(e)) => format!("Error: {}", e),
                    Err(e) => format!("Task error: {}", e),
                }
            }
            "coupling" => {
                let limit = params.limit.unwrap_or(20);
                let git_path = ctx.codebase_path.clone();

                match tokio::task::spawn_blocking(move || {
                    let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
                    analyzer.change_coupling(limit)
                })
                .await
                {
                    Ok(Ok(coupling)) => {
                        let mut output =
                            "## Change Coupling (Files Changed Together)\n\n".to_string();
                        output
                            .push_str("| Count | File A | File B |\n|-------|--------|--------|\n");
                        for (a, b, count) in &coupling {
                            output.push_str(&format!("| {} | {} | {} |\n", count, a, b));
                        }
                        output
                    }
                    Ok(Err(e)) => format!("Error: {}", e),
                    Err(e) => format!("Task error: {}", e),
                }
            }
            "review_diff" => {
                let base_ref = match params.base_ref.clone() {
                    Some(b) => b,
                    None => {
                        return "Error: 'base_ref' is required for mode=review_diff".into();
                    }
                };
                let git_path = ctx.codebase_path.clone();
                let base_ref_for_task = base_ref.clone();
                let head_ref_str = params
                    .head_ref
                    .clone()
                    .unwrap_or_else(|| "HEAD".to_string());

                let changed_files = match tokio::task::spawn_blocking(
                    move || -> anyhow::Result<Vec<(String, String)>> {
                        let repo = git2::Repository::open(&git_path)?;
                        let base = repo.revparse_single(&base_ref_for_task)?;
                        let head = repo.revparse_single(&head_ref_str)?;
                        let base_tree = base.peel_to_tree()?;
                        let head_tree = head.peel_to_tree()?;
                        let diff =
                            repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;

                        let mut files = Vec::new();
                        diff.foreach(
                            &mut |delta, _| {
                                let path = delta
                                    .new_file()
                                    .path()
                                    .or_else(|| delta.old_file().path())
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                let status = match delta.status() {
                                    git2::Delta::Added => "added",
                                    git2::Delta::Deleted => "deleted",
                                    git2::Delta::Modified => "modified",
                                    git2::Delta::Renamed => "renamed",
                                    _ => "other",
                                };
                                files.push((path, status.to_string()));
                                true
                            },
                            None,
                            None,
                            None,
                        )?;
                        Ok(files)
                    },
                )
                .await
                {
                    Ok(Ok(f)) => f,
                    Ok(Err(e)) => return format!("Error computing diff: {}", e),
                    Err(e) => return format!("Task error: {}", e),
                };

                let gq = GraphQuery::new(ctx.db);
                let head_display = params.head_ref.as_deref().unwrap_or("HEAD");

                let mut output = format!(
                    "## Diff Review: {} → {}\n\n**{} files changed**\n\n",
                    base_ref,
                    head_display,
                    changed_files.len()
                );

                if !changed_files.is_empty() {
                    let file_list = changed_files
                        .iter()
                        .map(|(fp, _)| format!("'{}'", fp.replace('\'', "\\'")))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let batch_query = format!(
                        "SELECT name, file_path, start_line, end_line FROM `function` WHERE file_path IN [{}]; \
                         SELECT name, file_path, start_line, end_line FROM class WHERE file_path IN [{}];",
                        file_list, file_list
                    );

                    let mut entities_by_file: std::collections::HashMap<
                        String,
                        Vec<(String, u32, u32)>,
                    > = std::collections::HashMap::with_capacity(changed_files.len());

                    if let Ok(batch_result) = gq.raw_query(&batch_query).await {
                        if let Some(arr) = batch_result.as_array() {
                            for stmt_result in arr {
                                if let Some(rows) = stmt_result.as_array() {
                                    for row in rows {
                                        let fp = row
                                            .get("file_path")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let name =
                                            row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                        let sl = row
                                            .get("start_line")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0)
                                            as u32;
                                        let el = row
                                            .get("end_line")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0)
                                            as u32;
                                        entities_by_file.entry(fp.to_string()).or_default().push((
                                            name.to_string(),
                                            sl,
                                            el,
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    for (file_path, status) in &changed_files {
                        output.push_str(&format!("### {} ({})\n", file_path, status));
                        if let Some(entities) = entities_by_file.get(file_path.as_str()) {
                            for (name, sl, el) in entities {
                                output.push_str(&format!("  - **{}** (L{}-{})\n", name, sl, el));
                            }
                        } else {
                            output.push_str("  (no indexed entities)\n");
                        }
                    }
                }

                output.push_str(&format!(
                    "\n---\n**Summary:** {} files affected.\n",
                    changed_files.len()
                ));
                output
            }
            other => {
                return format!(
                    "Error: unknown mode '{}'. Use: hotspots | churn | coupling | review_diff",
                    other
                );
            }
        };

        let archive_key = format!("code_health_{}", mode);
        maybe_archive(self.result_archive(), &archive_key, output).await
    }
}

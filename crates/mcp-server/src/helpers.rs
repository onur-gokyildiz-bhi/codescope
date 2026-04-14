use std::path::Path;
use std::sync::Arc;
use surrealdb::engine::local::Db;
use surrealdb::types::SurrealValue;
use surrealdb::Surreal;

/// Archive large tool outputs and return a summary with retrieval ID.
/// If the output is under the threshold (4096 chars), returns it unchanged.
/// Otherwise, stores the full output in the archive and returns the first 20 lines
/// plus a retrieval ID that can be used with `retrieve_archived`.
pub(crate) async fn maybe_archive(
    archive: &Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    tool_name: &str,
    output: String,
) -> String {
    const THRESHOLD: usize = 4096;
    if output.len() <= THRESHOLD {
        return output;
    }

    let id = format!("{}_{}", tool_name, archive.read().await.len());
    let summary_lines: Vec<&str> = output.lines().take(20).collect();
    let total_lines = output.lines().count();
    let remaining = total_lines.saturating_sub(20);
    let summary = format!(
        "{}\n\n… ({} more lines archived)\n**Retrieval ID:** `{}`\nUse `retrieve_archived(\"{}\")` to get the full output.",
        summary_lines.join("\n"),
        remaining,
        id,
        id,
    );
    archive.write().await.insert(id, output);
    summary
}

/// Derive a module scope from a file path for querying past decisions/problems.
/// e.g. "crates/core/src/graph/builder.rs" -> "core::graph"
/// e.g. "src/main.rs" -> "root"
pub fn derive_scope_from_file_path(file_path: &str) -> String {
    let normalized = file_path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() >= 3 && (parts[0] == "crates" || parts[0] == "src" || parts[0] == "lib") {
        // crates/core/src/graph/builder.rs -> core::graph
        parts
            .iter()
            .skip(1) // skip "crates"
            .take(parts.len().saturating_sub(3)) // skip filename and last dir
            .filter(|p| **p != "src")
            .copied()
            .collect::<Vec<_>>()
            .join("::")
    } else if parts.len() >= 2 {
        parts[..parts.len() - 1].join("::")
    } else {
        "root".to_string()
    }
}

/// Load known entity names from the graph for conversation-to-code linking.
/// Queries all 11 entity tables to maximize linking coverage.
pub(crate) async fn load_known_entities(db: &Surreal<Db>) -> Vec<String> {
    let query = "SELECT name, qualified_name FROM `function`; \
                 SELECT name, qualified_name FROM class; \
                 SELECT path AS name, path AS qualified_name FROM file; \
                 SELECT name, qualified_name FROM module; \
                 SELECT name, qualified_name FROM variable; \
                 SELECT name, qualified_name FROM import_decl; \
                 SELECT name, qualified_name FROM config; \
                 SELECT name, qualified_name FROM doc; \
                 SELECT name, qualified_name FROM api; \
                 SELECT name, qualified_name FROM infra; \
                 SELECT name, qualified_name FROM package;";

    let table_names = [
        "function",
        "class",
        "file",
        "module",
        "variable",
        "import_decl",
        "config",
        "doc",
        "api",
        "infra",
        "package",
    ];

    match db.query(query).await {
        Ok(mut response) => {
            let mut entities = Vec::new();

            for (table_idx, table_name) in table_names.iter().enumerate() {
                let results: Vec<serde_json::Value> = response.take(table_idx).unwrap_or_default();
                for r in results {
                    if let (Some(name), Some(qname)) = (
                        r.get("name").and_then(|v| v.as_str()),
                        r.get("qualified_name").and_then(|v| v.as_str()),
                    ) {
                        entities.push(format!("{}:{}:{}", table_name, name, qname));
                    }
                }
            }

            entities
        }
        Err(_) => Vec::new(),
    }
}

/// Check if a conversation file is already indexed by comparing stored hash
pub(crate) async fn check_conversation_hash(
    db: &Surreal<Db>,
    file_name: &str,
) -> anyhow::Result<Option<String>> {
    #[derive(serde::Deserialize, SurrealValue)]
    struct HashRecord {
        hash: Option<String>,
    }
    let results: Vec<HashRecord> = db
        .query("SELECT hash FROM conversation WHERE file_path = $name LIMIT 1")
        .bind(("name", file_name.to_string()))
        .await?
        .take(0)?;
    Ok(results.first().and_then(|r| r.hash.clone()))
}

/// Find the Claude projects directory matching a codebase path
pub fn find_claude_project_dir(codebase_path: &Path, repo_name: &str) -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let claude_projects = home.join(".claude").join("projects");

    let codebase_str = codebase_path
        .to_string_lossy()
        .replace(['/', '\\', ':'], "-")
        .replace("--", "-");

    match std::fs::read_dir(&claude_projects) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains(repo_name) || codebase_str.contains(&name) {
                    return entry.path();
                }
            }
            claude_projects
        }
        Err(_) => claude_projects,
    }
}

/// Build a concise conversation context summary from indexed conversations.
/// This gets injected into ServerInfo.instructions so Claude sees it automatically.
pub(crate) async fn build_context_summary(db: &Surreal<Db>, repo: &str) -> String {
    let mut sections = Vec::new();

    // Recent decisions (last 15), ordered by tier (critical first) then timestamp
    let decisions: Vec<serde_json::Value> = db
        .query("SELECT name, body, timestamp, tier FROM decision WHERE repo = $repo ORDER BY tier ASC, timestamp DESC LIMIT 15")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !decisions.is_empty() {
        let mut s = "## Recent Decisions\n".to_string();
        for d in &decisions {
            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let ts = d.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
            let tier = d.get("tier").and_then(|v| v.as_u64()).unwrap_or(2);
            let prefix = if tier == 0 { "[PINNED] " } else { "" };
            s.push_str(&format!("- {}: {}{}\n", date, prefix, name));
        }
        sections.push(s);
    }

    // Recent problems (last 5 unsolved)
    let problems: Vec<serde_json::Value> = db
        .query("SELECT name, timestamp FROM problem WHERE repo = $repo ORDER BY timestamp DESC LIMIT 5")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !problems.is_empty() {
        let mut s = "## Recent Problems\n".to_string();
        for p in &problems {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let ts = p.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
            s.push_str(&format!("- {}: {}\n", date, name));
        }
        sections.push(s);
    }

    // Recent solutions (last 5)
    let solutions: Vec<serde_json::Value> = db
        .query("SELECT name, timestamp FROM solution WHERE repo = $repo ORDER BY timestamp DESC LIMIT 5")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !solutions.is_empty() {
        let mut s = "## Recent Solutions\n".to_string();
        for sol in &solutions {
            let name = sol.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let ts = sol.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let date = if ts.len() >= 10 { &ts[..10] } else { ts };
            s.push_str(&format!("- {}: {}\n", date, name));
        }
        sections.push(s);
    }

    // Session count
    let stats: Vec<serde_json::Value> = db
        .query("SELECT count() FROM conversation WHERE repo = $repo GROUP ALL")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    let session_count = stats
        .first()
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if session_count > 0 {
        sections.push(format!(
            "*{} conversation sessions indexed for this project.*",
            session_count
        ));
    }

    // Last session context
    let last_sessions: Vec<serde_json::Value> = db
        .query(
            "SELECT name, timestamp FROM conversation WHERE repo = $repo ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if let Some(session) = last_sessions.first() {
        let ts = session
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        sections.push(format!("\n## Last Session\nLast indexed: {}\n", ts));
    }

    // Open problems (no linked solution)
    let open_problems: Vec<serde_json::Value> = db
        .query(
            "SELECT name, body FROM problem WHERE repo = $repo AND count(->solves_for) = 0 ORDER BY timestamp DESC LIMIT 5",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !open_problems.is_empty() {
        let mut s = "## Open Problems (Unresolved)\n".to_string();
        for p in &open_problems {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            s.push_str(&format!("- {}\n", name));
        }
        sections.push(s);
    }

    if sections.is_empty() {
        String::new()
    } else {
        format!("# Project Context\n\n{}", sections.join("\n"))
    }
}

/// Generate CONTEXT.md in ~/.codescope/projects/{repo}/ to avoid project bloat.
/// Falls back to {project}/.claude/CONTEXT.md if home dir is unavailable.
pub async fn generate_context_md(db: &Surreal<Db>, repo: &str, codebase_path: &Path) {
    let summary = build_context_summary(db, repo).await;
    let insights = build_post_index_insights(db, repo).await;

    if summary.is_empty() && insights.is_empty() {
        return;
    }

    let context_path = dirs::home_dir()
        .map(|h| {
            h.join(".codescope")
                .join("projects")
                .join(repo)
                .join("CONTEXT.md")
        })
        .unwrap_or_else(|| codebase_path.join(".claude").join("CONTEXT.md"));
    if let Some(parent) = context_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut content = format!(
        "<!-- Auto-generated by Codescope. Do not edit manually. -->\n\
         <!-- Updated: {} -->\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
    );

    if !insights.is_empty() {
        content.push_str(&insights);
        content.push_str("\n\n");
    }

    if !summary.is_empty() {
        content.push_str(&summary);
        content.push_str("\n\n");
    }

    content.push_str(
        "> Use `conversation_search` for deeper queries, `explore` for entity graph navigation.\n",
    );

    match std::fs::write(&context_path, &content) {
        Ok(_) => tracing::info!("Generated CONTEXT.md at {}", context_path.display()),
        Err(e) => tracing::warn!("Failed to write CONTEXT.md: {}", e),
    }
}

/// Build a compact project profile from indexed data.
/// Analyzes tech stack, architecture, naming convention, scale, and key patterns.
pub(crate) async fn build_project_profile(db: &Surreal<Db>, repo: &str) -> String {
    let mut lines = Vec::new();

    // 1. Tech Stack Detection — language distribution from functions
    let func_langs: Vec<serde_json::Value> = db
        .query(
            "SELECT language, count() AS cnt FROM `function` WHERE repo = $repo \
             GROUP BY language ORDER BY cnt DESC LIMIT 5",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    // Framework detection from packages
    let frameworks: Vec<serde_json::Value> = db
        .query(
            "SELECT name FROM package WHERE repo = $repo AND \
             (name ~ 'react' OR name ~ 'next' OR name ~ 'express' OR name ~ 'django' \
              OR name ~ 'flask' OR name ~ 'fastapi' OR name ~ 'spring' OR name ~ 'flutter' \
              OR name ~ 'aspire' OR name ~ 'axum' OR name ~ 'actix' OR name ~ 'gin' \
              OR name ~ 'fiber' OR name ~ 'rails') LIMIT 10",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !func_langs.is_empty() {
        let lang_parts: Vec<String> = func_langs
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let lang = l.get("language").and_then(|v| v.as_str())?;
                if lang.is_empty() || lang == "unknown" {
                    return None;
                }
                let label = if i == 0 {
                    format!("{} (primary)", lang)
                } else if i == 1 {
                    format!("{} (secondary)", lang)
                } else {
                    lang.to_string()
                };
                Some(label)
            })
            .collect();

        let fw_names: Vec<String> = frameworks
            .iter()
            .filter_map(|f| {
                f.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        let mut stack_line = lang_parts.join(", ");
        if !fw_names.is_empty() {
            stack_line.push_str(&format!(" — {}", fw_names.join(", ")));
        }
        lines.push(format!("- **Stack**: {}", stack_line));
    }

    // 2. Architecture Pattern — from file paths
    let file_paths: Vec<serde_json::Value> = db
        .query("SELECT path FROM file WHERE repo = $repo LIMIT 200")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !file_paths.is_empty() {
        let paths: Vec<String> = file_paths
            .iter()
            .filter_map(|f| {
                f.get("path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        let mut has_components = false;
        let mut has_crates = false;
        let mut has_lib_packages = false;
        let mut has_services = false;
        let mut has_models = false;
        let mut has_handlers = false;
        let mut seen_crates = std::collections::HashSet::new();
        for p in &paths {
            let normalized = p.replace('\\', "/");
            if normalized.contains("src/components/") || normalized.contains("/components/") {
                has_components = true;
            }
            if normalized.contains("crates/") && normalized.contains("/src/") {
                has_crates = true;
                // Count distinct crate names
                if let Some(after_crates) = normalized.split("crates/").nth(1) {
                    if let Some(crate_name) = after_crates.split('/').next() {
                        seen_crates.insert(crate_name.to_string());
                    }
                }
            }
            if normalized.starts_with("lib/")
                && normalized.split('/').count() > 2
                && !normalized.contains("node_modules")
            {
                has_lib_packages = true;
            }
            if normalized.contains("services/") && normalized.split('/').count() > 2 {
                has_services = true;
            }
            if normalized.contains("app/models/") {
                has_models = true;
            }
            if normalized.contains("handlers/") || normalized.contains("controllers/") {
                has_handlers = true;
            }
        }
        let crate_count = seen_crates.len();

        let arch = if has_crates {
            format!("Workspace monorepo ({} crates)", crate_count)
        } else if has_services {
            "Microservices".to_string()
        } else if has_components {
            "Component-based (React/Vue)".to_string()
        } else if has_lib_packages {
            "Dart/Flutter package".to_string()
        } else if has_models && has_handlers {
            "MVC (Rails/Django)".to_string()
        } else if has_handlers {
            "Handler pattern".to_string()
        } else {
            "Standard".to_string()
        };
        lines.push(format!("- **Architecture**: {}", arch));
    }

    // 3. Naming Convention — analyze function names
    let fn_names: Vec<serde_json::Value> = db
        .query("SELECT name FROM `function` WHERE repo = $repo LIMIT 100")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !fn_names.is_empty() {
        let mut snake = 0u32;
        let mut camel = 0u32;
        let mut pascal = 0u32;

        for f in &fn_names {
            if let Some(name) = f.get("name").and_then(|v| v.as_str()) {
                if name.contains('_') && name == name.to_lowercase() {
                    snake += 1;
                } else if !name.is_empty() {
                    let first_char = name.chars().next().unwrap_or('a');
                    if first_char.is_uppercase() {
                        pascal += 1;
                    } else if name.chars().any(|c| c.is_uppercase()) {
                        camel += 1;
                    }
                }
            }
        }

        let convention = if snake >= camel && snake >= pascal {
            "snake_case"
        } else if camel >= snake && camel >= pascal {
            "camelCase"
        } else {
            "PascalCase"
        };
        lines.push(format!("- **Convention**: {}", convention));
    }

    // 4. Scale — count functions, classes, files
    let scale_counts: Vec<serde_json::Value> = db
        .query(
            "SELECT \
             (SELECT count() FROM `function` WHERE repo = $repo GROUP ALL)[0].count AS fn_count, \
             (SELECT count() FROM class WHERE repo = $repo GROUP ALL)[0].count AS cls_count, \
             (SELECT count() FROM file WHERE repo = $repo GROUP ALL)[0].count AS file_count",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if let Some(row) = scale_counts.first() {
        let fns = row.get("fn_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let cls = row.get("cls_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let files = row.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0);
        if fns > 0 || cls > 0 || files > 0 {
            let mut parts = Vec::new();
            if fns > 0 {
                parts.push(format!("{} functions", fns));
            }
            if cls > 0 {
                parts.push(format!("{} classes", cls));
            }
            if files > 0 {
                parts.push(format!("{} files", files));
            }
            lines.push(format!("- **Scale**: {}", parts.join(", ")));
        }
    }

    // 5. Key patterns — top 5 most-called functions (by incoming call count)
    let top_called: Vec<serde_json::Value> = db
        .query(
            "SELECT out.name AS name, count() AS call_count FROM calls \
             WHERE out.repo = $repo \
             GROUP BY out.name ORDER BY call_count DESC LIMIT 5",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !top_called.is_empty() {
        let names: Vec<String> = top_called
            .iter()
            .filter_map(|r| {
                r.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| format!("`{}`", s))
            })
            .collect();
        if !names.is_empty() {
            lines.push(format!("- **Key patterns**: {}", names.join(", ")));
        }
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("## Project Profile\n{}\n", lines.join("\n"))
    }
}

/// Post-index project insights — auto-generated recommendations after indexing.
/// Returned as markdown and injected into CONTEXT.md + MCP server instructions.
pub(crate) async fn build_post_index_insights(db: &Surreal<Db>, repo: &str) -> String {
    let mut insights = Vec::new();

    // Project Profile at the top
    let profile = build_project_profile(db, repo).await;
    if !profile.is_empty() {
        insights.push(profile);
    }

    // 1. Hotspots — largest functions (refactoring candidates)
    let hotspots: Vec<serde_json::Value> = db
        .query(
            "SELECT name, file_path, (end_line - start_line) AS size FROM `function` \
                WHERE repo = $repo ORDER BY size DESC LIMIT 5",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !hotspots.is_empty() {
        let mut s = "### Hotspots (refactor candidates)\n".to_string();
        for h in &hotspots {
            let name = h.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let file = h.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let size = h.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            if size > 50 {
                s.push_str(&format!("- **{}** ({}, {} lines)\n", name, file, size));
            }
        }
        if s.lines().count() > 1 {
            insights.push(s);
        }
    }

    // 2. Dead code — functions with zero callers
    let dead: Vec<serde_json::Value> = db
        .query(
            "SELECT name, file_path, (end_line - start_line) AS size FROM `function` WHERE \
                repo = $repo AND \
                name NOT IN (SELECT VALUE out.name FROM calls WHERE out.name != NONE) \
                AND name != 'main' AND name != 'new' AND name != 'default' \
                AND string::starts_with(name, 'test_') = false \
                AND (end_line - start_line) >= 10 \
                ORDER BY size DESC LIMIT 10",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !dead.is_empty() {
        let mut s = format!("### Dead code ({} unused functions)\n", dead.len());
        for d in dead.iter().take(5) {
            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let file = d.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let size = d.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            s.push_str(&format!("- {} ({}, {} lines)\n", name, file, size));
        }
        insights.push(s);
    }

    // 3. High coupling — files with many shared functions
    let coupling: Vec<serde_json::Value> = db
        .query(
            "SELECT file_path, count() AS fn_count FROM `function` \
                WHERE repo = $repo GROUP BY file_path ORDER BY fn_count DESC LIMIT 3",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !coupling.is_empty() {
        let mut s = "### Most complex files\n".to_string();
        for c in &coupling {
            let file = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let count = c.get("fn_count").and_then(|v| v.as_u64()).unwrap_or(0);
            s.push_str(&format!("- {} ({} functions)\n", file, count));
        }
        insights.push(s);
    }

    // 4. CLAUDE.md check removed — .claude/ is a hidden dir not indexed by tree walker.
    //    CLAUDE.md existence is better checked by the agent itself via filesystem.

    // 5. Skill graph opportunity — suggest based on conversation count
    let conv_count: Vec<serde_json::Value> = db
        .query("SELECT count() FROM decision WHERE repo = $repo GROUP ALL")
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    let decisions = conv_count
        .first()
        .and_then(|v| v.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if decisions >= 3 {
        insights.push(format!(
            "### Skill graph opportunity\n{} decisions recorded. Run `generate_skill_notes` to auto-create a navigable knowledge base from conversation history.\n",
            decisions
        ));
    }

    // 6. Language distribution summary
    let langs: Vec<serde_json::Value> = db
        .query(
            "SELECT language, count() AS cnt FROM file WHERE repo = $repo \
                GROUP BY language ORDER BY cnt DESC LIMIT 5",
        )
        .bind(("repo", repo.to_string()))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    if !langs.is_empty() {
        let lang_summary: Vec<String> = langs
            .iter()
            .filter_map(|l| {
                let lang = l.get("language").and_then(|v| v.as_str())?;
                let cnt = l.get("cnt").and_then(|v| v.as_u64())?;
                Some(format!("{} ({})", lang, cnt))
            })
            .collect();
        insights.push(format!("### Languages\n{}\n", lang_summary.join(", ")));
    }

    if insights.is_empty() {
        String::new()
    } else {
        format!("## Project Insights\n\n{}", insights.join("\n"))
    }
}

/// Create cross-session topic links: sessions discussing the same code entity get co_discusses edges
pub(crate) async fn link_cross_session_topics(db: &Surreal<Db>, _repo: &str) -> usize {
    // Find code entities discussed in multiple sessions
    let query = "SELECT out AS entity, array::group(in) AS sessions \
                 FROM discussed_in \
                 GROUP BY out \
                 HAVING count() > 1 \
                 LIMIT 50;";

    let results: Vec<serde_json::Value> = match db.query(query).await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(_) => return 0,
    };

    let mut link_count = 0;
    for row in &results {
        let sessions = match row.get("sessions").and_then(|v| v.as_array()) {
            Some(s) => s,
            None => continue,
        };
        // Create pairwise co_discusses links (capped at 10 sessions per entity)
        let capped: Vec<_> = sessions.iter().take(10).collect();
        for i in 0..capped.len() {
            for j in (i + 1)..capped.len() {
                let from_id = capped[i].as_str().unwrap_or("");
                let to_id = capped[j].as_str().unwrap_or("");
                if !from_id.is_empty() && !to_id.is_empty() {
                    let q = format!(
                        "LET $existing = (SELECT * FROM co_discusses WHERE in = {} AND out = {} LIMIT 1); \
                         IF !$existing THEN \
                             RELATE {}->co_discusses->{} \
                         END;",
                        from_id, to_id, from_id, to_id
                    );
                    if db.query(&q).await.is_ok() {
                        link_count += 1;
                    }
                }
            }
        }
    }

    link_count
}

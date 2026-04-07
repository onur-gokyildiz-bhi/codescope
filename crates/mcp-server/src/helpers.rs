use std::path::Path;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

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

pub(crate) fn extract_path_from_question(question: &str) -> String {
    for word in question.split_whitespace() {
        let clean = word.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '/' && c != '\\' && c != '.' && c != '_' && c != '-'
        });
        if clean.contains('.')
            && (clean.contains('/')
                || clean.contains('\\')
                || clean.ends_with(".rs")
                || clean.ends_with(".ts")
                || clean.ends_with(".py")
                || clean.ends_with(".go")
                || clean.ends_with(".java")
                || clean.ends_with(".js"))
        {
            return clean.to_string();
        }
    }
    question.to_string()
}

/// Check if a conversation file is already indexed by comparing stored hash
pub(crate) async fn check_conversation_hash(
    db: &Surreal<Db>,
    file_name: &str,
) -> anyhow::Result<Option<String>> {
    #[derive(serde::Deserialize)]
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

    // Recent decisions (last 10)
    let decisions: Vec<serde_json::Value> = db
        .query("SELECT name, body, timestamp FROM decision WHERE repo = $repo ORDER BY timestamp DESC LIMIT 10")
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
            s.push_str(&format!("- {}: {}\n", date, name));
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

/// Post-index project insights — auto-generated recommendations after indexing.
/// Returned as markdown and injected into CONTEXT.md + MCP server instructions.
pub(crate) async fn build_post_index_insights(db: &Surreal<Db>, repo: &str) -> String {
    let mut insights = Vec::new();

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

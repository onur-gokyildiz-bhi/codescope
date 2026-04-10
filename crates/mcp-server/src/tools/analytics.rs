//! Analytics + insight tools: api_changelog, community_detection, export_obsidian,
//! capture_insight, suggest_structure.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::helpers::{build_project_profile, derive_scope_from_file_path};
use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = analytics_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Show recently indexed entities grouped by file
    #[tool(
        description = "Show recently changed, added, or modified functions and classes since last index. \
        Useful before code review or after re-indexing."
    )]
    async fn api_changelog(&self, Parameters(_params): Parameters<ApiChangelogParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let mut output = "## API Changelog\n\n".to_string();

        let q = "SELECT name, file_path, start_line, end_line, signature \
                 FROM `function` WHERE repo = $repo \
                 ORDER BY file_path, start_line LIMIT 200";
        match ctx.db.query(q).bind(("repo", ctx.repo_name.clone())).await {
            Ok(mut r) => {
                let functions: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if functions.is_empty() {
                    output.push_str("No functions found in the index.\n");
                    return output;
                }

                let mut current_file = String::new();
                for f in &functions {
                    let fp = f.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                    if fp != current_file {
                        current_file = fp.to_string();
                        output.push_str(&format!("\n### {}\n", fp));
                    }
                    let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let start = f.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                    let end = f.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0);
                    let lines = end.saturating_sub(start);
                    let sig = f.get("signature").and_then(|v| v.as_str()).unwrap_or("");
                    if sig.is_empty() {
                        output.push_str(&format!("- **{}** (L{}-{}, {} lines)\n", name, start, end, lines));
                    } else {
                        output.push_str(&format!("- **{}** (L{}-{}, {} lines) `{}`\n", name, start, end, lines, sig));
                    }
                }

                let cq = "SELECT name, file_path, start_line, end_line \
                          FROM class WHERE repo = $repo \
                          ORDER BY file_path, start_line LIMIT 100";
                if let Ok(mut cr) = ctx.db.query(cq).bind(("repo", ctx.repo_name.clone())).await {
                    let classes: Vec<serde_json::Value> = cr.take(0).unwrap_or_default();
                    if !classes.is_empty() {
                        output.push_str("\n## Classes\n");
                        let mut current_file2 = String::new();
                        for c in &classes {
                            let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                            if fp != current_file2 {
                                current_file2 = fp.to_string();
                                output.push_str(&format!("\n### {}\n", fp));
                            }
                            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let start = c.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
                            let end = c.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0);
                            let lines = end.saturating_sub(start);
                            output.push_str(&format!("- **{}** (L{}-{}, {} lines)\n", name, start, end, lines));
                        }
                    }
                }
            }
            Err(e) => {
                output.push_str(&format!("Error querying changelog: {}\n", e));
            }
        }

        output
    }

    /// Detect code communities and architectural boundaries
    #[tool(
        description = "Detect code communities, bridge modules, and central nodes in the codebase graph. \
        'clusters' — find groups of tightly-connected files, \
        'bridges' — find modules that connect separate clusters (high betweenness), \
        'central' — find the most connected/important entities (PageRank-like), \
        'all' — run all analyses."
    )]
    async fn community_detection(
        &self,
        Parameters(params): Parameters<CommunityDetectionParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let analysis = params.analysis.as_deref().unwrap_or("all");
        let limit = params.limit.unwrap_or(20);
        let mut output = "## Code Community Analysis\n\n".to_string();

        if analysis == "all" || analysis == "clusters" {
            let q = "SELECT file_path, count(->calls) AS out_calls, count(<-calls) AS in_calls, \
                     (count(->calls) + count(<-calls)) AS total_edges \
                     FROM `function` WHERE file_path != NONE \
                     GROUP BY file_path ORDER BY total_edges DESC LIMIT $lim";
            if let Ok(mut r) = ctx.db.query(q).bind(("lim", limit as i64)).await {
                let clusters: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if !clusters.is_empty() {
                    output.push_str("### Most Connected Files (Cluster Centers)\n\n");
                    output.push_str("| File | Outgoing | Incoming | Total |\n|------|----------|----------|-------|\n");
                    for c in &clusters {
                        let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let out_c = c.get("out_calls").and_then(|v| v.as_u64()).unwrap_or(0);
                        let in_c = c.get("in_calls").and_then(|v| v.as_u64()).unwrap_or(0);
                        let total = c.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);
                        output.push_str(&format!("| {} | {} | {} | {} |\n", fp, out_c, in_c, total));
                    }
                    output.push('\n');
                }
            }
        }

        if analysis == "all" || analysis == "bridges" {
            let q = "SELECT name, file_path, \
                     count(<-calls) AS callers, count(->calls) AS callees, \
                     (count(<-calls) * count(->calls)) AS bridge_score \
                     FROM `function` \
                     WHERE count(<-calls) > 0 AND count(->calls) > 0 \
                     ORDER BY bridge_score DESC LIMIT $lim";
            if let Ok(mut r) = ctx.db.query(q).bind(("lim", limit as i64)).await {
                let bridges: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if !bridges.is_empty() {
                    output.push_str("### Bridge Functions (Connect Different Parts)\n\n");
                    output.push_str("| Function | File | Callers | Callees | Bridge Score |\n|----------|------|---------|---------|-------------|\n");
                    for b in &bridges {
                        let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let fp = b.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let callers = b.get("callers").and_then(|v| v.as_u64()).unwrap_or(0);
                        let callees = b.get("callees").and_then(|v| v.as_u64()).unwrap_or(0);
                        let score = b.get("bridge_score").and_then(|v| v.as_u64()).unwrap_or(0);
                        output.push_str(&format!("| **{}** | {} | {} | {} | {} |\n", name, fp, callers, callees, score));
                    }
                    output.push('\n');
                }
            }
        }

        if analysis == "all" || analysis == "central" {
            let q = "SELECT name, file_path, count(<-calls) AS in_degree \
                     FROM `function` ORDER BY in_degree DESC LIMIT $lim";
            if let Ok(mut r) = ctx.db.query(q).bind(("lim", limit as i64)).await {
                let central: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                if !central.is_empty() {
                    output.push_str("### Most Central Functions (Highest In-Degree)\n\n");
                    output.push_str("| # | Function | File | Called By |\n|---|----------|------|-----------|\n");
                    for (i, c) in central.iter().enumerate() {
                        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
                        let deg = c.get("in_degree").and_then(|v| v.as_u64()).unwrap_or(0);
                        output.push_str(&format!("| {} | **{}** | {} | {} |\n", i + 1, name, fp, deg));
                    }
                }
            }
        }

        output
    }

    /// Export the knowledge graph as an Obsidian-compatible vault with wikilinks
    #[tool(
        description = "Export indexed functions and classes as an Obsidian vault with wikilinks. \
        Creates an index.md listing all entities and individual markdown files for the top 50 \
        most-connected functions (with callers/callees). Output defaults to ~/.codescope/exports/{repo}/."
    )]
    async fn export_obsidian(
        &self,
        Parameters(params): Parameters<ExportObsidianParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let limit = params.limit.unwrap_or(500);

        let output_dir = if let Some(dir) = params.output_dir {
            std::path::PathBuf::from(dir)
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".codescope")
                .join("exports")
                .join(&ctx.repo_name)
        };

        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            return format!("Failed to create output directory: {}", e);
        }

        let fq = "SELECT name, file_path, language, start_line, end_line, signature \
                   FROM `function` WHERE repo = $repo \
                   ORDER BY file_path, start_line LIMIT $lim";
        let functions: Vec<serde_json::Value> = match ctx
            .db
            .query(fq)
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("lim", limit as i64))
            .await
        {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(e) => return format!("Error querying functions: {}", e),
        };

        let cq = "SELECT name, file_path, language, start_line, end_line \
                   FROM class WHERE repo = $repo \
                   ORDER BY file_path, start_line LIMIT $lim";
        let classes: Vec<serde_json::Value> = match ctx
            .db
            .query(cq)
            .bind(("repo", ctx.repo_name.clone()))
            .bind(("lim", limit as i64))
            .await
        {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(e) => return format!("Error querying classes: {}", e),
        };

        let mut index = format!("# {} — Code Index\n\n", ctx.repo_name);

        index.push_str("## Functions\n\n");
        for f in &functions {
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let fp = f.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let line = f.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
            index.push_str(&format!("- [[{}]] (`{}:{}`)\n", name, fp, line));
        }

        index.push_str("\n## Classes\n\n");
        for c in &classes {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let line = c.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
            index.push_str(&format!("- [[{}]] (`{}:{}`)\n", name, fp, line));
        }

        if let Err(e) = std::fs::write(output_dir.join("index.md"), &index) {
            return format!("Error writing index.md: {}", e);
        }
        let mut file_count = 1usize;

        let top_q = "SELECT name, file_path, language, start_line, end_line, signature, \
                      count(<-calls) AS caller_count, count(->calls) AS callee_count, \
                      (count(<-calls) + count(->calls)) AS total_edges \
                      FROM `function` WHERE repo = $repo \
                      ORDER BY total_edges DESC LIMIT 50";
        let top_functions: Vec<serde_json::Value> = match ctx
            .db
            .query(top_q)
            .bind(("repo", ctx.repo_name.clone()))
            .await
        {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        for tf in &top_functions {
            let name = tf.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
            let fp = tf.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let lang = tf.get("language").and_then(|v| v.as_str()).unwrap_or("unknown");
            let start = tf.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0);
            let end = tf.get("end_line").and_then(|v| v.as_u64()).unwrap_or(0);
            let sig = tf.get("signature").and_then(|v| v.as_str()).unwrap_or("");

            let mut md = format!(
                "---\nkind: function\nfile_path: {}\nlanguage: {}\nstart_line: {}\nend_line: {}\n---\n\n",
                fp, lang, start, end
            );
            md.push_str(&format!("## {}\n\n", name));
            if !sig.is_empty() {
                md.push_str(&format!("`{}`\n\n", sig));
            }

            let caller_q = "SELECT <-calls<-`function`.name AS callers FROM `function` \
                            WHERE name = $name AND repo = $repo LIMIT 1";
            if let Ok(mut cr) = ctx
                .db
                .query(caller_q)
                .bind(("name", name.to_string()))
                .bind(("repo", ctx.repo_name.clone()))
                .await
            {
                let rows: Vec<serde_json::Value> = cr.take(0).unwrap_or_default();
                if let Some(row) = rows.first() {
                    if let Some(callers) = row.get("callers").and_then(|v| v.as_array()) {
                        if !callers.is_empty() {
                            md.push_str("### Called By\n\n");
                            for c in callers {
                                if let Some(cn) = c.as_str() {
                                    md.push_str(&format!("- [[{}]]\n", cn));
                                }
                            }
                            md.push('\n');
                        }
                    }
                }
            }

            let callee_q = "SELECT ->calls->`function`.name AS callees FROM `function` \
                            WHERE name = $name AND repo = $repo LIMIT 1";
            if let Ok(mut cr) = ctx
                .db
                .query(callee_q)
                .bind(("name", name.to_string()))
                .bind(("repo", ctx.repo_name.clone()))
                .await
            {
                let rows: Vec<serde_json::Value> = cr.take(0).unwrap_or_default();
                if let Some(row) = rows.first() {
                    if let Some(callees) = row.get("callees").and_then(|v| v.as_array()) {
                        if !callees.is_empty() {
                            md.push_str("### Calls\n\n");
                            for c in callees {
                                if let Some(cn) = c.as_str() {
                                    md.push_str(&format!("- [[{}]]\n", cn));
                                }
                            }
                            md.push('\n');
                        }
                    }
                }
            }

            let safe_name = name.replace(['/', '\\', ':', '<', '>', '|', '?', '*'], "_");
            if let Err(e) = std::fs::write(output_dir.join(format!("{}.md", safe_name)), &md) {
                return format!("Error writing {}.md: {}", safe_name, e);
            }
            file_count += 1;
        }

        format!(
            "Exported {} files to {}\n- index.md with {} functions and {} classes\n- {} individual entity files (top connected functions)",
            file_count,
            output_dir.display(),
            functions.len(),
            classes.len(),
            file_count - 1
        )
    }

    /// Record a decision, problem, solution, correction, or learning insight in real-time
    #[tool(
        description = "Record an insight into the knowledge graph in real-time. Types: decision, problem, solution, correction, learning. \
        Call this after making a decision, encountering a problem, finding a solution, or when the user corrects you (correction). \
        Corrections are especially important — they record what went wrong and the correct approach. \
        The agent field identifies which AI tool recorded this (claude-code, cursor, codex-cli, etc). \
        The insight is stored with timestamp, repo, scope, and optional entity links."
    )]
    async fn capture_insight(
        &self,
        Parameters(params): Parameters<CaptureInsightParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let valid_kinds = ["decision", "problem", "solution", "learning", "correction"];
        let kind = params.kind.to_lowercase();
        if !valid_kinds.contains(&kind.as_str()) {
            return format!(
                "Invalid kind '{}'. Must be one of: {}",
                params.kind,
                valid_kinds.join(", ")
            );
        }

        let table = match kind.as_str() {
            "decision" => "decision",
            "problem" => "problem",
            "solution" => "solution",
            "correction" => "solution",
            "learning" => "conv_topic",
            _ => unreachable!(),
        };

        let agent = params.agent.clone().unwrap_or_else(|| "unknown".to_string());

        let scope = params.file_path.as_deref().map(derive_scope_from_file_path);

        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let slug = params
            .summary
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
            .replace("__", "_")
            .trim_matches('_')
            .chars()
            .take(60)
            .collect::<String>();
        let qname = format!("{}:insight:{}:{}", ctx.repo_name, kind, slug);

        let body = if let Some(detail) = &params.detail {
            format!("{}\n\n{}", params.summary, detail)
        } else {
            params.summary.clone()
        };

        let esc = |s: &str| s.replace('\'', "\\'");

        let create_query = format!(
            "CREATE {table} SET \
             name = '{name}', \
             qualified_name = '{qname}', \
             kind = '{kind}', \
             file_path = '{file_path}', \
             repo = '{repo}', \
             language = 'insight', \
             start_line = 0, \
             end_line = 0, \
             body = '{body}', \
             timestamp = '{ts}', \
             scope = '{scope}', \
             agent = '{agent}';",
            table = table,
            name = esc(&params.summary),
            qname = esc(&qname),
            kind = esc(&kind),
            file_path = esc(params.file_path.as_deref().unwrap_or("")),
            repo = esc(&ctx.repo_name),
            body = esc(&body),
            ts = timestamp,
            scope = esc(scope.as_deref().unwrap_or("root")),
            agent = esc(&agent),
        );

        if let Err(e) = ctx.db.query(&create_query).await {
            return format!("Error storing insight: {}", e);
        }

        if let Some(entity_name) = &params.entity_name {
            let rel_kind = match kind.as_str() {
                "decision" => "decided_about",
                _ => "discussed_in",
            };

            let find_query = format!(
                "SELECT id FROM `function` WHERE name = '{}' AND repo = '{}' LIMIT 1; \
                 SELECT id FROM class WHERE name = '{}' AND repo = '{}' LIMIT 1; \
                 SELECT id FROM config WHERE name = '{}' AND repo = '{}' LIMIT 1;",
                esc(entity_name),
                esc(&ctx.repo_name),
                esc(entity_name),
                esc(&ctx.repo_name),
                esc(entity_name),
                esc(&ctx.repo_name),
            );

            if let Ok(mut resp) = ctx.db.query(&find_query).await {
                let mut target_id = None;
                for i in 0..3u32 {
                    let results: Vec<serde_json::Value> = resp.take(i as usize).unwrap_or_default();
                    if let Some(first) = results.first() {
                        if let Some(id) = first.get("id") {
                            target_id = Some(id.to_string());
                            break;
                        }
                    }
                }

                if let Some(target) = target_id {
                    let relate_query = format!(
                        "LET $insight = (SELECT id FROM {table} WHERE qualified_name = '{qname}' LIMIT 1); \
                         IF $insight THEN \
                             RELATE $insight[0].id->{rel}->{target} \
                         END;",
                        table = table,
                        qname = esc(&qname),
                        rel = rel_kind,
                        target = target.trim_matches('"'),
                    );
                    let _ = ctx.db.query(&relate_query).await;
                }
            }
        }

        let mut confirmation = format!(
            "Captured {} insight: \"{}\"\n- Repo: {}\n- Agent: {}\n- Timestamp: {}",
            kind, params.summary, ctx.repo_name, agent, timestamp
        );
        if let Some(scope) = &scope {
            confirmation.push_str(&format!("\n- Scope: {}", scope));
        }
        if let Some(entity) = &params.entity_name {
            confirmation.push_str(&format!("\n- Linked to entity: {}", entity));
        }
        confirmation
    }

    /// Suggest a project directory structure for new/empty projects, or return the project profile if already indexed.
    #[tool(
        description = "Suggest a directory structure for a new project based on language and description. \
        If the project is already indexed (has entities), returns the Project Profile instead. \
        For empty/new projects, reads README.md or DESIGN.md if available and suggests a \
        language-appropriate directory layout."
    )]
    async fn suggest_structure(
        &self,
        Parameters(params): Parameters<SuggestStructureParams>,
    ) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };

        let entity_count: Vec<serde_json::Value> = ctx
            .db
            .query(
                "SELECT \
                 (SELECT count() FROM `function` WHERE repo = $repo GROUP ALL)[0].count AS fn_count, \
                 (SELECT count() FROM class WHERE repo = $repo GROUP ALL)[0].count AS cls_count",
            )
            .bind(("repo", ctx.repo_name.clone()))
            .await
            .ok()
            .and_then(|mut r| r.take(0).ok())
            .unwrap_or_default();

        let has_entities = entity_count
            .first()
            .map(|row| {
                let fns = row.get("fn_count").and_then(|v| v.as_u64()).unwrap_or(0);
                let cls = row.get("cls_count").and_then(|v| v.as_u64()).unwrap_or(0);
                fns > 0 || cls > 0
            })
            .unwrap_or(false);

        if has_entities {
            let profile = build_project_profile(&ctx.db, &ctx.repo_name).await;
            if profile.is_empty() {
                return "Project is indexed but no profile data available. Try re-indexing.".into();
            }
            return format!("Project already indexed. Here is the current profile:\n\n{}", profile);
        }

        let mut context_from_files = String::new();

        for filename in &["README.md", "DESIGN.md"] {
            let file_path = ctx.codebase_path.join(filename);
            if file_path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    let truncated: String = content.chars().take(2000).collect();
                    context_from_files.push_str(&format!("### From {}\n{}\n\n", filename, truncated));
                }
            }
        }

        let docs_path = ctx.codebase_path.join("docs");
        if docs_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&docs_path) {
                for entry in entries.flatten().take(3) {
                    let path = entry.path();
                    if path.extension().map(|e| e == "md").unwrap_or(false) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("doc");
                            let truncated: String = content.chars().take(1000).collect();
                            context_from_files.push_str(&format!("### From docs/{}\n{}\n\n", fname, truncated));
                        }
                    }
                }
            }
        }

        let lang = if let Some(ref l) = params.language {
            l.to_lowercase()
        } else {
            let codebase = &ctx.codebase_path;
            if codebase.join("Cargo.toml").is_file() {
                "rust".to_string()
            } else if codebase.join("package.json").is_file() {
                if codebase.join("tsconfig.json").is_file() {
                    "typescript".to_string()
                } else {
                    "javascript".to_string()
                }
            } else if codebase.join("pyproject.toml").is_file() || codebase.join("requirements.txt").is_file() {
                "python".to_string()
            } else if codebase.join("pubspec.yaml").is_file() {
                "dart".to_string()
            } else if codebase.join("go.mod").is_file() {
                "go".to_string()
            } else if codebase.join("Project.csproj").is_file() || codebase.join("*.sln").is_file() {
                "csharp".to_string()
            } else {
                "unknown".to_string()
            }
        };

        let suggestion = match lang.as_str() {
            "rust" => "Suggested structure:\n```\nsrc/\n    main.rs\n    lib.rs\n    config.rs\n    error.rs\n    routes/\n        mod.rs\n        health.rs\nCargo.toml\n```",
            "typescript" | "javascript" => "Suggested structure:\n```\nsrc/\n    index.ts\n    config/\n    routes/\n    services/\n    models/\n    utils/\npackage.json\ntsconfig.json\n```",
            "python" => "Suggested structure:\n```\nsrc/\n    __init__.py\n    main.py\n    config.py\n    routes/\n    services/\n    models/\n    utils/\nrequirements.txt\npyproject.toml\n```",
            "dart" | "flutter" => "Suggested structure:\n```\nlib/\n    main.dart\n    core/\n    features/\n    shared/\npubspec.yaml\n```",
            "go" => "Suggested structure:\n```\ncmd/\n    server/\n        main.go\ninternal/\n    config/\n    handler/\n    service/\n    model/\n    repository/\npkg/\ngo.mod\n```",
            "csharp" | "c#" => "Suggested structure:\n```\nsrc/\n    Program.cs\n    Controllers/\n    Services/\n    Models/\n    Data/\n    Middleware/\nTests/\nProject.csproj\n```",
            _ => "Suggested structure:\n```\nsrc/\n    main\n    config/\n    core/\n    services/\n    models/\n    utils/\ntests/\ndocs/\n```",
        };

        let mut output = "# Project Structure Suggestion\n\n".to_string();
        output.push_str(&format!("**Detected language**: {}\n", lang));
        if let Some(ref desc) = params.description {
            output.push_str(&format!("**Goal**: {}\n", desc));
        }
        output.push('\n');
        output.push_str(suggestion);

        if !context_from_files.is_empty() {
            output.push_str("\n\n## Existing Documentation\n\n");
            output.push_str(&context_from_files);
        }

        output
    }
}

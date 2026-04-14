//! Skill / knowledge graph tool: single `skills` tool with action dispatch.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = skills_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Unified skills tool: index markdown folder, traverse with progressive detail, or auto-generate notes.
    #[tool(
        description = "Skills/knowledge graph: action=index|traverse|generate. index: parse markdown folder. traverse: navigate with detail 1-4. generate: auto-generate notes from conversations."
    )]
    async fn skills(&self, Parameters(params): Parameters<SkillsParams>) -> String {
        match params.action.as_str() {
            "index" => {
                let path = match params.path {
                    Some(p) => p,
                    None => {
                        return "Error: 'index' action requires 'path' (folder of markdown files)"
                            .to_string()
                    }
                };
                self.skills_index(path, params.clean.unwrap_or(false)).await
            }
            "traverse" => {
                let name = match params.path {
                    Some(p) => p,
                    None => {
                        return "Error: 'traverse' action requires 'path' (skill name)".to_string()
                    }
                };
                let detail = params.detail.unwrap_or(2) as usize;
                self.skills_traverse(name, detail).await
            }
            "generate" => self.skills_generate(params.path).await,
            other => format!(
                "Error: unknown action '{}'. Expected: index | traverse | generate",
                other
            ),
        }
    }
}

impl GraphRagServer {
    async fn skills_index(&self, path: String, clean: bool) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let target_path = ctx.codebase_path.join(&path);

        if !target_path.is_dir() {
            return format!("Path '{}' is not a directory", target_path.display());
        }

        if clean {
            let _ = ctx
                .db
                .query("DELETE FROM skill; DELETE FROM links_to;")
                .await;
        }

        let parser = codescope_core::parser::CodeParser::new();
        let builder = codescope_core::graph::builder::GraphBuilder::new(ctx.db.clone());
        let repo_name = ctx.repo_name.clone();
        let base = target_path.clone();

        let walker = ignore::WalkBuilder::new(&target_path)
            .hidden(true)
            .git_ignore(true)
            .build();

        let mut file_count = 0;
        let mut skill_count = 0;
        let mut link_count = 0;
        let mut errors = Vec::new();

        for entry in walker.flatten() {
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "md" && ext != "mdx" {
                continue;
            }

            let rel_path = path
                .strip_prefix(&base)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string()
                .replace('\\', "/");

            match std::fs::read_to_string(path) {
                Ok(content) => {
                    match parser.parse_source(std::path::Path::new(&rel_path), &content, &repo_name)
                    {
                        Ok((entities, relations)) => {
                            let skills = entities
                                .iter()
                                .filter(|e| {
                                    matches!(
                                        e.kind,
                                        codescope_core::EntityKind::SkillNode
                                            | codescope_core::EntityKind::SkillMOC
                                    )
                                })
                                .count();
                            let links = relations
                                .iter()
                                .filter(|r| matches!(r.kind, codescope_core::RelationKind::LinksTo))
                                .count();

                            if let Err(e) = builder.insert_entities(&entities).await {
                                tracing::warn!("Entity insert failed: {e}");
                            }
                            if let Err(e) = builder.insert_relations(&relations).await {
                                tracing::warn!("Relation insert failed: {e}");
                            }

                            file_count += 1;
                            skill_count += skills;
                            link_count += links;
                        }
                        Err(e) => errors.push(format!("{}: {}", rel_path, e)),
                    }
                }
                Err(e) => errors.push(format!("{}: {}", rel_path, e)),
            }
        }

        let mut output = format!(
            "Skill graph indexed: {} files, {} skill nodes, {} wikilinks",
            file_count, skill_count, link_count,
        );
        if !errors.is_empty() {
            output.push_str(&format!("\n\nErrors ({}):\n", errors.len()));
            for err in errors.iter().take(5) {
                output.push_str(&format!("- {}\n", err));
            }
        }
        output
    }

    async fn skills_traverse(&self, name: String, detail: usize) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let gq = GraphQuery::new(ctx.db);
        let depth = 1usize;

        match gq.traverse_skill_graph(&name, depth, detail).await {
            Ok(result) => {
                if result.get("error").is_some() {
                    return result["error"].as_str().unwrap_or("Not found").to_string();
                }

                let mut output = String::new();

                if let Some(skill) = result.get("skill") {
                    let sname = skill.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let desc = skill
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let ntype = skill
                        .get("node_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("skill");
                    output.push_str(&format!("# {} [{}]\n\n", sname, ntype));
                    if !desc.is_empty() {
                        output.push_str(&format!("{}\n\n", desc));
                    }
                }

                if let Some(links) = result.get("links_to").and_then(|v| v.as_array()) {
                    if !links.is_empty() {
                        output.push_str("## Links To\n\n");
                        for link in links {
                            let lname = link.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let desc = link
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let ctx_s = link.get("context").and_then(|v| v.as_str()).unwrap_or("");
                            output.push_str(&format!("- [[{}]]", lname));
                            if !desc.is_empty() {
                                output.push_str(&format!(" — {}", desc));
                            }
                            if !ctx_s.is_empty() {
                                output.push_str(&format!("\n  > {}", ctx_s));
                            }
                            output.push('\n');
                        }
                        output.push('\n');
                    }
                }

                if let Some(links) = result.get("linked_from").and_then(|v| v.as_array()) {
                    if !links.is_empty() {
                        output.push_str("## Linked From\n\n");
                        for link in links {
                            let lname = link.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let desc = link
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            output.push_str(&format!("- [[{}]]", lname));
                            if !desc.is_empty() {
                                output.push_str(&format!(" — {}", desc));
                            }
                            output.push('\n');
                        }
                        output.push('\n');
                    }
                }

                if let Some(sections) = result.get("sections").and_then(|v| v.as_array()) {
                    if !sections.is_empty() {
                        output.push_str("## Sections\n\n");
                        for sec in sections {
                            let sname = sec.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            output.push_str(&format!("- {}\n", sname));
                        }
                    }
                }

                if output.is_empty() {
                    format!("No skill node found matching '{}'", name)
                } else {
                    output
                }
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    async fn skills_generate(&self, output_dir_override: Option<String>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let output_dir = ctx
            .codebase_path
            .join(output_dir_override.as_deref().unwrap_or("skills"));

        let mut response = match ctx
            .db
            .query(
                "SELECT name, body, kind, timestamp FROM decision; \
             SELECT name, body, kind, timestamp FROM problem; \
             SELECT name, body, kind, timestamp FROM solution;",
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return format!("Error querying conversations: {}", e),
        };

        let decisions: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
        let problems: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
        let solutions: Vec<serde_json::Value> = response.take(2).unwrap_or_default();

        let mut segments = Vec::new();
        for (kind, items) in [
            ("decision", &decisions),
            ("problem", &problems),
            ("solution", &solutions),
        ] {
            for item in items {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let body = item
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let ts = item
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if !name.is_empty() {
                    segments.push((kind.to_string(), name, body, ts));
                }
            }
        }

        if segments.is_empty() {
            return "No conversation segments found. Run conversations(action=\"index\") first."
                .into();
        }

        let code_refs: Vec<String> = match ctx
            .db
            .query("SELECT VALUE name FROM `function` LIMIT 200")
            .await
        {
            Ok(mut r) => r.take(0).unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        let files = codescope_core::conversation::generate_skill_notes(&segments, &code_refs);

        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            return format!("Cannot create output directory: {}", e);
        }

        let mut written = 0;
        for (filename, content) in &files {
            let path = output_dir.join(filename);
            if let Err(e) = std::fs::write(&path, content) {
                return format!("Error writing {}: {}", filename, e);
            }
            written += 1;
        }

        format!(
            "Generated {} skill notes in {}\n\nFiles:\n{}",
            written,
            output_dir.display(),
            files
                .iter()
                .map(|(f, _)| format!("- {}", f))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

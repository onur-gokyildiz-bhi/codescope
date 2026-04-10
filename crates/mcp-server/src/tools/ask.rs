//! Natural language query tool: ask
//! Uses the nlp module to parse questions into structured intents and routes
//! them through GraphQuery delegation.

use codescope_core::graph::query::GraphQuery;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::nlp;
use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = ask_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Translate a natural language question to a SurrealQL query and execute it
    #[tool(
        description = "Ask a question about the codebase in natural language. Understands intent and extracts search terms intelligently. \
        Examples: 'find functions related to binary quantize', 'what calls parse_file?', 'how many classes?', \
        'functions in main.rs', 'largest functions', 'who calls embed_functions?'"
    )]
    async fn ask(&self, Parameters(params): Parameters<NaturalLanguageQueryParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let question = params.question.to_lowercase();
        let gq = GraphQuery::new(ctx.db.clone());

        let parsed = nlp::parse_question(&question);
        tracing::debug!("NLP parsed: {:?}", parsed);

        match parsed.intent {
            nlp::Intent::Count(entity) => {
                let table = match entity {
                    nlp::Entity::Function => "`function`",
                    nlp::Entity::Class => "class",
                    nlp::Entity::File => "file",
                    nlp::Entity::Any => "`function`",
                };
                let surql = format!("SELECT count() AS total FROM {} GROUP ALL", table);
                match ctx.db.query(&surql).await {
                    Ok(mut r) => {
                        let rows: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        format!(
                            "**Query:** `{}`\n\n**Results:**\n{}",
                            surql,
                            serde_json::to_string_pretty(&rows).unwrap_or_default()
                        )
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }

            nlp::Intent::ListAll(entity) => {
                let surql = match entity {
                    nlp::Entity::Function => "SELECT name, file_path, start_line, signature FROM `function` ORDER BY name LIMIT 50",
                    nlp::Entity::Class => "SELECT name, kind, file_path, start_line FROM class ORDER BY name LIMIT 50",
                    nlp::Entity::File => "SELECT path, language, line_count FROM file ORDER BY path LIMIT 50",
                    nlp::Entity::Any => "SELECT name, file_path, start_line, signature FROM `function` ORDER BY name LIMIT 50",
                };
                match ctx.db.query(surql).await {
                    Ok(mut r) => {
                        let rows: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        format!(
                            "**Query:** `{}`\n\n**Results ({}):**\n{}",
                            surql,
                            rows.len(),
                            serde_json::to_string_pretty(&rows).unwrap_or_default()
                        )
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }

            nlp::Intent::Search(entity) => {
                if parsed.search_terms.is_empty() {
                    return "No search terms extracted from question. Try: 'find functions related to <keyword>'".into();
                }

                let mut all_results: Vec<serde_json::Value> = Vec::new();
                let search_functions = matches!(entity, nlp::Entity::Function | nlp::Entity::Any);
                let search_classes = matches!(entity, nlp::Entity::Class | nlp::Entity::Any);

                for term in &parsed.search_terms {
                    let term_owned = term.clone();
                    if search_functions {
                        let surql = "SELECT name, qualified_name, file_path, start_line, end_line, signature \
                                     FROM `function` WHERE string::contains(string::lowercase(name), $term) \
                                     OR string::contains(string::lowercase(qualified_name ?? ''), $term) \
                                     OR string::contains(string::lowercase(signature ?? ''), $term) \
                                     LIMIT 25";
                        if let Ok(mut r) =
                            ctx.db.query(surql).bind(("term", term_owned.clone())).await
                        {
                            let rows: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                            all_results.extend(rows);
                        }
                    }
                    if search_classes {
                        let surql = "SELECT name, qualified_name, kind, file_path, start_line, end_line \
                                     FROM class WHERE string::contains(string::lowercase(name), $term) \
                                     OR string::contains(string::lowercase(qualified_name ?? ''), $term) \
                                     LIMIT 25";
                        if let Ok(mut r) = ctx.db.query(surql).bind(("term", term_owned)).await {
                            let rows: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                            all_results.extend(rows);
                        }
                    }
                }

                let mut seen = std::collections::HashSet::new();
                all_results.retain(|v| {
                    let key = format!(
                        "{}:{}",
                        v.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                        v.get("file_path").and_then(|p| p.as_str()).unwrap_or("")
                    );
                    seen.insert(key)
                });

                all_results.sort_by(|a, b| {
                    let score = |v: &serde_json::Value| -> usize {
                        let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_lowercase();
                        let qname = v.get("qualified_name").and_then(|n| n.as_str()).unwrap_or("").to_lowercase();
                        parsed
                            .search_terms
                            .iter()
                            .filter(|t| name.contains(t.as_str()) || qname.contains(t.as_str()))
                            .count()
                    };
                    score(b).cmp(&score(a))
                });

                all_results.truncate(30);

                let terms_str = parsed.search_terms.join(", ");
                format!(
                    "**Search terms:** [{}]\n**Results ({}):**\n{}",
                    terms_str,
                    all_results.len(),
                    serde_json::to_string_pretty(&all_results).unwrap_or_default()
                )
            }

            nlp::Intent::CallGraph(direction) => {
                let func_name = parsed.search_terms.first().map(|s| s.as_str()).unwrap_or("");
                if func_name.is_empty() {
                    return "Which function? Try: 'what calls parse_file?' or 'call graph for embed_functions'".into();
                }
                match direction {
                    nlp::CallDirection::Callers => match gq.find_callers(func_name).await {
                        Ok(results) => {
                            let mut out =
                                format!("**Callers of `{}`** ({}):\n\n", func_name, results.len());
                            for r in &results {
                                out.push_str(&format!(
                                    "- `{}` in {} (line {})\n",
                                    r.name.as_deref().unwrap_or("?"),
                                    r.file_path.as_deref().unwrap_or("?"),
                                    r.start_line.map(|l| l.to_string()).unwrap_or_else(|| "?".into())
                                ));
                            }
                            if results.is_empty() {
                                out.push_str("No callers found.\n");
                            }
                            out
                        }
                        Err(e) => format!("Error finding callers: {}", e),
                    },
                    nlp::CallDirection::Callees => match gq.find_callees(func_name).await {
                        Ok(results) => {
                            let mut out =
                                format!("**`{}` calls** ({}):\n\n", func_name, results.len());
                            for r in &results {
                                out.push_str(&format!(
                                    "- `{}` in {} (line {})\n",
                                    r.name.as_deref().unwrap_or("?"),
                                    r.file_path.as_deref().unwrap_or("?"),
                                    r.start_line.map(|l| l.to_string()).unwrap_or_else(|| "?".into())
                                ));
                            }
                            if results.is_empty() {
                                out.push_str("No callees found.\n");
                            }
                            out
                        }
                        Err(e) => format!("Error finding callees: {}", e),
                    },
                    nlp::CallDirection::Both => {
                        let callers = gq.find_callers(func_name).await.unwrap_or_default();
                        let callees = gq.find_callees(func_name).await.unwrap_or_default();
                        let mut out = format!("**Call graph for `{}`:**\n\n", func_name);
                        out.push_str(&format!("### Callers ({}):\n", callers.len()));
                        for r in &callers {
                            out.push_str(&format!("- `{}`\n", r.name.as_deref().unwrap_or("?")));
                        }
                        out.push_str(&format!("\n### Callees ({}):\n", callees.len()));
                        for r in &callees {
                            out.push_str(&format!("- `{}`\n", r.name.as_deref().unwrap_or("?")));
                        }
                        out
                    }
                }
            }

            nlp::Intent::InFile => {
                let path = parsed
                    .file_path
                    .as_deref()
                    .or_else(|| parsed.search_terms.first().map(|s| s.as_str()))
                    .unwrap_or("");
                if path.is_empty() {
                    return "Which file? Try: 'functions in src/main.rs'".into();
                }
                match gq.file_entities(path).await {
                    Ok(results) => {
                        let mut out =
                            format!("**Entities in `{}`** ({}):\n\n", path, results.len());
                        for r in &results {
                            out.push_str(&format!(
                                "- `{}` (line {}–{})\n",
                                r.name.as_deref().unwrap_or("?"),
                                r.start_line.map(|l| l.to_string()).unwrap_or_else(|| "?".into()),
                                r.end_line.map(|l| l.to_string()).unwrap_or_else(|| "?".into())
                            ));
                        }
                        if results.is_empty() {
                            out.push_str("No entities found. Check the file path.\n");
                        }
                        out
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }

            nlp::Intent::Largest => {
                let surql = "SELECT name, file_path, start_line, end_line, \
                             (end_line - start_line) AS size \
                             FROM `function` WHERE end_line != NONE AND start_line != NONE \
                             ORDER BY (end_line - start_line) DESC LIMIT 15";
                match ctx.db.query(surql).await {
                    Ok(mut r) => {
                        let rows: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        format!(
                            "**Largest functions ({}):**\n{}",
                            rows.len(),
                            serde_json::to_string_pretty(&rows).unwrap_or_default()
                        )
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }

            nlp::Intent::Imports => {
                let surql = "SELECT name, file_path FROM import_decl ORDER BY file_path LIMIT 50";
                match ctx.db.query(surql).await {
                    Ok(mut r) => {
                        let rows: Vec<serde_json::Value> = r.take(0).unwrap_or_default();
                        format!(
                            "**Imports ({}):**\n{}",
                            rows.len(),
                            serde_json::to_string_pretty(&rows).unwrap_or_default()
                        )
                    }
                    Err(e) => format!("Error: {}", e),
                }
            }
        }
    }
}

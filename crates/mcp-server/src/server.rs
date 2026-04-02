use rmcp::model::*;
use rmcp::{ServerHandler, tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use codescope_core::graph::query::GraphQuery;

/// The MCP server for Code Graph RAG
#[derive(Clone)]
pub struct GraphRagServer {
    db: Surreal<Db>,
    repo_name: String,
    codebase_path: PathBuf,
}

// Parameter structs for MCP tools

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchParams {
    /// The search query (function/class name or pattern)
    pub query: String,
    /// Maximum number of results (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FileEntitiesParams {
    /// Path to the file to inspect
    pub file_path: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindCallersParams {
    /// Name of the function to find callers for
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindCalleesParams {
    /// Name of the function to find callees for
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RawQueryParams {
    /// SurrealQL query to execute
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexParams {
    /// Path to index (relative to codebase root)
    pub path: Option<String>,
    /// Clear existing data before indexing
    pub clean: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ImpactAnalysisParams {
    /// Name of the function to analyze impact for
    pub function_name: String,
    /// Depth of the call graph to traverse (default: 3)
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NaturalLanguageQueryParams {
    /// Natural language question about the codebase
    pub question: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SyncHistoryParams {
    /// Path to the git repository
    pub git_path: Option<String>,
    /// Number of recent commits to sync (default: 200)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HotspotParams {
    /// Minimum risk score threshold (default: 0)
    pub min_score: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChurnParams {
    /// Number of top churned files to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CouplingParams {
    /// Number of top coupled file pairs to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DiffReviewParams {
    /// Git ref to diff against (e.g., "main", "HEAD~3", commit hash)
    pub base_ref: String,
    /// Optional head ref (default: HEAD)
    pub head_ref: Option<String>,
}

impl GraphRagServer {
    pub fn new(db: Surreal<Db>, repo_name: String, codebase_path: PathBuf) -> Self {
        Self {
            db,
            repo_name,
            codebase_path,
        }
    }

    fn gq(&self) -> GraphQuery {
        GraphQuery::new(self.db.clone())
    }
}

#[tool(tool_box)]
impl ServerHandler for GraphRagServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Code Graph RAG — Intelligent code knowledge graph. \
                 Search, analyze, and query your codebase using a graph database. \
                 Supports semantic search, call graph analysis, impact analysis, and more."
                    .into(),
            ),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                ..Default::default()
            },
            ..ServerInfo::default()
        }
    }
}

#[tool(tool_box)]
impl GraphRagServer {
    /// Search for functions by name or pattern in the code graph
    #[tool(description = "Search for functions by name or pattern. Returns matching functions with file paths and line numbers.")]
    async fn search_functions(&self, #[tool(aggr)] params: SearchParams) -> String {
        let limit = params.limit.unwrap_or(20);
        let gq = self.gq();

        match gq.search_functions(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No functions found matching '{}'", params.query);
                }
                let mut output = format!("Found {} functions matching '{}':\n\n", results.len().min(limit), params.query);
                for (i, r) in results.iter().enumerate().take(limit) {
                    output.push_str(&format!(
                        "{}. **{}** ({}:{})\n",
                        i + 1,
                        r.name.as_deref().unwrap_or("?"),
                        r.file_path.as_deref().unwrap_or("?"),
                        r.start_line.unwrap_or(0),
                    ));
                    if let Some(sig) = &r.signature {
                        output.push_str(&format!("   `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Error searching: {}", e),
        }
    }

    /// Find a function by exact name
    #[tool(description = "Find a function by exact name. Returns detailed info including signature, file path, and line numbers.")]
    async fn find_function(&self, #[tool(aggr)] params: SearchParams) -> String {
        let gq = self.gq();

        match gq.find_function(&params.query).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No function found with name '{}'", params.query);
                }
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "Error formatting results".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// List all code entities (functions, classes) in a specific file
    #[tool(description = "List all functions and classes in a file. Provides an overview of the file's structure.")]
    async fn file_entities(&self, #[tool(aggr)] params: FileEntitiesParams) -> String {
        let gq = self.gq();

        match gq.file_entities(&params.file_path).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No entities found in '{}'", params.file_path);
                }
                let mut output = format!("Entities in {}:\n\n", params.file_path);
                for r in &results {
                    output.push_str(&format!(
                        "- **{}** (L{}-{})\n",
                        r.name.as_deref().unwrap_or("?"),
                        r.start_line.unwrap_or(0),
                        r.end_line.unwrap_or(0),
                    ));
                    if let Some(sig) = &r.signature {
                        output.push_str(&format!("  `{}`\n", sig));
                    }
                }
                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find all functions that call the specified function (callers / incoming calls)
    #[tool(description = "Find all functions that call the specified function. Useful for understanding who depends on a function.")]
    async fn find_callers(&self, #[tool(aggr)] params: FindCallersParams) -> String {
        let gq = self.gq();

        match gq.find_callers(&params.function_name).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No callers found for '{}'", params.function_name);
                }
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Find all functions called by the specified function (callees / outgoing calls)
    #[tool(description = "Find all functions called by the specified function. Useful for understanding a function's dependencies.")]
    async fn find_callees(&self, #[tool(aggr)] params: FindCalleesParams) -> String {
        let gq = self.gq();

        match gq.find_callees(&params.function_name).await {
            Ok(results) => {
                if results.is_empty() {
                    return format!("No callees found for '{}'", params.function_name);
                }
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Get statistics about the indexed code graph
    #[tool(description = "Get statistics about the code graph: number of files, functions, classes, and relationships indexed.")]
    async fn graph_stats(&self) -> String {
        let gq = self.gq();

        match gq.stats().await {
            Ok(stats) => {
                serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "Error formatting stats".into())
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// Execute a raw SurrealQL query against the code graph
    #[tool(description = "Execute a raw SurrealQL query against the code graph database. Use for advanced queries like graph traversals.")]
    async fn raw_query(&self, #[tool(aggr)] params: RawQueryParams) -> String {
        let gq = self.gq();

        match gq.raw_query(&params.query).await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Error formatting".into())
            }
            Err(e) => format!("Query error: {}", e),
        }
    }

    /// Index or re-index the codebase into the graph database
    #[tool(description = "Index the codebase into the knowledge graph. Parses source files and extracts entities and relationships.")]
    async fn index_codebase(&self, #[tool(aggr)] params: IndexParams) -> String {
        let target_path = match &params.path {
            Some(p) => self.codebase_path.join(p),
            None => self.codebase_path.clone(),
        };

        let parser = codescope_core::parser::CodeParser::new();
        let builder = codescope_core::graph::builder::GraphBuilder::new(self.db.clone());

        if params.clean.unwrap_or(false) {
            if let Err(e) = builder.clear_repo(&self.repo_name).await {
                return format!("Error clearing repo: {}", e);
            }
        }

        let walker = ignore::WalkBuilder::new(&target_path)
            .hidden(true)
            .git_ignore(true)
            .build();

        let mut files = 0;
        let mut entities = 0;
        let mut relations = 0;
        let mut errors = Vec::new();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }
            let file_path = entry.path();
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !parser.supports_extension(ext) {
                continue;
            }

            match parser.parse_file(file_path, &self.repo_name) {
                Ok((ents, rels)) => {
                    entities += ents.len();
                    relations += rels.len();
                    let _ = builder.insert_entities(&ents).await;
                    let _ = builder.insert_relations(&rels).await;
                    files += 1;
                }
                Err(e) => {
                    errors.push(format!("{}: {}", file_path.display(), e));
                }
            }
        }

        let mut output = format!(
            "Indexing complete!\n- Files: {}\n- Entities: {}\n- Relations: {}",
            files, entities, relations
        );
        if !errors.is_empty() {
            output.push_str(&format!("\n- Errors: {}", errors.len()));
        }
        output
    }

    /// Analyze the impact of changing a function — what else could be affected
    #[tool(description = "Analyze the impact of changing a function. Shows the transitive call graph to understand what would be affected by a change.")]
    async fn impact_analysis(&self, #[tool(aggr)] params: ImpactAnalysisParams) -> String {
        let _depth = params.depth.unwrap_or(3);

        // Build transitive callers up to `depth` levels
        let query = format!(
            "SELECT name, qualified_name, file_path, start_line FROM `function` WHERE name = $name;\
             SELECT <-calls<-`function`.name AS direct_callers FROM `function` WHERE name = $name;\
             SELECT <-calls<-`function`<-calls<-`function`.name AS indirect_callers FROM `function` WHERE name = $name;"
        );

        let name = params.function_name.clone();
        match self.db.query(query).bind(("name", name)).await {
            Ok(mut response) => {
                let func_info: Vec<serde_json::Value> = response.take(0).unwrap_or_default();
                let direct: Vec<serde_json::Value> = response.take(1).unwrap_or_default();
                let indirect: Vec<serde_json::Value> = response.take(2).unwrap_or_default();

                let mut output = format!("## Impact Analysis: {}\n\n", params.function_name);

                if let Some(info) = func_info.first() {
                    output.push_str(&format!("**Location:** {}:{}\n\n",
                        info.get("file_path").and_then(|v| v.as_str()).unwrap_or("?"),
                        info.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0),
                    ));
                }

                output.push_str("### Direct Callers\n");
                if direct.is_empty() {
                    output.push_str("None found\n");
                } else {
                    output.push_str(&serde_json::to_string_pretty(&direct).unwrap_or_default());
                }

                output.push_str("\n\n### Indirect Callers (2 hops)\n");
                if indirect.is_empty() {
                    output.push_str("None found\n");
                } else {
                    output.push_str(&serde_json::to_string_pretty(&indirect).unwrap_or_default());
                }

                output
            }
            Err(e) => format!("Error: {}", e),
        }
    }

    /// List all supported programming languages
    #[tool(description = "List all programming languages supported by the code graph parser.")]
    async fn supported_languages(&self) -> String {
        let parser = codescope_core::parser::CodeParser::new();
        let languages = parser.supported_languages();
        format!("Supported languages: {}", languages.join(", "))
    }

    /// Sync git commit history into the graph database for temporal analysis
    #[tool(description = "Sync git commit history into the graph database. Enables temporal queries like hotspot detection, change coupling, and code evolution tracking.")]
    async fn sync_git_history(&self, #[tool(aggr)] params: SyncHistoryParams) -> String {
        let git_path = params.git_path
            .map(|p| self.codebase_path.join(p))
            .unwrap_or_else(|| self.codebase_path.clone());
        let limit = params.limit.unwrap_or(200);

        // Fetch commits in a blocking task (git2 is !Send)
        let commits = match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.recent_commits(limit)
        }).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => return format!("Error reading git history: {}", e),
            Err(e) => return format!("Task error: {}", e),
        };

        let sync = codescope_core::temporal::TemporalGraphSync::new(self.db.clone());
        match sync.sync_commit_data(&commits, &self.repo_name).await {
            Ok(count) => format!("Synced {} commits into the graph database", count),
            Err(e) => format!("Error syncing commits: {}", e),
        }
    }

    /// Detect code hotspots — files/functions with high complexity AND high churn
    #[tool(description = "Detect code hotspots: functions with high complexity and high change frequency. These are high-risk areas that may need refactoring.")]
    async fn hotspot_detection(&self, #[tool(aggr)] params: HotspotParams) -> String {
        let sync = codescope_core::temporal::TemporalGraphSync::new(self.db.clone());
        match sync.calculate_hotspots(&self.repo_name).await {
            Ok(hotspots) => {
                if hotspots.is_empty() {
                    return "No hotspots found. Make sure to sync git history first with sync_git_history.".into();
                }
                let min_score = params.min_score.unwrap_or(0);
                let filtered: Vec<_> = hotspots.iter()
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

    /// Get file churn — most frequently changed files in git history
    #[tool(description = "Get the most frequently changed files in git history. High-churn files may indicate instability or active development areas.")]
    async fn file_churn(&self, #[tool(aggr)] params: ChurnParams) -> String {
        let limit = params.limit.unwrap_or(20);
        let git_path = self.codebase_path.clone();

        match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.file_churn(limit)
        }).await {
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

    /// Get change coupling — files that are frequently changed together
    #[tool(description = "Find files that are frequently changed together in commits. High coupling suggests hidden dependencies or that files should be colocated.")]
    async fn change_coupling(&self, #[tool(aggr)] params: CouplingParams) -> String {
        let limit = params.limit.unwrap_or(20);
        let git_path = self.codebase_path.clone();

        match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.change_coupling(limit)
        }).await {
            Ok(Ok(coupling)) => {
                let mut output = "## Change Coupling (Files Changed Together)\n\n".to_string();
                output.push_str("| Count | File A | File B |\n|-------|--------|--------|\n");
                for (a, b, count) in &coupling {
                    output.push_str(&format!("| {} | {} | {} |\n", count, a, b));
                }
                output
            }
            Ok(Err(e)) => format!("Error: {}", e),
            Err(e) => format!("Task error: {}", e),
        }
    }

    /// Review a git diff with graph context — analyze which functions and relationships are affected
    #[tool(description = "Review a git diff with graph context. Shows which functions, classes, and call relationships are affected by changes between two git refs.")]
    async fn review_diff(&self, #[tool(aggr)] params: DiffReviewParams) -> String {
        let git_path = self.codebase_path.clone();
        let base_ref = params.base_ref.clone();
        let head_ref_str = params.head_ref.clone().unwrap_or_else(|| "HEAD".to_string());

        // Get changed files in blocking task
        let changed_files = match tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<(String, String)>> {
            let repo = git2::Repository::open(&git_path)?;
            let base = repo.revparse_single(&base_ref)?;
            let head = repo.revparse_single(&head_ref_str)?;
            let base_tree = base.peel_to_tree()?;
            let head_tree = head.peel_to_tree()?;
            let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;

            let mut files = Vec::new();
            diff.foreach(
                &mut |delta, _| {
                    let path = delta.new_file().path()
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
                None, None, None,
            )?;
            Ok(files)
        }).await {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => return format!("Error computing diff: {}", e),
            Err(e) => return format!("Task error: {}", e),
        };

        let gq = self.gq();
        let head_display = params.head_ref.as_deref().unwrap_or("HEAD");

        let mut output = format!(
            "## Diff Review: {} → {}\n\n**{} files changed**\n\n",
            params.base_ref, head_display, changed_files.len()
        );

        for (file_path, status) in &changed_files {
            output.push_str(&format!("### {} ({})\n", file_path, status));
            match gq.file_entities(file_path).await {
                Ok(entities) if !entities.is_empty() => {
                    for e in &entities {
                        output.push_str(&format!(
                            "  - **{}** (L{}-{})\n",
                            e.name.as_deref().unwrap_or("?"),
                            e.start_line.unwrap_or(0),
                            e.end_line.unwrap_or(0),
                        ));
                    }
                }
                _ => { output.push_str("  (no indexed entities)\n"); }
            }
        }

        output.push_str(&format!("\n---\n**Summary:** {} files affected.\n", changed_files.len()));
        output
    }

    /// Get contributor expertise map — who knows which parts of the codebase
    #[tool(description = "Get a contributor expertise map showing who has the most knowledge about which files. Useful for finding the right reviewer for a change.")]
    async fn contributor_map(&self) -> String {
        let git_path = self.codebase_path.clone();

        match tokio::task::spawn_blocking(move || {
            let analyzer = codescope_core::temporal::GitAnalyzer::open(&git_path)?;
            analyzer.contributor_map()
        }).await {
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
    #[tool(description = "Suggest code reviewers for a set of changed files based on who has the most expertise with those files.")]
    async fn suggest_reviewers(&self, #[tool(aggr)] params: DiffReviewParams) -> String {
        let git_path = self.codebase_path.clone();
        let base_ref = params.base_ref.clone();
        let head_ref_str = params.head_ref.clone().unwrap_or_else(|| "HEAD".to_string());

        // All git2 work in blocking task
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

            let analyzer = codescope_core::temporal::GitAnalyzer::open(&repo.path().parent().unwrap_or(repo.path()))?;
            let contributor_map = analyzer.contributor_map()?;

            Ok((changed_files, contributor_map))
        }).await;

        let (changed_files, contributor_map) = match result {
            Ok(Ok((cf, cm))) => (cf, cm),
            Ok(Err(e)) => return format!("Error: {}", e),
            Err(e) => return format!("Task error: {}", e),
        };

        let mut reviewer_scores: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (author, files) in &contributor_map {
            for (file, count) in files {
                if changed_files.iter().any(|cf| file.contains(cf) || cf.contains(file)) {
                    *reviewer_scores.entry(author.clone()).or_insert(0) += count;
                }
            }
        }

        let mut reviewers: Vec<_> = reviewer_scores.into_iter().collect();
        reviewers.sort_by(|a, b| b.1.cmp(&a.1));

        let head_display = params.head_ref.as_deref().unwrap_or("HEAD");
        let mut output = format!(
            "## Suggested Reviewers for {} → {}\n\n**{} files changed**\n\n",
            params.base_ref, head_display, changed_files.len()
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

    /// Translate a natural language question to a SurrealQL query and execute it
    #[tool(description = "Ask a question about the codebase in natural language. Translates to a graph query and returns results. Examples: 'what functions are in main.rs?', 'find all structs', 'show call graph for parse_file'")]
    async fn ask(&self, #[tool(aggr)] params: NaturalLanguageQueryParams) -> String {
        let question = params.question.to_lowercase();
        let gq = self.gq();

        // Pattern matching for common questions
        let surql = if question.contains("how many") && question.contains("file") {
            "SELECT count() FROM file GROUP ALL".to_string()
        } else if question.contains("how many") && question.contains("function") {
            "SELECT count() FROM `function` GROUP ALL".to_string()
        } else if question.contains("how many") && question.contains("class") {
            "SELECT count() FROM class GROUP ALL".to_string()
        } else if question.contains("all function") || question.contains("list function") {
            "SELECT name, file_path, start_line FROM `function` ORDER BY name LIMIT 50".to_string()
        } else if question.contains("all class") || question.contains("list class") || question.contains("all struct") || question.contains("list struct") {
            "SELECT name, kind, file_path, start_line FROM class ORDER BY name LIMIT 50".to_string()
        } else if question.contains("all file") || question.contains("list file") {
            "SELECT path, language, line_count FROM file ORDER BY path LIMIT 50".to_string()
        } else if question.contains("call graph") || question.contains("calls") {
            // Try to extract a function name
            let words: Vec<&str> = question.split_whitespace().collect();
            if let Some(idx) = words.iter().position(|w| *w == "for" || *w == "of") {
                let func_name = words[idx + 1..].join(" ").trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_string();
                format!(
                    "SELECT ->calls->`function`.name AS calls FROM `function` WHERE name = '{}'",
                    func_name
                )
            } else {
                "SELECT *, ->calls->`function`.name AS calls FROM `function` WHERE array::len(->calls) > 0 LIMIT 20".to_string()
            }
        } else if question.contains("in file") || question.contains("in ") && question.contains(".rs") || question.contains(".ts") || question.contains(".py") {
            // Extract file path from question
            let path = extract_path_from_question(&question);
            format!(
                "SELECT name, qualified_name, start_line, end_line FROM `function` WHERE file_path CONTAINS '{}' \
                 UNION \
                 SELECT name, qualified_name, start_line, end_line FROM class WHERE file_path CONTAINS '{}'",
                path, path
            )
        } else if question.contains("largest") || question.contains("biggest") || question.contains("longest") {
            "SELECT name, file_path, start_line, end_line, (end_line - start_line) AS size FROM `function` ORDER BY size DESC LIMIT 10".to_string()
        } else if question.contains("import") {
            "SELECT name, file_path FROM import_decl ORDER BY file_path LIMIT 50".to_string()
        } else {
            // Fallback: try text search on function names
            let search_term = question.split_whitespace()
                .filter(|w| w.len() > 3 && !["what", "where", "which", "find", "show", "list", "does", "that", "this", "from", "with"].contains(w))
                .next()
                .unwrap_or(&question);
            format!(
                "SELECT name, file_path, start_line, signature FROM `function` WHERE name ~ '{}' LIMIT 20",
                search_term
            )
        };

        match gq.raw_query(&surql).await {
            Ok(result) => {
                let mut output = format!("**Query:** `{}`\n\n**Results:**\n", surql);
                output.push_str(&serde_json::to_string_pretty(&result).unwrap_or_default());
                output
            }
            Err(e) => format!("Error executing query: {}\n\nQuery was: {}", e, surql),
        }
    }
}

fn extract_path_from_question(question: &str) -> String {
    // Find anything that looks like a file path
    for word in question.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '\\' && c != '.' && c != '_' && c != '-');
        if clean.contains('.') && (clean.contains('/') || clean.contains('\\') || clean.ends_with(".rs") || clean.ends_with(".ts") || clean.ends_with(".py") || clean.ends_with(".go") || clean.ends_with(".java") || clean.ends_with(".js")) {
            return clean.to_string();
        }
    }
    question.to_string()
}

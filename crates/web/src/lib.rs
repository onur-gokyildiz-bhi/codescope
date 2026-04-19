use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use codescope_core::daemon::DaemonState;
use codescope_core::graph::query::GraphQuery;

mod error;
pub use error::{code as error_code, ApiError};

enum ProjectSource {
    /// Backing DB for the CLI-provided path (primary repo). We also cache
    /// lazily-opened DBs for other repos discovered under `~/.codescope/db/`
    /// so `/api/projects` isn't a dead end in single mode.
    Single {
        primary_repo: String,
        primary_db: codescope_core::DbHandle,
        extra_dbs: tokio::sync::Mutex<std::collections::HashMap<String, codescope_core::DbHandle>>,
    },
    Multi(Arc<DaemonState>),
}

pub struct AppState {
    source: ProjectSource,
}

impl AppState {
    async fn resolve_query(&self, repo: Option<&str>) -> Result<GraphQuery, ApiError> {
        match &self.source {
            ProjectSource::Single {
                primary_repo,
                primary_db,
                extra_dbs,
            } => {
                let requested = repo.filter(|r| !r.is_empty());
                // Fast path: no repo param or same as primary.
                if requested.is_none() || requested == Some(primary_repo.as_str()) {
                    return Ok(GraphQuery::new(primary_db.clone()));
                }
                let name = requested.unwrap().to_string();
                {
                    let cache = extra_dbs.lock().await;
                    if let Some(db) = cache.get(&name) {
                        return Ok(GraphQuery::new(db.clone()));
                    }
                }
                // Lazy-open via the shared surreal server — the per-repo
                // filesystem path is a migration fossil; the server
                // auto-creates the DB under NS=codescope on first
                // `use_db`. We just need to verify the repo actually
                // has data by checking the legacy dir (until the server
                // reports empty DBs through INFO FOR NS).
                let db = codescope_core::connect_repo(&name)
                    .await
                    .map_err(|e| ApiError::from_db_err(&name, e))?;
                let _ = codescope_core::graph::schema::init_schema(&db).await;
                let cloned = db.clone();
                extra_dbs.lock().await.insert(name, cloned);
                Ok(GraphQuery::new(db))
            }
            ProjectSource::Multi(daemon) => {
                let repo_name = match repo {
                    Some(r) if !r.is_empty() => r.to_string(),
                    _ => daemon.discover_repos().into_iter().next().ok_or_else(|| {
                        ApiError::new(
                            StatusCode::NOT_FOUND,
                            error_code::REPO_NOT_FOUND,
                            "No projects found. Index a codebase first.",
                        )
                        .with_hint("Run `codescope index <path> --repo <name>`")
                    })?,
                };
                let db = daemon
                    .get_db(&repo_name)
                    .await
                    .map_err(|e| ApiError::from_db_err(&repo_name, e))?;
                Ok(GraphQuery::new(db))
            }
        }
    }
}

/// Enumerate every project with a persisted DB under `~/.codescope/db/`.
/// Used by `api_projects` in single mode so the frontend's project switcher
/// works even when the user started `codescope web` with a single path.
fn discover_local_projects() -> Vec<String> {
    let db_root = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".codescope")
        .join("db");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&db_root) {
        for e in entries.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(name) = e.file_name().to_str() {
                    // Hidden helpers like `.tmp` or the archive sentinel are skipped.
                    if !name.starts_with('.') {
                        out.push(name.to_string());
                    }
                }
            }
        }
    }
    out.sort();
    out
}

#[derive(Deserialize)]
struct RepoParam {
    repo: Option<String>,
}

fn build_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(index_page))
        .route("/assets/{*path}", get(serve_asset))
        .route("/api/projects", get(api_projects))
        .route("/api/stats", get(api_stats))
        .route("/api/search", get(api_search))
        .route("/api/graph", get(api_graph))
        .route("/api/query", get(api_raw_query))
        .route("/api/conversations", get(api_conversations))
        .route("/api/files", get(api_files))
        .route("/api/file-content", get(api_file_content))
        .route("/api/node-detail", get(api_node_detail))
        .route("/api/knowledge-detail", get(api_knowledge_detail))
        .route("/api/hotspots", get(api_hotspots))
        .route("/api/clusters", get(api_clusters))
        .route("/api/skill-graph", get(api_skill_graph))
}

/// Build the web API router from an existing DB connection (single-project mode).
pub fn build_web_router(db: codescope_core::DbHandle) -> Router {
    let state = Arc::new(AppState {
        source: ProjectSource::Single {
            primary_repo: String::new(),
            primary_db: db,
            extra_dbs: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        },
    });
    build_routes().with_state(state)
}

/// Build the web API router for daemon mode (multi-project).
pub fn build_multi_web_router(daemon: Arc<DaemonState>) -> Router {
    let state = Arc::new(AppState {
        source: ProjectSource::Multi(daemon),
    });
    build_routes().with_state(state)
}

/// Run the web visualization server.
/// This is the main entry point used by both the standalone binary and the unified CLI.
pub async fn run_web(
    path: PathBuf,
    repo: Option<String>,
    port: u16,
    auto_index: bool,
    db_path_override: Option<PathBuf>,
    host: String,
) -> Result<()> {
    // Resolve repo name: --repo > directory name > "default"
    let repo_name = repo.unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("default")
            .to_string()
    });
    let db_path = db_path_override.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codescope")
            .join("db")
            .join(&repo_name)
    });

    // Connect via the shared surreal server.
    let db = codescope_core::connect_repo(&repo_name).await?;
    codescope_core::graph::schema::init_schema(&db).await?;
    codescope_core::graph::migrations::migrate_to_current(&db).await?;
    println!(
        "DB: ns=codescope db={} (via {})",
        repo_name,
        std::env::var("CODESCOPE_DB_URL")
            .unwrap_or_else(|_| codescope_core::db::DEFAULT_URL.to_string())
    );

    // Auto-index if requested
    if auto_index {
        let codebase_path = std::fs::canonicalize(&path).unwrap_or(path.clone());
        println!("Indexing {}...", codebase_path.display());

        let parser = codescope_core::parser::CodeParser::new();
        let builder = codescope_core::graph::builder::GraphBuilder::new(db.clone());

        let parse_path = codebase_path.clone();
        let parse_repo = repo_name.clone();
        let results = tokio::task::spawn_blocking(move || {
            use rayon::prelude::*;
            let walker = ignore::WalkBuilder::new(&parse_path)
                .hidden(true)
                .git_ignore(true)
                .build();

            let files: Vec<std::path::PathBuf> = walker
                .flatten()
                .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
                .filter(|e| {
                    let fp = e.path();
                    let ext = fp.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let fname = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    (parser.supports_extension(ext) || parser.supports_filename(fname))
                        && !codescope_core::parser::should_skip_file(fp)
                })
                .map(|e| e.into_path())
                .collect();

            println!("Found {} files to parse", files.len());

            files
                .par_iter()
                .filter_map(|file_path| {
                    let rel_path = file_path
                        .strip_prefix(&parse_path)
                        .unwrap_or(file_path)
                        .to_string_lossy()
                        .to_string()
                        .replace('\\', "/");
                    let content = std::fs::read_to_string(file_path).ok()?;
                    parser
                        .parse_source(std::path::Path::new(&rel_path), &content, &parse_repo)
                        .ok()
                })
                .collect::<Vec<_>>()
        })
        .await?;

        let mut file_count = 0;
        for (entities, relations) in results {
            if let Err(e) = builder.insert_entities(&entities).await {
                tracing::warn!("Entity insert failed: {e}");
            }
            if let Err(e) = builder.insert_relations(&relations).await {
                tracing::warn!("Relation insert failed: {e}");
            }
            file_count += 1;
        }
        println!("Indexed {} files", file_count);
    }

    let state = Arc::new(AppState {
        source: ProjectSource::Single {
            primary_repo: repo_name.clone(),
            primary_db: db,
            extra_dbs: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        },
    });

    let app = build_routes().with_state(state);

    let addr = format!("{}:{}", host, port);
    println!("Codescope Web UI: http://{}:{}", host, port);
    if host == "0.0.0.0" {
        // Show LAN IPs so other machines can connect
        if let Ok(hostname) = std::process::Command::new("hostname").output() {
            let name = String::from_utf8_lossy(&hostname.stdout).trim().to_string();
            if !name.is_empty() {
                println!("  LAN access: http://{}:{}", name, port);
            }
        }
        println!("  (localhost-only: use --host 127.0.0.1)");
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("../frontend/dist/index.html"))
}

async fn serve_asset(axum::extract::Path(path): axum::extract::Path<String>) -> impl IntoResponse {
    // Serve JS/CSS from embedded frontend build
    let content_type = if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else {
        "application/octet-stream"
    };

    // Try to find the file in the build output
    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("frontend")
        .join("dist")
        .join("assets");
    let file_path = assets_dir.join(&path);

    match std::fs::read(&file_path) {
        Ok(bytes) => ([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response(),
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
    repo: Option<String>,
}

async fn api_projects(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.source {
        ProjectSource::Multi(daemon) => {
            let discovered = daemon.discover_repos();
            let active = daemon.active_repos().await;
            Json(serde_json::json!({ "projects": discovered, "active": active })).into_response()
        }
        ProjectSource::Single { primary_repo, .. } => {
            // Single mode can still browse every other indexed repo under
            // `~/.codescope/db/`. Secondary DBs are opened lazily on first
            // query (see resolve_query's extra_dbs cache).
            let mut discovered = discover_local_projects();
            // Ensure the primary repo is present even if the user's custom
            // --db-path points outside `~/.codescope/db/`.
            if !primary_repo.is_empty() && !discovered.iter().any(|p| p == primary_repo) {
                discovered.insert(0, primary_repo.clone());
            }
            let active = if primary_repo.is_empty() {
                Vec::<String>::new()
            } else {
                vec![primary_repo.clone()]
            };
            Json(serde_json::json!({ "projects": discovered, "active": active })).into_response()
        }
    }
}

async fn api_stats(
    State(state): State<Arc<AppState>>,
    Query(rp): Query<RepoParam>,
) -> impl IntoResponse {
    let query = match state.resolve_query(rp.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    match query.raw_query("SELECT count() AS total FROM file GROUP ALL; SELECT count() AS total FROM `function` GROUP ALL; SELECT count() AS total FROM class GROUP ALL; SELECT count() AS total FROM import_decl GROUP ALL; SELECT count() AS total FROM config GROUP ALL; SELECT count() AS total FROM doc GROUP ALL; SELECT count() AS total FROM package GROUP ALL; SELECT count() AS total FROM knowledge GROUP ALL; SELECT count() AS total FROM decision GROUP ALL").await {
        Ok(result) => {
            // Flatten [[{"total":N}],...] -> {"files":N,"functions":N,...}
            let labels = ["files", "functions", "classes", "imports", "configs", "docs", "packages", "knowledge", "decisions"];
            let arr = result.as_array();
            let mut stats = serde_json::Map::new();
            if let Some(items) = arr {
                for (i, label) in labels.iter().enumerate() {
                    let count = items.get(i)
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|o| o.get("total"))
                        .and_then(|n| n.as_u64())
                        .unwrap_or(0);
                    stats.insert(label.to_string(), serde_json::Value::Number(count.into()));
                }
            }
            Json(serde_json::Value::Object(stats)).into_response()
        }
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

async fn api_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let pattern = params.q.unwrap_or_default();
    if pattern.is_empty() {
        return Json(serde_json::json!([])).into_response();
    }
    let gq = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let mut results: Vec<serde_json::Value> = Vec::new();

    if let Ok(fn_results) = gq.search_functions(&pattern).await {
        for sr in fn_results {
            if let Ok(val) = serde_json::to_value(&sr) {
                results.push(val);
            }
        }
    }

    let safe = pattern.replace('\'', "");
    let know_q = format!(
        "SELECT id, title AS name, kind, confidence, tags FROM knowledge \
         WHERE title CONTAINS '{}' OR content CONTAINS '{}' LIMIT 20",
        safe, safe
    );
    if let Ok(know_results) = gq.raw_query(&know_q).await {
        for row in flatten_result(&know_results) {
            let mut obj = serde_json::Map::new();
            let title = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let kind = row
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("concept");
            obj.insert("name".into(), serde_json::json!(title));
            obj.insert(
                "id".into(),
                row.get("id").cloned().unwrap_or(serde_json::json!(title)),
            );
            obj.insert(
                "kind".into(),
                serde_json::json!(format!("knowledge:{}", kind)),
            );
            if let Some(conf) = row.get("confidence") {
                obj.insert("confidence".into(), conf.clone());
            }
            if let Some(tags) = row.get("tags") {
                obj.insert("tags".into(), tags.clone());
            }
            results.push(serde_json::Value::Object(obj));
        }
    }

    Json(serde_json::json!(results)).into_response()
}

#[derive(Deserialize)]
struct GraphParams {
    center: Option<String>,
    depth: Option<u32>,
    repo: Option<String>,
    /// Clustering mode: "none" (default), "folder" (group by top-2 path segments), "auto" (cluster when nodes > max_nodes)
    cluster_mode: Option<String>,
    /// Auto-cluster threshold (default 500)
    max_nodes: Option<usize>,
}

/// Apply folder-based clustering to a graph. Replaces nodes within the same
/// top-level folder with a single super-node when that folder has >10 members.
/// Aggregates cross-folder edges into edges between cluster nodes.
fn apply_folder_clustering(data: GraphData) -> GraphData {
    use std::collections::HashMap;

    // Derive cluster id for a node: top 2 path segments (e.g. "crates/core")
    fn cluster_id(node: &GraphNode) -> Option<String> {
        if node.file_path.is_empty() || node.kind == "file" {
            return None;
        }
        let parts: Vec<&str> = node.file_path.split('/').take(2).collect();
        if parts.len() < 2 {
            return None;
        }
        Some(format!("{}/{}", parts[0], parts[1]))
    }

    // Count members per cluster
    let mut cluster_counts: HashMap<String, usize> = HashMap::new();
    for n in &data.nodes {
        if let Some(c) = cluster_id(n) {
            *cluster_counts.entry(c).or_insert(0) += 1;
        }
    }

    // Threshold: cluster only if folder has > 10 members
    const MIN_CLUSTER_SIZE: usize = 10;
    let active_clusters: std::collections::HashSet<String> = cluster_counts
        .iter()
        .filter(|(_, &n)| n > MIN_CLUSTER_SIZE)
        .map(|(k, _)| k.clone())
        .collect();

    if active_clusters.is_empty() {
        return data;
    }

    // Build node map: original id -> cluster id (or itself if not clustered)
    let mut node_map: HashMap<String, String> = HashMap::new();
    let mut clustered_nodes: Vec<GraphNode> = Vec::new();
    let mut seen_clusters: std::collections::HashSet<String> = std::collections::HashSet::new();

    for n in &data.nodes {
        if let Some(c) = cluster_id(n) {
            if active_clusters.contains(&c) {
                let cid = format!("cluster:{}", c);
                node_map.insert(n.id.clone(), cid.clone());
                if seen_clusters.insert(cid.clone()) {
                    let count = cluster_counts.get(&c).copied().unwrap_or(0);
                    clustered_nodes.push(GraphNode {
                        id: cid,
                        name: format!("{} ({})", c, count),
                        kind: "cluster".to_string(),
                        file_path: c.clone(),
                        confidence: None,
                        tags: None,
                        content: None,
                        source_url: None,
                    });
                }
                continue;
            }
        }
        node_map.insert(n.id.clone(), n.id.clone());
        clustered_nodes.push(GraphNode {
            id: n.id.clone(),
            name: n.name.clone(),
            kind: n.kind.clone(),
            file_path: n.file_path.clone(),
            confidence: n.confidence.clone(),
            tags: n.tags.clone(),
            content: n.content.clone(),
            source_url: n.source_url.clone(),
        });
    }

    // Aggregate edges
    let mut edge_counts: HashMap<(String, String, String), u32> = HashMap::new();
    for e in &data.links {
        let src = node_map.get(&e.source).cloned().unwrap_or(e.source.clone());
        let tgt = node_map.get(&e.target).cloned().unwrap_or(e.target.clone());
        if src == tgt {
            continue;
        }
        *edge_counts.entry((src, tgt, e.kind.clone())).or_insert(0) += 1;
    }

    let clustered_edges: Vec<GraphEdge> = edge_counts
        .into_iter()
        .map(|((source, target, kind), _)| GraphEdge {
            source,
            target,
            kind,
        })
        .collect();

    GraphData {
        nodes: clustered_nodes,
        links: clustered_edges,
    }
}

#[derive(Serialize)]
struct GraphNode {
    id: String,
    name: String,
    kind: String,
    file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_url: Option<String>,
}

#[derive(Serialize)]
struct GraphEdge {
    source: String,
    target: String,
    kind: String,
}

#[derive(Serialize)]
struct GraphData {
    nodes: Vec<GraphNode>,
    links: Vec<GraphEdge>,
}

async fn api_graph(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GraphParams>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let center = params.center.unwrap_or_default();
    let _depth = params.depth.unwrap_or(2);

    if center.is_empty() {
        // Return overview: top 50 functions with their call relationships
        let query = "SELECT name, qualified_name, file_path FROM `function` LIMIT 50";
        match gq.raw_query(query).await {
            Ok(result) => {
                let mut nodes = Vec::new();
                let mut edges = Vec::new();

                for row in flatten_result(&result) {
                    let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let qname = row
                        .get("qualified_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(name);
                    let fp = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                    nodes.push(GraphNode {
                        id: qname.to_string(),
                        name: name.to_string(),
                        kind: "function".to_string(),
                        file_path: fp.to_string(),
                        confidence: None,
                        tags: None,
                        content: None,
                        source_url: None,
                    });
                }

                // Get call edges
                let edge_query = "SELECT in.qualified_name AS source, out.qualified_name AS target FROM calls WHERE out.qualified_name != NONE LIMIT 200";
                if let Ok(edge_result) = gq.raw_query(edge_query).await {
                    for row in flatten_result(&edge_result) {
                        let src = row.get("source").and_then(|v| v.as_str()).unwrap_or("");
                        let tgt = row.get("target").and_then(|v| v.as_str()).unwrap_or("");
                        if !src.is_empty() && !tgt.is_empty() {
                            edges.push(GraphEdge {
                                source: src.to_string(),
                                target: tgt.to_string(),
                                kind: "calls".to_string(),
                            });
                        }
                    }
                }

                // Get contains edges (file → function) — group functions by file
                {
                    let node_ids: std::collections::HashSet<String> =
                        nodes.iter().map(|n| n.id.clone()).collect();
                    let mut file_fns: std::collections::HashMap<String, Vec<String>> =
                        std::collections::HashMap::new();
                    for n in &nodes {
                        if n.kind == "function" && !n.file_path.is_empty() {
                            file_fns
                                .entry(n.file_path.clone())
                                .or_default()
                                .push(n.id.clone());
                        }
                    }
                    for (fp, fns) in &file_fns {
                        let file_id = format!("file:{}", fp);
                        if !node_ids.contains(&file_id) {
                            let short = fp.rsplit('/').next().unwrap_or(fp);
                            nodes.push(GraphNode {
                                id: file_id.clone(),
                                name: short.to_string(),
                                kind: "file".to_string(),
                                file_path: fp.clone(),
                                confidence: None,
                                tags: None,
                                content: None,
                                source_url: None,
                            });
                        }
                        for fn_id in fns {
                            edges.push(GraphEdge {
                                source: file_id.clone(),
                                target: fn_id.clone(),
                                kind: "contains".to_string(),
                            });
                        }
                    }
                }

                // Knowledge nodes + edges
                let know_q = "SELECT id, title, kind, confidence, tags, content, source_url FROM knowledge LIMIT 50";
                if let Ok(know_result) = gq.raw_query(know_q).await {
                    for row in flatten_result(&know_result) {
                        let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let title = row.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                        let kind = row
                            .get("kind")
                            .and_then(|v| v.as_str())
                            .unwrap_or("concept");
                        let conf = row
                            .get("confidence")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let tags_val = row.get("tags").and_then(|v| v.as_array()).map(|a| {
                            a.iter()
                                .filter_map(|t| t.as_str().map(|s| s.to_string()))
                                .collect::<Vec<_>>()
                        });
                        let content_val = row
                            .get("content")
                            .and_then(|v| v.as_str())
                            .map(|s| truncate_chars(s, 200));
                        let src_url = row
                            .get("source_url")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string());
                        nodes.push(GraphNode {
                            id: id.to_string(),
                            name: title.to_string(),
                            kind: format!("knowledge:{}", kind),
                            file_path: String::new(),
                            confidence: conf,
                            tags: tags_val,
                            content: content_val,
                            source_url: src_url,
                        });
                    }
                }

                // Knowledge relation edges
                let know_edge_q = "SELECT in AS source, out AS target, 'supports' AS kind FROM supports; \
                                   SELECT in AS source, out AS target, 'contradicts' AS kind FROM contradicts; \
                                   SELECT in AS source, out AS target, 'related_to' AS kind FROM related_to";
                if let Ok(ke_result) = gq.raw_query(know_edge_q).await {
                    if let Some(arr) = ke_result.as_array() {
                        for batch in arr {
                            for row in batch.as_array().unwrap_or(&vec![]) {
                                let src = row.get("source").and_then(|v| v.as_str()).unwrap_or("");
                                let tgt = row.get("target").and_then(|v| v.as_str()).unwrap_or("");
                                let kind = row
                                    .get("kind")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("related_to");
                                if !src.is_empty() && !tgt.is_empty() {
                                    edges.push(GraphEdge {
                                        source: src.to_string(),
                                        target: tgt.to_string(),
                                        kind: kind.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }

                // Decision / Problem / Solution / ConvTopic / Conversation nodes
                // These live in _global and have no code counterpart — include them so
                // selecting _global project shows a populated graph.
                let conv_entity_q = "\
                    SELECT id, name AS title, 'decision' AS kind, tags, body AS content FROM decision LIMIT 80; \
                    SELECT id, name AS title, 'problem' AS kind, tags, body AS content FROM problem LIMIT 80; \
                    SELECT id, name AS title, 'solution' AS kind, tags, body AS content FROM solution LIMIT 80; \
                    SELECT id, name AS title, 'topic' AS kind, tags, '' AS content FROM conv_topic LIMIT 40; \
                    SELECT id, name AS title, 'conversation' AS kind, NONE AS tags, body AS content FROM conversation LIMIT 40";
                if let Ok(ce_result) = gq.raw_query(conv_entity_q).await {
                    if let Some(arr) = ce_result.as_array() {
                        for batch in arr {
                            for row in batch.as_array().unwrap_or(&vec![]) {
                                let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                let title =
                                    row.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                                let kind = row
                                    .get("kind")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("decision");
                                if id.is_empty() {
                                    continue;
                                }
                                let tags_val =
                                    row.get("tags").and_then(|v| v.as_array()).map(|a| {
                                        a.iter()
                                            .filter_map(|t| t.as_str().map(|s| s.to_string()))
                                            .collect::<Vec<_>>()
                                    });
                                let content_val = row
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .map(|s| truncate_chars(s, 200));
                                nodes.push(GraphNode {
                                    id: id.to_string(),
                                    name: title.to_string(),
                                    kind: kind.to_string(),
                                    file_path: String::new(),
                                    confidence: None,
                                    tags: tags_val,
                                    content: content_val,
                                    source_url: None,
                                });
                            }
                        }
                    }
                }

                // Conversation relation edges
                let conv_edge_q = "\
                    SELECT in AS source, out AS target, 'discussed_in' AS kind FROM discussed_in LIMIT 200; \
                    SELECT in AS source, out AS target, 'decided_about' AS kind FROM decided_about LIMIT 200; \
                    SELECT in AS source, out AS target, 'solves_for' AS kind FROM solves_for LIMIT 200; \
                    SELECT in AS source, out AS target, 'co_discusses' AS kind FROM co_discusses LIMIT 200; \
                    SELECT in AS source, out AS target, 'links_to' AS kind FROM links_to LIMIT 200";
                if let Ok(ce_result) = gq.raw_query(conv_edge_q).await {
                    if let Some(arr) = ce_result.as_array() {
                        for batch in arr {
                            for row in batch.as_array().unwrap_or(&vec![]) {
                                let src = row.get("source").and_then(|v| v.as_str()).unwrap_or("");
                                let tgt = row.get("target").and_then(|v| v.as_str()).unwrap_or("");
                                let kind = row
                                    .get("kind")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("links_to");
                                if !src.is_empty() && !tgt.is_empty() {
                                    edges.push(GraphEdge {
                                        source: src.to_string(),
                                        target: tgt.to_string(),
                                        kind: kind.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }

                // 3d-force-graph chokes on orphan edges (source/target not
                // present in nodes list). Happens when LIMIT on functions
                // truncates the node set while call/contains edges still
                // reference the full graph. Drop any edge whose endpoints
                // aren't in the emitted node set.
                let node_ids: std::collections::HashSet<String> =
                    nodes.iter().map(|n| n.id.clone()).collect();
                edges.retain(|e| node_ids.contains(&e.source) && node_ids.contains(&e.target));

                let mut data = GraphData {
                    nodes,
                    links: edges,
                };
                let max_nodes = params.max_nodes.unwrap_or(500);
                let mode = params.cluster_mode.as_deref().unwrap_or("none");
                let should_cluster =
                    mode == "folder" || (mode == "auto" && data.nodes.len() > max_nodes);
                if should_cluster {
                    data = apply_folder_clustering(data);
                }
                Json(data).into_response()
            }
            Err(e) => ApiError::internal(e.to_string()).into_response(),
        }
    } else {
        // Centered graph: find the entity + all its neighbors
        let safe = center.replace('\'', "");
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Helper to add a node if not already seen
        let add_node = |nodes: &mut Vec<GraphNode>,
                        seen: &mut std::collections::HashSet<String>,
                        id: &str,
                        name: &str,
                        kind: &str,
                        fp: &str| {
            if seen.insert(id.to_string()) {
                nodes.push(GraphNode {
                    id: id.to_string(),
                    name: name.to_string(),
                    kind: kind.to_string(),
                    file_path: fp.to_string(),
                    confidence: None,
                    tags: None,
                    content: None,
                    source_url: None,
                });
            }
        };

        // 1. Find center entity details (try function first, then class)
        let mut center_id = safe.clone();
        let mut center_fp = String::new();
        let mut center_kind = "function".to_string();

        let fn_q = format!(
            "SELECT name, qualified_name, file_path FROM `function` WHERE name = '{}' LIMIT 1",
            safe
        );
        if let Ok(result) = gq.raw_query(&fn_q).await {
            if let Some(row) = flatten_result(&result).first() {
                center_id = row
                    .get("qualified_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&safe)
                    .to_string();
                center_fp = row
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
            }
        }
        if center_fp.is_empty() {
            let cls_q = format!(
                "SELECT name, qualified_name, file_path, kind FROM class WHERE name = '{}' LIMIT 1",
                safe
            );
            if let Ok(result) = gq.raw_query(&cls_q).await {
                if let Some(row) = flatten_result(&result).first() {
                    center_id = row
                        .get("qualified_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&safe)
                        .to_string();
                    center_fp = row
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    center_kind = "class".to_string();
                }
            }
        }
        add_node(
            &mut nodes,
            &mut seen,
            &center_id,
            &safe,
            &center_kind,
            &center_fp,
        );

        // 2. Callers (who calls this function)
        let caller_q = format!(
            "SELECT in.name AS name, in.qualified_name AS qualified_name, in.file_path AS file_path \
             FROM calls WHERE out.name = '{}' AND in.name != NONE",
            safe
        );
        if let Ok(result) = gq.raw_query(&caller_q).await {
            let wrapped = serde_json::json!([{"callers": result}]);
            extract_neighbors(
                &wrapped, "callers", &mut nodes, &mut edges, &mut seen, &center_id, true,
            );
        }

        // 3. Callees (what this function calls)
        let callee_q = format!(
            "SELECT out.name AS name, out.qualified_name AS qualified_name, out.file_path AS file_path \
             FROM calls WHERE in.name = '{}' AND out.name != NONE",
            safe
        );
        if let Ok(result) = gq.raw_query(&callee_q).await {
            let wrapped = serde_json::json!([{"callees": result}]);
            extract_neighbors(
                &wrapped, "callees", &mut nodes, &mut edges, &mut seen, &center_id, false,
            );
        }

        // 4. Sibling functions in same file
        if !center_fp.is_empty() {
            let sibling_q = format!(
                "SELECT name, qualified_name, file_path FROM `function` WHERE file_path = '{}' AND name != '{}' LIMIT 15",
                center_fp.replace('\'', ""), safe
            );
            if let Ok(result) = gq.raw_query(&sibling_q).await {
                for row in flatten_result(&result) {
                    let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let qn = row
                        .get("qualified_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(name);
                    let fp = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                    add_node(&mut nodes, &mut seen, qn, name, "sibling", fp);
                    edges.push(GraphEdge {
                        source: center_id.clone(),
                        target: qn.to_string(),
                        kind: "same_file".to_string(),
                    });
                }
            }

            // 5. File node
            let file_id = format!("file:{}", center_fp);
            add_node(
                &mut nodes, &mut seen, &file_id, &center_fp, "file", &center_fp,
            );
            edges.push(GraphEdge {
                source: file_id,
                target: center_id.clone(),
                kind: "contains".to_string(),
            });
        }

        Json(GraphData {
            nodes,
            links: edges,
        })
        .into_response()
    }
}

/// Truncate a string to at most `max_chars` Unicode scalar values, appending
/// an ellipsis when truncated. Byte-slicing multi-byte codepoints (e.g. Turkish
/// 'ı', 'ş') caused panics in /api/graph on conversation bodies — this is the
/// safe replacement.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut count = 0usize;
    let mut end = s.len();
    for (i, _) in s.char_indices() {
        if count == max_chars {
            end = i;
            break;
        }
        count += 1;
    }
    if end < s.len() {
        format!("{}…", &s[..end])
    } else {
        s.to_string()
    }
}

/// Flatten a SurrealDB raw_query result into a Vec of objects.
fn flatten_result(result: &serde_json::Value) -> Vec<&serde_json::Value> {
    let mut out = Vec::new();
    if let Some(arr) = result.as_array() {
        for item in arr {
            if let Some(inner) = item.as_array() {
                out.extend(inner.iter());
            } else if item.is_object() {
                out.push(item);
            }
        }
    }
    out
}

/// Extract neighbor nodes from a SurrealDB graph traversal result.
fn extract_neighbors(
    result: &serde_json::Value,
    field: &str,
    nodes: &mut Vec<GraphNode>,
    edges: &mut Vec<GraphEdge>,
    seen: &mut std::collections::HashSet<String>,
    center_id: &str,
    is_caller: bool,
) {
    for row in flatten_result(result) {
        if let Some(neighbors) = row.get(field).and_then(|v| v.as_array()) {
            for n in neighbors {
                let name = n.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                let qn = n
                    .get("qualified_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(name);
                let fp = n.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                let kind = if is_caller { "caller" } else { "callee" };
                if seen.insert(qn.to_string()) {
                    nodes.push(GraphNode {
                        id: qn.to_string(),
                        name: name.to_string(),
                        kind: kind.to_string(),
                        file_path: fp.to_string(),
                        confidence: None,
                        tags: None,
                        content: None,
                        source_url: None,
                    });
                }
                let (src, tgt) = if is_caller {
                    (qn.to_string(), center_id.to_string())
                } else {
                    (center_id.to_string(), qn.to_string())
                };
                edges.push(GraphEdge {
                    source: src,
                    target: tgt,
                    kind: "calls".to_string(),
                });
            }
        }
    }
}

async fn api_conversations(
    State(state): State<Arc<AppState>>,
    Query(rp): Query<RepoParam>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(rp.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let query = "\
        SELECT id, name, body, kind, timestamp, file_path FROM decision ORDER BY timestamp DESC LIMIT 80; \
        SELECT id, name, body, kind, timestamp, file_path FROM problem ORDER BY timestamp DESC LIMIT 80; \
        SELECT id, name, body, kind, timestamp, file_path FROM solution ORDER BY timestamp DESC LIMIT 80; \
        SELECT id, name, body, kind, timestamp FROM conv_topic ORDER BY timestamp DESC LIMIT 50; \
        SELECT id, name, qualified_name, file_path, body FROM conversation ORDER BY name LIMIT 30; \
        SELECT id, title AS name, content AS body, kind, confidence, tags, source_url \
            FROM knowledge LIMIT 80;";

    match gq.raw_query(query).await {
        Ok(result) => {
            let items = result.as_array();
            let mut out = serde_json::Map::new();
            let keys = [
                "decisions",
                "problems",
                "solutions",
                "topics",
                "sessions",
                "knowledge",
            ];
            if let Some(arr) = items {
                for (i, key) in keys.iter().enumerate() {
                    let data = arr
                        .get(i)
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    out.insert(key.to_string(), serde_json::Value::Array(data));
                }
            }
            Json(serde_json::Value::Object(out)).into_response()
        }
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct RawQueryParams {
    q: Option<String>,
    repo: Option<String>,
}

async fn api_raw_query(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RawQueryParams>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let query = params.q.unwrap_or_default();
    if query.is_empty() {
        return ApiError::invalid_input("No query provided").into_response();
    }
    match gq.raw_query(&query).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

// File tree
async fn api_files(
    State(state): State<Arc<AppState>>,
    Query(rp): Query<RepoParam>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(rp.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    match gq
        .raw_query("SELECT path, language, line_count FROM file ORDER BY path")
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

// File content
#[derive(Deserialize)]
struct FileContentParams {
    path: Option<String>,
    repo: Option<String>,
}

async fn api_file_content(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileContentParams>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let path = params.path.unwrap_or_default();
    if path.is_empty() {
        return ApiError::invalid_input("No path").into_response();
    }
    let entities = gq.raw_query(&format!(
        "SELECT name, start_line, end_line, signature, 'function' AS type FROM `function` WHERE file_path = '{}' ORDER BY start_line; \
         SELECT name, start_line, end_line, kind, 'class' AS type FROM class WHERE file_path = '{}' ORDER BY start_line",
        path.replace('\'', "\\'"), path.replace('\'', "\\'")
    )).await.unwrap_or(serde_json::Value::Null);

    let content = std::fs::read_to_string(&path)
        .or_else(|_| {
            let home = dirs::home_dir().unwrap_or_default();
            std::fs::read_to_string(home.join(&path))
        })
        .unwrap_or_default();

    Json(serde_json::json!({
        "path": path,
        "content": content,
        "entities": entities,
    }))
    .into_response()
}

// Node detail
#[derive(Deserialize)]
struct NodeDetailParams {
    name: Option<String>,
    repo: Option<String>,
}

async fn api_node_detail(
    State(state): State<Arc<AppState>>,
    Query(params): Query<NodeDetailParams>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let name = params.name.unwrap_or_default();
    if name.is_empty() {
        return ApiError::invalid_input("No name").into_response();
    }
    let q = format!(
        "SELECT name, qualified_name, signature, file_path, start_line, end_line FROM `function` WHERE name = '{}'; \
         SELECT in.name AS name, in.file_path AS file_path, in.signature AS sig FROM calls WHERE out.name = '{}' AND in.name != NONE LIMIT 20; \
         SELECT out.name AS name, out.file_path AS file_path, out.signature AS sig FROM calls WHERE in.name = '{}' AND out.name != NONE LIMIT 20; \
         SELECT name, kind, file_path, start_line FROM class WHERE name = '{}'",
        name.replace('\'', "\\'"), name.replace('\'', "\\'"),
        name.replace('\'', "\\'"), name.replace('\'', "\\'"),
    );
    match gq.raw_query(&q).await {
        Ok(result) => {
            let items = result.as_array();
            let mut out = serde_json::Map::new();
            let keys = ["entity", "callers", "callees", "class"];
            if let Some(arr) = items {
                for (i, key) in keys.iter().enumerate() {
                    if let Some(data) = arr.get(i) {
                        out.insert(key.to_string(), data.clone());
                    }
                }
            }
            Json(serde_json::Value::Object(out)).into_response()
        }
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

// Hotspots
async fn api_hotspots(
    State(state): State<Arc<AppState>>,
    Query(rp): Query<RepoParam>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(rp.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    match gq
        .raw_query(
            "SELECT name, file_path, start_line, end_line, (end_line - start_line) AS size \
         FROM `function` ORDER BY size DESC LIMIT 50",
        )
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

// Clusters
async fn api_clusters(
    State(state): State<Arc<AppState>>,
    Query(rp): Query<RepoParam>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(rp.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    match gq
        .raw_query(
            "SELECT file_path, count() AS fn_count, array::group(name) AS functions \
         FROM `function` GROUP BY file_path ORDER BY fn_count DESC LIMIT 30",
        )
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

// Knowledge detail
#[derive(Deserialize)]
struct KnowledgeDetailParams {
    id: Option<String>,
    repo: Option<String>,
}

async fn api_knowledge_detail(
    State(state): State<Arc<AppState>>,
    Query(params): Query<KnowledgeDetailParams>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let id = params.id.unwrap_or_default();
    if id.is_empty() {
        return ApiError::invalid_input("No id").into_response();
    }
    let safe = id.replace('\'', "");
    let q = format!(
        "SELECT * FROM knowledge WHERE id = '{}'; \
         SELECT out.id AS id, out.title AS title, out.kind AS kind, context FROM supports WHERE in = '{}'; \
         SELECT out.id AS id, out.title AS title, out.kind AS kind, context FROM contradicts WHERE in = '{}'; \
         SELECT out.id AS id, out.title AS title, out.kind AS kind, relation FROM related_to WHERE in = '{}'; \
         SELECT in.id AS id, in.title AS title, in.kind AS kind, relation FROM related_to WHERE out = '{}'",
        safe, safe, safe, safe, safe
    );
    match gq.raw_query(&q).await {
        Ok(result) => {
            let items = result.as_array();
            let mut out = serde_json::Map::new();
            let keys = [
                "entity",
                "supports",
                "contradicts",
                "related_out",
                "related_in",
            ];
            if let Some(arr) = items {
                for (i, key) in keys.iter().enumerate() {
                    if let Some(data) = arr.get(i) {
                        out.insert(key.to_string(), data.clone());
                    }
                }
            }
            Json(serde_json::Value::Object(out)).into_response()
        }
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

// Skill graph
async fn api_skill_graph(
    State(state): State<Arc<AppState>>,
    Query(rp): Query<RepoParam>,
) -> impl IntoResponse {
    let gq = match state.resolve_query(rp.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let q =
        "SELECT name, qualified_name, description, node_type, file_path FROM skill ORDER BY name; \
             SELECT in.name AS source, out.name AS target, context FROM links_to";
    match gq.raw_query(q).await {
        Ok(result) => {
            let items = result.as_array();
            let mut out = serde_json::Map::new();
            if let Some(arr) = items {
                out.insert(
                    "nodes".into(),
                    arr.first()
                        .cloned()
                        .unwrap_or(serde_json::Value::Array(vec![])),
                );
                out.insert(
                    "edges".into(),
                    arr.get(1)
                        .cloned()
                        .unwrap_or(serde_json::Value::Array(vec![])),
                );
            }
            Json(serde_json::Value::Object(out)).into_response()
        }
        Err(e) => ApiError::internal(e.to_string()).into_response(),
    }
}

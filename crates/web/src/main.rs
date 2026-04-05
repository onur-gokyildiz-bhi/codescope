use anyhow::Result;
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use codescope_core::graph::query::GraphQuery;

#[derive(Parser)]
#[command(name = "codescope-web")]
#[command(about = "Codescope Web UI — Graph visualization dashboard")]
struct Args {
    /// Path to the codebase to visualize
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Database path (default: ~/.codescope/db/{repo})
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// Repository name (used to find DB at ~/.codescope/db/{repo})
    #[arg(long)]
    repo: Option<String>,

    /// Port to listen on
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Auto-index the codebase on startup
    #[arg(long)]
    auto_index: bool,
}

struct AppState {
    query: GraphQuery,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Resolve repo name: --repo > directory name > "default"
    let repo_name = args.repo.unwrap_or_else(|| {
        args.path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("default")
            .to_string()
    });
    let db_path = args.db_path.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codescope")
            .join("db")
            .join(&repo_name)
    });

    // Connect to SurrealDB
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = surrealdb::Surreal::new::<surrealdb::engine::local::SurrealKv>(
        db_path.to_string_lossy().as_ref(),
    )
    .await?;
    db.use_ns("codescope").use_db(&repo_name).await?;
    codescope_core::graph::schema::init_schema(&db).await?;
    println!("DB: {} (ns: codescope, db: {})", db_path.display(), repo_name);

    // Auto-index if requested
    if args.auto_index {
        let codebase_path = std::fs::canonicalize(&args.path).unwrap_or(args.path.clone());
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
                    parser.parse_source(std::path::Path::new(&rel_path), &content, &parse_repo).ok()
                })
                .collect::<Vec<_>>()
        })
        .await?;

        let mut file_count = 0;
        for (entities, relations) in results {
            let _ = builder.insert_entities(&entities).await;
            let _ = builder.insert_relations(&relations).await;
            file_count += 1;
        }
        println!("Indexed {} files", file_count);
    }

    let state = Arc::new(AppState {
        query: GraphQuery::new(db),
    });

    let app = Router::new()
        .route("/", get(index_page))
        .route("/api/stats", get(api_stats))
        .route("/api/search", get(api_search))
        .route("/api/graph", get(api_graph))
        .route("/api/query", get(api_raw_query))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", args.port);
    println!("Codescope Web UI: http://localhost:{}", args.port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

#[derive(Deserialize)]
struct SearchParams {
    q: Option<String>,
}

async fn api_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.query.raw_query("SELECT count() AS total FROM file GROUP ALL; SELECT count() AS total FROM `function` GROUP ALL; SELECT count() AS total FROM class GROUP ALL; SELECT count() AS total FROM import_decl GROUP ALL; SELECT count() AS total FROM config GROUP ALL; SELECT count() AS total FROM doc GROUP ALL; SELECT count() AS total FROM package GROUP ALL").await {
        Ok(result) => {
            // Flatten [[{"total":N}],...] → {"files":N,"functions":N,...}
            let labels = ["files", "functions", "classes", "imports", "configs", "docs", "packages"];
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
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
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
    match state.query.search_functions(&pattern).await {
        Ok(results) => Json(serde_json::json!(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct GraphParams {
    center: Option<String>,
    depth: Option<u32>,
}

#[derive(Serialize)]
struct GraphNode {
    id: String,
    name: String,
    kind: String,
    file_path: String,
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
    edges: Vec<GraphEdge>,
}

async fn api_graph(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GraphParams>,
) -> impl IntoResponse {
    let center = params.center.unwrap_or_default();
    let _depth = params.depth.unwrap_or(2);

    if center.is_empty() {
        // Return overview: top 50 functions with their call relationships
        let query = "SELECT name, qualified_name, file_path FROM `function` LIMIT 50";
        match state.query.raw_query(query).await {
            Ok(result) => {
                let mut nodes = Vec::new();
                let mut edges = Vec::new();

                for row in flatten_result(&result) {
                    let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let qname = row.get("qualified_name").and_then(|v| v.as_str()).unwrap_or(name);
                    let fp = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                    nodes.push(GraphNode {
                        id: qname.to_string(),
                        name: name.to_string(),
                        kind: "function".to_string(),
                        file_path: fp.to_string(),
                    });
                }

                // Get call edges with valid source/target
                let edge_query = "SELECT in.qualified_name AS source, out.qualified_name AS target FROM calls WHERE out.qualified_name != NONE LIMIT 200";
                if let Ok(edge_result) = state.query.raw_query(edge_query).await {
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

                Json(GraphData { nodes, edges }).into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        // Centered graph: find the entity + all its neighbors
        let safe = center.replace('\'', "");
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Helper to add a node if not already seen
        let add_node = |nodes: &mut Vec<GraphNode>, seen: &mut std::collections::HashSet<String>,
                            id: &str, name: &str, kind: &str, fp: &str| {
            if seen.insert(id.to_string()) {
                nodes.push(GraphNode {
                    id: id.to_string(),
                    name: name.to_string(),
                    kind: kind.to_string(),
                    file_path: fp.to_string(),
                });
            }
        };

        // 1. Find center entity details (try function first, then class)
        let mut center_id = safe.clone();
        let mut center_fp = String::new();
        let mut center_kind = "function".to_string();

        let fn_q = format!("SELECT name, qualified_name, file_path FROM `function` WHERE name = '{}' LIMIT 1", safe);
        if let Ok(result) = state.query.raw_query(&fn_q).await {
            if let Some(row) = flatten_result(&result).first() {
                center_id = row.get("qualified_name").and_then(|v| v.as_str()).unwrap_or(&safe).to_string();
                center_fp = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
        }
        if center_fp.is_empty() {
            let cls_q = format!("SELECT name, qualified_name, file_path, kind FROM class WHERE name = '{}' LIMIT 1", safe);
            if let Ok(result) = state.query.raw_query(&cls_q).await {
                if let Some(row) = flatten_result(&result).first() {
                    center_id = row.get("qualified_name").and_then(|v| v.as_str()).unwrap_or(&safe).to_string();
                    center_fp = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    center_kind = "class".to_string();
                }
            }
        }
        add_node(&mut nodes, &mut seen, &center_id, &safe, &center_kind, &center_fp);

        // 2. Callers (who calls this function) — direct edge traversal, no subquery
        let caller_q = format!(
            "SELECT in.name AS name, in.qualified_name AS qualified_name, in.file_path AS file_path \
             FROM calls WHERE out.name = '{}' AND in.name != NONE",
            safe
        );
        if let Ok(result) = state.query.raw_query(&caller_q).await {
            let wrapped = serde_json::json!([{"callers": result}]);
            extract_neighbors(&wrapped, "callers", &mut nodes, &mut edges, &mut seen, &center_id, true);
        }

        // 3. Callees (what this function calls) — direct edge traversal, no subquery
        let callee_q = format!(
            "SELECT out.name AS name, out.qualified_name AS qualified_name, out.file_path AS file_path \
             FROM calls WHERE in.name = '{}' AND out.name != NONE",
            safe
        );
        if let Ok(result) = state.query.raw_query(&callee_q).await {
            let wrapped = serde_json::json!([{"callees": result}]);
            extract_neighbors(&wrapped, "callees", &mut nodes, &mut edges, &mut seen, &center_id, false);
        }

        // 4. Sibling functions in same file
        if !center_fp.is_empty() {
            let sibling_q = format!(
                "SELECT name, qualified_name, file_path FROM `function` WHERE file_path = '{}' AND name != '{}' LIMIT 15",
                center_fp.replace('\'', ""), safe
            );
            if let Ok(result) = state.query.raw_query(&sibling_q).await {
                for row in flatten_result(&result) {
                    let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let qn = row.get("qualified_name").and_then(|v| v.as_str()).unwrap_or(name);
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
            add_node(&mut nodes, &mut seen, &file_id, &center_fp, "file", &center_fp);
            edges.push(GraphEdge {
                source: file_id,
                target: center_id.clone(),
                kind: "contains".to_string(),
            });
        }

        Json(GraphData { nodes, edges }).into_response()
    }
}

/// Flatten a SurrealDB raw_query result into a Vec of objects.
/// raw_query returns Value::Array([Value::Array([...])]) or Value::Array([obj, obj...])
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
                let qn = n.get("qualified_name").and_then(|v| v.as_str()).unwrap_or(name);
                let fp = n.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                let kind = if is_caller { "caller" } else { "callee" };
                if seen.insert(qn.to_string()) {
                    nodes.push(GraphNode {
                        id: qn.to_string(),
                        name: name.to_string(),
                        kind: kind.to_string(),
                        file_path: fp.to_string(),
                    });
                }
                let (src, tgt) = if is_caller {
                    (qn.to_string(), center_id.to_string())
                } else {
                    (center_id.to_string(), qn.to_string())
                };
                edges.push(GraphEdge { source: src, target: tgt, kind: "calls".to_string() });
            }
        }
    }
}

#[derive(Deserialize)]
struct RawQueryParams {
    q: Option<String>,
}

async fn api_raw_query(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RawQueryParams>,
) -> impl IntoResponse {
    let query = params.q.unwrap_or_default();
    if query.is_empty() {
        return Json(serde_json::json!({"error": "No query provided"})).into_response();
    }
    match state.query.raw_query(&query).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

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

use codescope_core::graph::query::GraphQuery;

struct AppState {
    query: GraphQuery,
}

/// Run the web visualization server.
/// This is the main entry point used by both the standalone binary and the unified CLI.
pub async fn run_web(
    path: PathBuf,
    repo: Option<String>,
    port: u16,
    auto_index: bool,
    db_path_override: Option<PathBuf>,
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
    println!(
        "DB: {} (ns: codescope, db: {})",
        db_path.display(),
        repo_name
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
        query: GraphQuery::new(db),
    });

    let app = Router::new()
        .route("/", get(index_page))
        .route("/api/stats", get(api_stats))
        .route("/api/search", get(api_search))
        .route("/api/graph", get(api_graph))
        .route("/api/query", get(api_raw_query))
        .route("/api/conversations", get(api_conversations))
        .route("/api/files", get(api_files))
        .route("/api/file-content", get(api_file_content))
        .route("/api/node-detail", get(api_node_detail))
        .route("/api/hotspots", get(api_hotspots))
        .route("/api/clusters", get(api_clusters))
        .route("/api/skill-graph", get(api_skill_graph))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    println!("Codescope Web UI: http://localhost:{}", port);

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
            // Flatten [[{"total":N}],...] -> {"files":N,"functions":N,...}
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
        if let Ok(result) = state.query.raw_query(&fn_q).await {
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
            if let Ok(result) = state.query.raw_query(&cls_q).await {
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
        if let Ok(result) = state.query.raw_query(&caller_q).await {
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
        if let Ok(result) = state.query.raw_query(&callee_q).await {
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
            if let Ok(result) = state.query.raw_query(&sibling_q).await {
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

        Json(GraphData { nodes, edges }).into_response()
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

async fn api_conversations(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let query = "\
        SELECT name, body, kind, timestamp, file_path FROM decision ORDER BY timestamp DESC LIMIT 50; \
        SELECT name, body, kind, timestamp, file_path FROM problem ORDER BY timestamp DESC LIMIT 50; \
        SELECT name, body, kind, timestamp, file_path FROM solution ORDER BY timestamp DESC LIMIT 50; \
        SELECT name, body, kind, timestamp FROM conv_topic ORDER BY timestamp DESC LIMIT 30; \
        SELECT name, qualified_name, file_path, body FROM conversation ORDER BY name LIMIT 20; \
        SELECT out.name AS entity, in.name AS segment, 'decided_about' AS rel \
            FROM decided_about LIMIT 50; \
        SELECT out.name AS entity, in.name AS segment, 'discussed_in' AS rel \
            FROM discussed_in LIMIT 50;";

    match state.query.raw_query(query).await {
        Ok(result) => {
            let items = result.as_array();
            let mut out = serde_json::Map::new();
            let keys = [
                "decisions",
                "problems",
                "solutions",
                "topics",
                "sessions",
                "code_decisions",
                "code_discussions",
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
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
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

// File tree
async fn api_files(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state
        .query
        .raw_query("SELECT path, language, line_count FROM file ORDER BY path")
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// File content
#[derive(Deserialize)]
struct FileContentParams {
    path: Option<String>,
}

async fn api_file_content(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FileContentParams>,
) -> impl IntoResponse {
    let path = params.path.unwrap_or_default();
    if path.is_empty() {
        return Json(serde_json::json!({"error": "No path"})).into_response();
    }
    let entities = state.query.raw_query(&format!(
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
}

async fn api_node_detail(
    State(state): State<Arc<AppState>>,
    Query(params): Query<NodeDetailParams>,
) -> impl IntoResponse {
    let name = params.name.unwrap_or_default();
    if name.is_empty() {
        return Json(serde_json::json!({"error": "No name"})).into_response();
    }
    let q = format!(
        "SELECT name, qualified_name, signature, file_path, start_line, end_line FROM `function` WHERE name = '{}'; \
         SELECT in.name AS name, in.file_path AS file_path, in.signature AS sig FROM calls WHERE out.name = '{}' AND in.name != NONE LIMIT 20; \
         SELECT out.name AS name, out.file_path AS file_path, out.signature AS sig FROM calls WHERE in.name = '{}' AND out.name != NONE LIMIT 20; \
         SELECT name, kind, file_path, start_line FROM class WHERE name = '{}'",
        name.replace('\'', "\\'"), name.replace('\'', "\\'"),
        name.replace('\'', "\\'"), name.replace('\'', "\\'"),
    );
    match state.query.raw_query(&q).await {
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
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// Hotspots
async fn api_hotspots(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state
        .query
        .raw_query(
            "SELECT name, file_path, start_line, end_line, (end_line - start_line) AS size \
         FROM `function` ORDER BY size DESC LIMIT 50",
        )
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// Clusters
async fn api_clusters(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state
        .query
        .raw_query(
            "SELECT file_path, count() AS fn_count, array::group(name) AS functions \
         FROM `function` GROUP BY file_path ORDER BY fn_count DESC LIMIT 30",
        )
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// Skill graph
async fn api_skill_graph(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let q =
        "SELECT name, qualified_name, description, node_type, file_path FROM skill ORDER BY name; \
             SELECT in.name AS source, out.name AS target, context FROM links_to";
    match state.query.raw_query(q).await {
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
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

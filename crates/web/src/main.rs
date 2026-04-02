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
    /// Database path
    #[arg(long, default_value = ".graph-rag/db")]
    db_path: PathBuf,

    /// Port to listen on
    #[arg(long, default_value = "8080")]
    port: u16,
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

    // Connect to SurrealDB
    if let Some(parent) = args.db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = surrealdb::Surreal::new::<surrealdb::engine::local::RocksDb>(
        args.db_path.to_string_lossy().as_ref(),
    )
    .await?;
    db.use_ns("graph_rag").use_db("code").await?;
    codescope_core::graph::schema::init_schema(&db).await?;

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
        Ok(result) => Json(result).into_response(),
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

                if let Some(arr) = result.as_array() {
                    for item in arr {
                        if let Some(inner) = item.as_array() {
                            for row in inner {
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
                        }
                    }
                }

                // Get call edges
                let edge_query = "SELECT in.qualified_name AS source, out.qualified_name AS target FROM calls LIMIT 200";
                if let Ok(edge_result) = state.query.raw_query(edge_query).await {
                    if let Some(arr) = edge_result.as_array() {
                        for item in arr {
                            if let Some(inner) = item.as_array() {
                                for row in inner {
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
                        }
                    }
                }

                Json(GraphData { nodes, edges }).into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        // Centered graph: find callers and callees
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // Center node
        nodes.push(GraphNode {
            id: center.clone(),
            name: center.clone(),
            kind: "function".to_string(),
            file_path: String::new(),
        });

        // Callers
        let caller_q = format!(
            "SELECT in.name AS name, in.qualified_name AS qname, in.file_path AS file_path FROM calls WHERE out.name = '{}'",
            center.replace('\'', "")
        );
        if let Ok(result) = state.query.raw_query(&caller_q).await {
            if let Some(arr) = result.as_array() {
                for item in arr {
                    if let Some(inner) = item.as_array() {
                        for row in inner {
                            let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let qname = row.get("qname").and_then(|v| v.as_str()).unwrap_or(name);
                            let fp = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                            nodes.push(GraphNode {
                                id: qname.to_string(),
                                name: name.to_string(),
                                kind: "caller".to_string(),
                                file_path: fp.to_string(),
                            });
                            edges.push(GraphEdge {
                                source: qname.to_string(),
                                target: center.clone(),
                                kind: "calls".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Callees
        let callee_q = format!(
            "SELECT out.name AS name, out.qualified_name AS qname, out.file_path AS file_path FROM calls WHERE in.name = '{}'",
            center.replace('\'', "")
        );
        if let Ok(result) = state.query.raw_query(&callee_q).await {
            if let Some(arr) = result.as_array() {
                for item in arr {
                    if let Some(inner) = item.as_array() {
                        for row in inner {
                            let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let qname = row.get("qname").and_then(|v| v.as_str()).unwrap_or(name);
                            let fp = row.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
                            nodes.push(GraphNode {
                                id: qname.to_string(),
                                name: name.to_string(),
                                kind: "callee".to_string(),
                                file_path: fp.to_string(),
                            });
                            edges.push(GraphEdge {
                                source: center.clone(),
                                target: qname.to_string(),
                                kind: "calls".to_string(),
                            });
                        }
                    }
                }
            }
        }

        Json(GraphData { nodes, edges }).into_response()
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

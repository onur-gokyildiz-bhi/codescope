//! Phase 3 Dream — narrated tours through the knowledge graph.
//!
//! An *arc* is a coherent slice of project memory: a set of
//! `knowledge` entities (decisions, problems, solutions, claims,
//! concepts) that share a topical tag and belong to the same story.
//! The Dream page on the frontend picks an arc, flies the 3D graph
//! from scene to scene, and shows a generated narration card per
//! stop.
//!
//! Arc discovery (MVP v1) is tag-based:
//! * scan every `tags` array in the `knowledge` table;
//! * drop meta tags (`status:*`, `shipped:*`, `session*`, `phase-*`,
//!   `v*`, `bug:*`) — those group across arcs, not inside one;
//! * the remaining tags become arc candidates;
//! * any tag with fewer than two entries is discarded — you can't
//!   tell a story with one scene.
//!
//! Narration is template-based here. LLM narration is a later
//! iteration (cached into a `dream_narration` column so we don't
//! pay Ollama/API costs on every page load).

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::ApiError;
use crate::AppState;

/// Arcs live under user-provided tag strings. Keep the set narrow
/// enough that we can inline them into SurrealQL without binding
/// (raw_query doesn't support parameters today). Anything outside
/// this character class is rejected as `invalid_input`.
fn is_safe_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 128
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.' | '/'))
}

/// One candidate arc — summary shape returned by `/api/dream/arcs`.
#[derive(Serialize)]
pub struct ArcSummary {
    pub id: String,
    pub title: String,
    pub tag: String,
    pub count: usize,
    pub first_at: Option<String>,
    pub last_at: Option<String>,
    pub kinds: Vec<String>,
}

/// A single scene in an arc — one knowledge entity, with generated
/// narration attached.
#[derive(Serialize)]
pub struct Scene {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub created_at: Option<String>,
    pub tags: Vec<String>,
    /// Template-generated narration for this scene. Fed to the
    /// narration card on the frontend. LLM v2 will overwrite this
    /// with a richer string.
    pub narration: String,
}

#[derive(Serialize)]
pub struct ArcDetail {
    pub id: String,
    pub title: String,
    pub tag: String,
    pub scenes: Vec<Scene>,
}

#[derive(Deserialize)]
pub struct ArcQuery {
    pub repo: Option<String>,
}

/// Tags that span many unrelated stories — exclude from arc candidates.
fn is_meta_tag(t: &str) -> bool {
    let low = t.to_ascii_lowercase();
    low.starts_with("status:")
        || low.starts_with("shipped:")
        || low.starts_with("session")
        || low.starts_with("phase-")
        || low.starts_with("bug:")
        || (low.starts_with('v')
            && low.len() > 1
            && low[1..]
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false))
}

/// Convert `autocalib` → `Autocalib`, `surreal-migration` → `Surreal Migration`.
fn humanise_tag(tag: &str) -> String {
    tag.split(|c: char| c == '-' || c == '_')
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Primary handler: enumerate candidate arcs in the repo.
pub async fn api_dream_arcs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ArcQuery>,
) -> impl IntoResponse {
    let query = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };

    // Pull the minimum needed from Surreal — order by created_at so
    // the aggregation below can just collect() first/last without a
    // second sort.
    let sql =
        "SELECT id, kind, title, tags, created_at FROM knowledge ORDER BY created_at ASC LIMIT 5000";
    let rows = match query.raw_query(sql).await {
        Ok(v) => flatten_rows(v),
        Err(e) => {
            return ApiError::from_db_err(params.repo.as_deref().unwrap_or("?"), e).into_response();
        }
    };

    // Group by tag.
    #[derive(Default)]
    struct Agg {
        count: usize,
        first_at: Option<String>,
        last_at: Option<String>,
        kinds: std::collections::BTreeSet<String>,
    }
    let mut by_tag: BTreeMap<String, Agg> = BTreeMap::new();
    for row in &rows {
        let tags = row
            .get("tags")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let kind = row
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("knowledge")
            .to_string();
        let created = row
            .get("created_at")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        for t in tags {
            let Some(tag) = t.as_str() else { continue };
            if is_meta_tag(tag) {
                continue;
            }
            let entry = by_tag.entry(tag.to_string()).or_default();
            entry.count += 1;
            entry.kinds.insert(kind.clone());
            if entry.first_at.is_none()
                || created
                    .as_deref()
                    .zip(entry.first_at.as_deref())
                    .map(|(a, b)| a < b)
                    .unwrap_or(false)
            {
                if let Some(c) = &created {
                    entry.first_at = Some(c.clone());
                }
            }
            if entry.last_at.is_none()
                || created
                    .as_deref()
                    .zip(entry.last_at.as_deref())
                    .map(|(a, b)| a > b)
                    .unwrap_or(false)
            {
                if let Some(c) = &created {
                    entry.last_at = Some(c.clone());
                }
            }
        }
    }

    let mut arcs: Vec<ArcSummary> = by_tag
        .into_iter()
        .filter(|(_, a)| a.count >= 2)
        .map(|(tag, a)| ArcSummary {
            id: tag.clone(),
            title: humanise_tag(&tag),
            tag,
            count: a.count,
            first_at: a.first_at,
            last_at: a.last_at,
            kinds: a.kinds.into_iter().collect(),
        })
        .collect();

    // Most-recently-updated arcs first — that's usually what the
    // user wants to relive.
    arcs.sort_by(|a, b| b.last_at.cmp(&a.last_at));

    axum::response::Json(serde_json::json!({ "arcs": arcs })).into_response()
}

/// Arc detail — returns the scenes for a specific tag.
pub async fn api_dream_arc(
    State(state): State<Arc<AppState>>,
    Path(arc_id): Path<String>,
    Query(params): Query<ArcQuery>,
) -> impl IntoResponse {
    let query = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };

    if !is_safe_tag(&arc_id) {
        return ApiError::invalid_input("arc id must be alphanumeric + `-_:./`, ≤128 chars")
            .into_response();
    }

    // `raw_query` doesn't support params today; the is_safe_tag guard
    // above restricts arc_id to SurrealQL-safe characters, so inlining
    // the tag as a quoted literal is not a SQL-injection risk.
    // ORDER BY created_at ASC so narration reads chronologically.
    let sql = format!(
        "SELECT id, kind, title, content, tags, created_at FROM knowledge \
         WHERE '{}' IN tags ORDER BY created_at ASC LIMIT 500",
        arc_id.replace('\'', "")
    );
    let rows = match query.raw_query(&sql).await {
        Ok(v) => flatten_rows(v),
        Err(e) => {
            return ApiError::from_db_err(params.repo.as_deref().unwrap_or("?"), e).into_response();
        }
    };

    let mut scenes = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let id = row
            .get("id")
            .and_then(|v| v.as_str().map(str::to_string))
            .or_else(|| {
                // Surreal sometimes emits id as {tb: "knowledge", id: {...}}
                row.get("id").map(|v| v.to_string())
            })
            .unwrap_or_default();
        let kind = row
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("knowledge")
            .to_string();
        let title = row
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("(untitled)")
            .to_string();
        let content = row
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let created_at = row
            .get("created_at")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let tags: Vec<String> = row
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let narration = render_narration(i, &kind, &title, &content, created_at.as_deref());
        scenes.push(Scene {
            id,
            kind,
            title,
            content,
            created_at,
            tags,
            narration,
        });
    }

    let detail = ArcDetail {
        id: arc_id.clone(),
        title: humanise_tag(&arc_id),
        tag: arc_id,
        scenes,
    };
    axum::response::Json(detail).into_response()
}

/// Template narration — deliberate first-person voice so the Dream
/// page reads like a memoir rather than a log dump. Keep it short:
/// 2-3 sentences max, extracted from the entity itself.
fn render_narration(
    idx: usize,
    kind: &str,
    title: &str,
    content: &str,
    created_at: Option<&str>,
) -> String {
    let date_str = created_at
        .and_then(|s| s.split('T').next())
        .unwrap_or("an earlier day");
    let summary = first_sentence(content);

    let opener = match idx {
        0 => format!("On {date_str} the story begins."),
        _ => format!("Then, on {date_str},"),
    };

    let kind_phrase = match kind {
        "decision" => "you decided:",
        "problem" => "you hit a wall —",
        "solution" => "the fix landed:",
        "claim" => "you noted:",
        "concept" => "a new concept surfaced:",
        _ => "you captured:",
    };

    if summary.is_empty() {
        format!("{opener} {kind_phrase} **{title}**.")
    } else {
        format!("{opener} {kind_phrase} **{title}**. {summary}")
    }
}

/// Trim content to one plain-text sentence (up to ~240 chars) for
/// narration. We strip code fences and trailing whitespace.
fn first_sentence(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Drop leading markdown headings / fences.
    let start = trimmed.find(|c: char| c.is_alphanumeric()).unwrap_or(0);
    let body = &trimmed[start..];
    // First sentence break: `.`, `!`, `?`, or newline.
    let end = body
        .find(|c: char| matches!(c, '.' | '!' | '?' | '\n'))
        .map(|i| i + 1)
        .unwrap_or(body.len());
    let mut s = body[..end.min(body.len())].trim().to_string();
    const MAX: usize = 240;
    if s.len() > MAX {
        s.truncate(MAX);
        s.push('…');
    }
    s
}

/// `GraphQuery::raw_query` returns a flat `Array(rows)` for a single
/// statement, but `Array(Array(stmt_rows))` when multiple statements
/// were issued. We only issue one statement here — but accept both
/// shapes in case the upstream helper's backward-compat path ever
/// flips. An empty/non-array value becomes an empty row list.
fn flatten_rows(v: serde_json::Value) -> Vec<serde_json::Value> {
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    // Heuristic: if every top-level element is itself an array, treat
    // it as the nested shape and take element 0; otherwise the outer
    // array already IS the row list.
    let nested = !arr.is_empty() && arr.iter().all(|e| e.is_array());
    if nested {
        arr.first()
            .and_then(|inner| inner.as_array())
            .cloned()
            .unwrap_or_default()
    } else {
        arr.clone()
    }
}

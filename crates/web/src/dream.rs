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
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
    /// Dream-B — if this scene has ≥0.7 Jaccard similarity with an
    /// earlier scene in the same arc, this is the earlier scene's
    /// id (1-indexed position as well, for easy UI labelling).
    /// `None` when there's nothing that looks like a duplicate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<DuplicateRef>,
}

#[derive(Serialize, Clone)]
pub struct DuplicateRef {
    pub id: String,
    pub index: usize,
    pub score: f32,
}

/// Dream-E — a proposed `RELATE` edge between two scenes. Shown
/// on the active scene card; the user clicks to accept, which
/// writes the actual relation. Inlined on arc-detail so we don't
/// pay a second round-trip.
#[derive(Serialize, Clone)]
pub struct EdgeProposal {
    pub from_id: String,
    pub to_id: String,
    pub to_index: usize,
    pub to_title: String,
    pub relation: &'static str,
    pub score: f32,
    pub reason: String,
}

#[derive(Serialize)]
pub struct ArcDetail {
    pub id: String,
    pub title: String,
    pub tag: String,
    pub scenes: Vec<Scene>,
    /// Dream-E — proposed RELATE edges between scenes. Computed
    /// from kind + Jaccard overlap, no LLM. Each proposal the
    /// user hasn't accepted stays here; accepted ones are
    /// removed client-side after a successful POST.
    pub edge_proposals: Vec<EdgeProposal>,
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
    tag.split(['-', '_'])
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
            duplicate_of: None,
        });
    }

    // Dream-B — flag likely duplicates. For each pair (i < j),
    // compute Jaccard on the title + first 400 chars of content.
    // If ≥0.7, mark j as duplicate_of i. Quadratic but arcs are
    // small (<500 scenes guaranteed by the SELECT LIMIT).
    let bags: Vec<BTreeSet<String>> = scenes
        .iter()
        .map(|s| {
            meaningful_words(&format!("{} {}", s.title, first_n_chars(&s.content, 400)))
                .into_iter()
                .collect()
        })
        .collect();
    for j in 0..scenes.len() {
        if bags[j].is_empty() {
            continue;
        }
        for i in 0..j {
            if bags[i].is_empty() {
                continue;
            }
            let inter = bags[i].intersection(&bags[j]).count();
            if inter == 0 {
                continue;
            }
            let union = bags[i].len() + bags[j].len() - inter;
            let score = inter as f32 / union.max(1) as f32;
            if score >= 0.7 {
                scenes[j].duplicate_of = Some(DuplicateRef {
                    id: scenes[i].id.clone(),
                    index: i,
                    score,
                });
                break;
            }
        }
    }

    // Dream-D — upgrade narration in-place if the LLM cache
    // has an entry for this arc. First fetch of a given arc
    // kicks off background generation; the user sees template
    // narration immediately and the LLM version on the next
    // refresh. Silent no-op when `CODESCOPE_LLM_URL` /
    // `CODESCOPE_LLM_MODEL` aren't set.
    apply_llm_narration(&arc_id, &mut scenes);

    let edge_proposals = propose_edges(&scenes);

    let detail = ArcDetail {
        id: arc_id.clone(),
        title: humanise_tag(&arc_id),
        tag: arc_id,
        scenes,
        edge_proposals,
    };
    axum::response::Json(detail).into_response()
}

// ── Dream-D: LLM narration + cache ─────────────────────────────

use std::sync::{Mutex, OnceLock};

type NarrationCache = std::collections::HashMap<u64, Vec<String>>;

fn cache() -> &'static Mutex<NarrationCache> {
    static CACHE: OnceLock<Mutex<NarrationCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(NarrationCache::new()))
}

/// Hash the arc's identity so a scene-set change (new scene
/// landed, order changed) invalidates the cached narration.
fn arc_cache_key(arc_id: &str, scenes: &[Scene]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    arc_id.hash(&mut h);
    for s in scenes {
        s.id.hash(&mut h);
    }
    h.finish()
}

fn apply_llm_narration(arc_id: &str, scenes: &mut [Scene]) {
    let Some(cfg) = codescope_core::llm::LlmConfig::from_env() else {
        return;
    };
    let key = arc_cache_key(arc_id, scenes);

    // Cache hit → overwrite narration.
    if let Some(cached) = cache().lock().ok().and_then(|m| m.get(&key).cloned()) {
        for (i, s) in scenes.iter_mut().enumerate() {
            if let Some(line) = cached.get(i) {
                if !line.trim().is_empty() {
                    s.narration = line.clone();
                }
            }
        }
        return;
    }

    // Cache miss — kick off background generation. Template
    // narration stays on this response; the next fetch gets
    // LLM output.
    let arc_id_owned = arc_id.to_string();
    let scene_data: Vec<(String, String, String, String)> = scenes
        .iter()
        .map(|s| {
            (
                s.kind.clone(),
                s.title.clone(),
                s.created_at.clone().unwrap_or_default(),
                first_n_chars(&s.content, 600),
            )
        })
        .collect();
    tokio::spawn(async move {
        match generate_llm_narrations(&cfg, &arc_id_owned, &scene_data).await {
            Ok(lines) if !lines.is_empty() => {
                if let Ok(mut g) = cache().lock() {
                    g.insert(key, lines);
                }
            }
            Ok(_) => { /* empty response — skip cache */ }
            Err(e) => {
                tracing::debug!("Dream-D LLM generation failed: {e}");
            }
        }
    });
}

/// Build one prompt that asks the LLM for one narration line
/// per scene, numbered. Parse the response back into the scene
/// order. One network round-trip per arc = cheaper than N.
async fn generate_llm_narrations(
    cfg: &codescope_core::llm::LlmConfig,
    arc_id: &str,
    scenes: &[(String, String, String, String)],
) -> anyhow::Result<Vec<String>> {
    let mut prompt = String::with_capacity(4096);
    prompt.push_str(
        "You are narrating a chronological arc of a software project's \
         decisions, problems, and solutions. Each scene below is one \
         entry from a knowledge graph. Write ONE short narration line per \
         scene (25–60 words), in second-person past tense (\"you decided …\", \
         \"you solved …\"). Keep tone reflective, not celebratory. Number \
         your lines 1. 2. 3. … exactly matching the scene numbers. \
         Output only the numbered lines, nothing else.\n\n",
    );
    prompt.push_str(&format!("Arc topic: {arc_id}\n\n"));
    for (i, (kind, title, date, content)) in scenes.iter().enumerate() {
        prompt.push_str(&format!(
            "Scene {n} [{kind}, {date}]: {title}\n{content}\n\n",
            n = i + 1,
            kind = kind,
            date = date,
            title = title,
            content = content.replace("\n\n", "\n")
        ));
    }
    let text = codescope_core::llm::complete(cfg, &prompt).await?;
    Ok(parse_numbered_response(&text, scenes.len()))
}

/// Split "1. foo\n2. bar\n..." into a `Vec<String>` of length
/// `expected`. Missing entries come back as empty strings — the
/// caller keeps the template narration for those scenes.
fn parse_numbered_response(text: &str, expected: usize) -> Vec<String> {
    let mut out = vec![String::new(); expected];
    let mut current_idx: Option<usize> = None;
    let mut buf = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        // Match "1. ", "2) ", "10. " …
        let mut ni = 0;
        let bytes = trimmed.as_bytes();
        while ni < bytes.len() && bytes[ni].is_ascii_digit() {
            ni += 1;
        }
        let is_numbered = ni > 0
            && ni < bytes.len()
            && matches!(bytes[ni], b'.' | b')')
            && ni + 1 < bytes.len()
            && bytes[ni + 1] == b' ';
        if is_numbered {
            if let Some(idx) = current_idx {
                if idx < out.len() {
                    out[idx] = buf.trim().to_string();
                }
            }
            let n: usize = trimmed[..ni].parse().unwrap_or(0);
            current_idx = Some(n.saturating_sub(1));
            buf = trimmed[ni + 2..].trim().to_string();
        } else if current_idx.is_some() {
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(trimmed);
        }
    }
    if let Some(idx) = current_idx {
        if idx < out.len() {
            out[idx] = buf.trim().to_string();
        }
    }
    out
}

/// Dream-E — rule-based edge proposer. Kind pair + Jaccard
/// overlap decides which relation, if any, to propose between
/// two scenes.
///
/// * `solution` + `problem`  →  `solves_for`   (solution → problem)
/// * `decision` + `problem`  →  `decided_about` (decision → problem)
/// * `decision` + `claim`    →  `decided_about`
/// * anything else with overlap ≥0.4 → `related_to`
/// * duplicate pairs are skipped — Dream-B already flagged
///   those.
fn propose_edges(scenes: &[Scene]) -> Vec<EdgeProposal> {
    let bags: Vec<BTreeSet<String>> = scenes
        .iter()
        .map(|s| {
            meaningful_words(&format!("{} {}", s.title, first_n_chars(&s.content, 400)))
                .into_iter()
                .collect()
        })
        .collect();
    let mut out = Vec::new();
    for i in 0..scenes.len() {
        if scenes[i].duplicate_of.is_some() || bags[i].is_empty() {
            continue;
        }
        for j in 0..scenes.len() {
            if i == j || scenes[j].duplicate_of.is_some() || bags[j].is_empty() {
                continue;
            }
            let inter = bags[i].intersection(&bags[j]).count();
            if inter == 0 {
                continue;
            }
            let union = bags[i].len() + bags[j].len() - inter;
            let score = inter as f32 / union.max(1) as f32;
            if score < 0.3 {
                continue;
            }
            let (relation, reason) = match (scenes[i].kind.as_str(), scenes[j].kind.as_str()) {
                ("solution", "problem") => (
                    "solves_for",
                    format!(
                        "{}% shared vocabulary — solution looks like it closes this problem",
                        (score * 100.0) as u32
                    ),
                ),
                ("decision", "problem") | ("decision", "claim") => (
                    "decided_about",
                    format!("decision overlaps the {} scene", scenes[j].kind),
                ),
                // Symmetric cases flip the relation's direction;
                // we emit once, from the "stronger" kind to the
                // weaker — no need to emit a to b and b to a.
                ("problem", "solution") | ("claim", "decision") | ("problem", "decision") => {
                    continue;
                }
                _ if scenes[i].kind == scenes[j].kind => continue,
                _ if score >= 0.4 => (
                    "related_to",
                    format!("{}% shared vocabulary", (score * 100.0) as u32),
                ),
                _ => continue,
            };
            out.push(EdgeProposal {
                from_id: scenes[i].id.clone(),
                to_id: scenes[j].id.clone(),
                to_index: j,
                to_title: scenes[j].title.clone(),
                relation,
                score,
                reason,
            });
        }
    }
    // Highest-confidence first; cap to keep the UI quiet.
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(50);
    out
}

/// `POST /api/dream/relate?repo=X` with `{from_id, to_id, relation}`
/// — writes a RELATE between two knowledge records. Relation is
/// restricted to the same whitelist we accept for arc IDs; ids
/// are escaped the same way `apply-tag` does.
#[derive(Deserialize)]
pub struct RelateBody {
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
}

/// Valid relation names. Kept tight — the schema allows more,
/// but these are the ones the Dream-E proposer can emit.
const ALLOWED_RELATIONS: &[&str] = &[
    "related_to",
    "solves_for",
    "decided_about",
    "supports",
    "contradicts",
    "links_to",
];

pub async fn api_dream_relate(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ArcQuery>,
    axum::Json(body): axum::Json<RelateBody>,
) -> impl IntoResponse {
    let query = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    if !ALLOWED_RELATIONS.contains(&body.relation.as_str()) {
        return ApiError::invalid_input("relation not in the allowed set").into_response();
    }
    let from = sanitise_knowledge_id(&body.from_id);
    let to = sanitise_knowledge_id(&body.to_id);
    let (Some(from), Some(to)) = (from, to) else {
        return ApiError::invalid_input("malformed knowledge id").into_response();
    };
    let sql = format!(
        "RELATE `knowledge`:\u{27E8}{from}\u{27E9}->`{rel}`->`knowledge`:\u{27E8}{to}\u{27E9}",
        from = from,
        to = to,
        rel = body.relation,
    );
    match query.raw_query(&sql).await {
        Ok(_) => axum::response::Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => ApiError::from_db_err(params.repo.as_deref().unwrap_or("?"), e).into_response(),
    }
}

fn sanitise_knowledge_id(raw: &str) -> Option<String> {
    let id = raw
        .trim_start_matches("knowledge:")
        .trim_matches('\u{27E8}')
        .trim_matches('\u{27E9}');
    if id.is_empty() || id.len() > 256 || !id.chars().all(is_record_id_char) {
        return None;
    }
    Some(id.to_string())
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

    // For the first scene the opener ends in a period, so the
    // `kind_phrase` starts a new sentence and needs capitalisation.
    // Every later scene's opener ends in a comma — lowercase stays.
    let (opener, sentence_start) = match idx {
        0 => (format!("On {date_str} the story begins."), true),
        _ => (format!("Then, on {date_str},"), false),
    };

    let phrase = match kind {
        "decision" => "you decided:",
        "problem" => "you hit a wall —",
        "solution" => "the fix landed:",
        "claim" => "you noted:",
        "concept" => "a new concept surfaced:",
        _ => "you captured:",
    };
    let kind_phrase = if sentence_start {
        capitalise_first(phrase)
    } else {
        phrase.to_string()
    };

    // Use typographic quotes around the title so the output reads
    // cleanly both as plain prose (frontend) and as markdown
    // (export) — avoids literal `**` leaking into the UI.
    let quoted_title = format!("\u{201C}{title}\u{201D}");

    if summary.is_empty() {
        format!("{opener} {kind_phrase} {quoted_title}.")
    } else {
        format!("{opener} {kind_phrase} {quoted_title}. {summary}")
    }
}

fn capitalise_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Trim content to one plain-text sentence (up to ~240 chars) for
/// narration. Strips markdown scaffolding (headings, bold, bullets,
/// code fences) so the narration reads as prose rather than raw
/// markdown.
fn first_sentence(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Find the first line that isn't a markdown header, bullet, or
    // fence — otherwise we narrate "##" back to the user.
    let body_line = trimmed
        .lines()
        .find(|l| {
            let t = l.trim_start();
            !t.is_empty()
                && !t.starts_with('#')
                && !t.starts_with("```")
                && !t.starts_with("- ")
                && !t.starts_with("* ")
                && !t.starts_with("> ")
                && !t.starts_with('|')
        })
        .unwrap_or("");
    let stripped = strip_md_inline(body_line);
    let body = stripped.trim();
    if body.is_empty() {
        return String::new();
    }
    // Skip leading non-alphanum noise (leftover colons / punctuation).
    let start = body.find(char::is_alphanumeric).unwrap_or(0);
    let body = &body[start..];
    // First sentence break: `.`, `!`, `?`.
    let end = body
        .find(['.', '!', '?'])
        .map(|i| i + 1)
        .unwrap_or(body.len());
    let mut s = body[..end.min(body.len())].trim().to_string();
    // Uppercase the first letter so the narration reads as prose.
    if let Some(first) = s.chars().next() {
        if first.is_ascii_lowercase() {
            let mut c = s.chars();
            let head = c.next().unwrap().to_ascii_uppercase();
            s = std::iter::once(head).chain(c).collect();
        }
    }
    const MAX: usize = 240;
    if s.chars().count() > MAX {
        // Truncate on a char boundary so multi-byte chars survive.
        let mut cut = s.len().min(MAX * 4);
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push('…');
    }
    s
}

/// Remove inline markdown emphasis (`**`, `*`, `_`, `` ` ``) so the
/// narration doesn't show raw markers. Non-emphasis punctuation is
/// left alone.
fn strip_md_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' => {
                while let Some(&n) = chars.peek() {
                    if n == c {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            '`' => { /* strip backticks */ }
            _ => out.push(c),
        }
    }
    out
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

// ── Dream-A: auto-tag suggestion ──────────────────────────────

/// One suggestion — an untagged knowledge entry and the top-3
/// arcs it most likely belongs to, with Jaccard scores.
#[derive(Serialize)]
pub struct Suggestion {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub candidates: Vec<SuggestionCandidate>,
}

#[derive(Serialize)]
pub struct SuggestionCandidate {
    pub tag: String,
    pub score: f32,
    pub matched_words: Vec<String>,
}

/// `GET /api/dream/suggest-tags?repo=X` — propose topical tags
/// for entries that have none. Jaccard on title + first 400
/// chars of content; threshold 0.15; top-3 per entry; top-50
/// entries overall.
pub async fn api_dream_suggest(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ArcQuery>,
) -> impl IntoResponse {
    let query = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    let sql = "SELECT id, kind, title, content, tags FROM knowledge LIMIT 5000";
    let rows = match query.raw_query(sql).await {
        Ok(v) => flatten_rows(v),
        Err(e) => {
            return ApiError::from_db_err(params.repo.as_deref().unwrap_or("?"), e).into_response();
        }
    };
    let suggestions = build_suggestions(&rows);
    axum::response::Json(serde_json::json!({ "suggestions": suggestions })).into_response()
}

#[derive(Deserialize)]
pub struct ApplyTagBody {
    pub id: String,
    pub tag: String,
}

/// `POST /api/dream/apply-tag?repo=X` with JSON body
/// `{id, tag}` — add a tag to one knowledge entry. Both fields
/// are restricted to SurrealQL-safe characters so we can inline
/// them without a bound-parameter path (raw_query has no binds
/// yet).
pub async fn api_dream_apply_tag(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ArcQuery>,
    axum::Json(body): axum::Json<ApplyTagBody>,
) -> impl IntoResponse {
    let query = match state.resolve_query(params.repo.as_deref()).await {
        Ok(q) => q,
        Err(e) => return e.into_response(),
    };
    if !is_safe_tag(&body.tag) {
        return ApiError::invalid_input("tag must be alphanumeric + `-_:./`, ≤128 chars")
            .into_response();
    }
    let id_part = body
        .id
        .trim_start_matches("knowledge:")
        .trim_matches('\u{27E8}')
        .trim_matches('\u{27E9}');
    if id_part.is_empty() || id_part.len() > 256 || !id_part.chars().all(is_record_id_char) {
        return ApiError::invalid_input("knowledge id has forbidden characters").into_response();
    }
    let sql = format!(
        "UPDATE `knowledge`:\u{27E8}{id}\u{27E9} SET tags += '{tag}' RETURN AFTER",
        id = id_part,
        tag = body.tag,
    );
    match query.raw_query(&sql).await {
        Ok(_) => axum::response::Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => ApiError::from_db_err(params.repo.as_deref().unwrap_or("?"), e).into_response(),
    }
}

fn is_record_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/')
}

fn build_suggestions(rows: &[serde_json::Value]) -> Vec<Suggestion> {
    let mut tag_bags: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut untagged: Vec<(String, String, String, String)> = Vec::new();

    for row in rows {
        let id = row
            .get("id")
            .and_then(value_to_id_string)
            .unwrap_or_default();
        let title = row
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let kind = row
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("knowledge")
            .to_string();
        let content = row
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tags: Vec<String> = row
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let words = meaningful_words(&format!("{title} {}", first_n_chars(&content, 400)));
        let topical: Vec<String> = tags.iter().filter(|t| !is_meta_tag(t)).cloned().collect();

        if topical.is_empty() {
            untagged.push((id, title, kind, content));
        }
        for tag in topical {
            let bag = tag_bags.entry(tag).or_default();
            for w in &words {
                bag.insert(w.clone());
            }
        }
    }

    let mut out = Vec::new();
    for (id, title, kind, content) in untagged {
        if id.is_empty() {
            continue;
        }
        let words: BTreeSet<String> =
            meaningful_words(&format!("{title} {}", first_n_chars(&content, 400)))
                .into_iter()
                .collect();
        if words.is_empty() {
            continue;
        }
        let mut candidates: Vec<SuggestionCandidate> = tag_bags
            .iter()
            .filter_map(|(tag, bag)| {
                let inter: Vec<String> = words.intersection(bag).cloned().collect();
                // Jaccard for the general topical overlap.
                let jaccard = if inter.is_empty() {
                    0.0
                } else {
                    let union = words.len() + bag.len() - inter.len();
                    inter.len() as f32 / union.max(1) as f32
                };
                // Tag-name-in-title / content bonus: if any word
                // form of the tag literally appears in the entry,
                // that's a near-certain signal. Split on `-_` so
                // `surreal-migration` matches "surreal" or
                // "migration" individually.
                let tag_words: Vec<String> = tag
                    .split(['-', '_'])
                    .map(|p| p.to_ascii_lowercase())
                    .filter(|p| p.len() >= 3)
                    .collect();
                let tag_hits = tag_words.iter().filter(|w| words.contains(*w)).count();
                let tag_bonus = if tag_hits == 0 {
                    0.0
                } else {
                    // 0.35 for a single tag-word match; 0.55 for two+.
                    0.15 + 0.2 * tag_hits.min(2) as f32
                };
                let score = jaccard.max(tag_bonus);
                if score < 0.12 {
                    return None;
                }
                Some(SuggestionCandidate {
                    tag: tag.clone(),
                    score,
                    matched_words: inter.into_iter().take(6).collect(),
                })
            })
            .collect();
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(3);
        if candidates.is_empty() {
            continue;
        }
        out.push(Suggestion {
            id,
            title,
            kind,
            candidates,
        });
    }
    out.sort_by(|a, b| {
        let sa = a.candidates.first().map(|c| c.score).unwrap_or(0.0);
        let sb = b.candidates.first().map(|c| c.score).unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(50);
    out
}

fn first_n_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn meaningful_words(text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "that", "this", "from", "have", "has", "was", "were", "but",
        "not", "all", "any", "are", "can", "out", "our", "new", "one", "two", "into", "over",
        "just", "when", "then", "what", "your", "you", "use", "used", "uses", "via", "also",
    ];
    let mut out = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            current.push(c.to_ascii_lowercase());
        } else {
            push_word(&mut out, &mut seen, &mut current, STOP);
        }
    }
    push_word(&mut out, &mut seen, &mut current, STOP);
    out
}

fn push_word(
    out: &mut Vec<String>,
    seen: &mut HashMap<String, ()>,
    current: &mut String,
    stop: &[&str],
) {
    if current.len() >= 3
        && !stop.contains(&current.as_str())
        && !seen.contains_key(current.as_str())
    {
        seen.insert(current.clone(), ());
        out.push(current.clone());
    }
    current.clear();
}

fn value_to_id_string(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if let Some(obj) = v.as_object() {
        let tb = obj
            .get("tb")
            .and_then(|v| v.as_str())
            .unwrap_or("knowledge");
        let id = obj.get("id")?;
        let inner = if let Some(s) = id.as_str() {
            s.to_string()
        } else if let Some(o) = id.as_object() {
            o.get("String")
                .and_then(|v| v.as_str())
                .map(str::to_string)?
        } else {
            return None;
        };
        return Some(format!("{tb}:{inner}"));
    }
    None
}

// ── Dream-C: cross-repo pattern detection ─────────────────────

#[derive(Serialize)]
pub struct Pattern {
    pub tag: String,
    pub title: String,
    pub repos: Vec<PatternRepoEntry>,
    pub total: usize,
}

#[derive(Serialize)]
pub struct PatternRepoEntry {
    pub repo: String,
    pub count: usize,
    pub example_title: String,
}

/// `GET /api/dream/patterns` — scan every repo on the surreal
/// server for knowledge tags that repeat across ≥2 projects.
/// The point is "you've solved this kind of thing before" — same
/// `auth`, `migration`, `caching`, etc. tag in three different
/// codebases is a strong signal worth surfacing.
///
/// Only returns topical tags (meta tags excluded). Limits each
/// repo query to the 500 most recent knowledge rows to bound the
/// cost; patterns that need more evidence than that are edge
/// cases.
pub async fn api_dream_patterns(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Need the server-wide repo list; only meaningful in daemon
    // (multi) mode, but we also try `list_server_repos` in single
    // mode — it falls back via connect_admin.
    let daemon = state.daemon_state().cloned();
    let repos: Vec<String> = if let Some(d) = &daemon {
        d.list_server_repos().await
    } else {
        // Single-mode fallback: query admin directly.
        match codescope_core::connect_admin().await {
            Ok(admin) => {
                let ns = std::env::var("CODESCOPE_DB_NS")
                    .unwrap_or_else(|_| codescope_core::DEFAULT_NS.to_string());
                let _ = admin.use_ns(&ns).await;
                admin
                    .query("INFO FOR NS")
                    .await
                    .ok()
                    .and_then(|mut r| r.take::<Vec<serde_json::Value>>(0).ok())
                    .map(|rows| {
                        rows.into_iter()
                            .filter_map(|row| {
                                let dbs = row
                                    .get("databases")
                                    .or_else(|| row.get("db"))?
                                    .as_object()?;
                                Some(dbs.keys().cloned().collect::<Vec<_>>())
                            })
                            .flatten()
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Err(_) => Vec::new(),
        }
    };
    let repos: Vec<String> = repos
        .into_iter()
        .filter(|n| !n.starts_with('_') && !n.contains(".old."))
        .collect();

    // `(tag, repo) → (count, example_title)`.
    let mut tally: BTreeMap<(String, String), (usize, String)> = BTreeMap::new();

    for repo in &repos {
        // Open a one-shot admin connection per repo — we don't
        // want to cache unrelated handles here, and the opens are
        // cheap against a live server.
        let Ok(db) = codescope_core::connect_repo(repo).await else {
            continue;
        };
        let gq = codescope_core::graph::query::GraphQuery::new(db);
        let Ok(value) = gq
            .raw_query("SELECT title, tags FROM knowledge ORDER BY created_at DESC LIMIT 500")
            .await
        else {
            continue;
        };
        let rows = flatten_rows(value);
        for row in rows {
            let title = row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tags: Vec<String> = row
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            for tag in tags {
                if is_meta_tag(&tag) {
                    continue;
                }
                let entry = tally
                    .entry((tag, repo.clone()))
                    .or_insert_with(|| (0, title.clone()));
                entry.0 += 1;
            }
        }
    }

    // Pivot: group per-tag; keep only tags spanning ≥2 repos.
    let mut by_tag: BTreeMap<String, Vec<PatternRepoEntry>> = BTreeMap::new();
    for ((tag, repo), (count, example)) in tally {
        by_tag.entry(tag).or_default().push(PatternRepoEntry {
            repo,
            count,
            example_title: example,
        });
    }
    let mut patterns: Vec<Pattern> = by_tag
        .into_iter()
        .filter(|(_, entries)| entries.len() >= 2)
        .map(|(tag, entries)| {
            let total = entries.iter().map(|e| e.count).sum();
            Pattern {
                title: humanise_tag(&tag),
                tag,
                total,
                repos: entries,
            }
        })
        .collect();

    // Highest-reach patterns first.
    patterns.sort_by(|a, b| {
        b.repos
            .len()
            .cmp(&a.repos.len())
            .then_with(|| b.total.cmp(&a.total))
    });
    patterns.truncate(50);
    axum::response::Json(serde_json::json!({ "patterns": patterns })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jaccard_suggests_matching_arc() {
        let rows = vec![
            json!({
                "id": "knowledge:a",
                "kind": "decision",
                "title": "Autocalib preset table for model-specific dispatch",
                "content": "Shipped 68 tok/s standard path.",
                "tags": ["autocalib", "status:done"]
            }),
            json!({
                "id": "knowledge:b",
                "kind": "decision",
                "title": "Autocalib autotune CLI",
                "content": "tq autotune command with JSON cache.",
                "tags": ["autocalib", "autotune", "status:done"]
            }),
            json!({
                "id": "knowledge:c",
                "kind": "claim",
                "title": "Autocalib phase 3 pending",
                "content": "arch and hidden_dim cache key planned.",
                "tags": ["status:planned"]
            }),
        ];
        let suggestions = build_suggestions(&rows);
        assert!(!suggestions.is_empty(), "should produce suggestions");
        let s = &suggestions[0];
        assert!(s.candidates.iter().any(|c| c.tag == "autocalib"));
    }

    #[test]
    fn no_suggestion_when_untagged_shares_nothing() {
        let rows = vec![
            json!({
                "id": "knowledge:x",
                "kind": "claim",
                "title": "Unrelated note about coffee",
                "content": "Morning habit.",
                "tags": []
            }),
            json!({
                "id": "knowledge:y",
                "kind": "decision",
                "title": "Database migration",
                "content": "Switched from SQLite to Surreal.",
                "tags": ["database", "surreal-migration"]
            }),
        ];
        let suggestions = build_suggestions(&rows);
        assert!(suggestions.is_empty(), "no overlap → no suggestion");
    }
}

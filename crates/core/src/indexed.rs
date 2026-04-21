//! CMX-absorb: generic indexed-content store.
//!
//! Stores arbitrary text (web fetches, log captures, doc dumps)
//! in the `indexed_content` SurrealDB table with a FULLTEXT BM25
//! index. The LLM searches via [`search`]; the user (or a
//! background task) writes via [`store`] / [`fetch_and_store`].
//!
//! Distinct from the `knowledge` table on purpose — `knowledge`
//! carries structured `kind` (decision / problem / solution …)
//! that downstream tools group on. Indexed content is dumb text:
//! one `kind: "web" | "log" | "doc"` for filtering, no
//! taxonomy. Keeping them separate stops e.g. `knowledge_search`
//! from returning a 500 KB log dump as a "decision".

use crate::DbHandle;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// One row in `indexed_content`. `id` is `None` on inserts; the
/// server fills it.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct IndexedItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    pub title: String,
    pub body: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed_at: Option<String>,
}

/// Store one item. Re-stores against the same `source` UPSERT —
/// the SurrealDB `idxc_source` UNIQUE index handles dedupe.
pub async fn store(db: &DbHandle, item: IndexedItem) -> Result<()> {
    let body = item.body;
    let title = item.title;
    let source = item.source;
    let kind = item.kind.unwrap_or_else(|| "doc".to_string());
    let tags = item.tags.unwrap_or_default();
    let size = body.len() as i64;
    let now = now_iso();
    // UPSERT by source so re-indexing replaces in place.
    let sql = "
        UPSERT indexed_content SET
            title = $title,
            body = $body,
            source = $source,
            kind = $kind,
            tags = $tags,
            size_bytes = $size,
            indexed_at = $indexed_at
        WHERE source = $source
    ";
    db.query(sql)
        .bind(("title", title))
        .bind(("body", body))
        .bind(("source", source))
        .bind(("kind", kind))
        .bind(("tags", tags))
        .bind(("size", size))
        .bind(("indexed_at", now))
        .await
        .context("upsert indexed_content")?;
    Ok(())
}

/// Fetch a URL or local file and store its extracted text. HTML
/// is rendered to plain text via `html2text` (good for docs,
/// blog posts; not great for SPA-only sites — those need a real
/// headless browser, out of scope).
pub async fn fetch_and_store(
    db: &DbHandle,
    source: &str,
    title_override: Option<&str>,
    tags: Vec<String>,
) -> Result<IndexedItem> {
    let (raw, kind, derived_title) =
        if source.starts_with("http://") || source.starts_with("https://") {
            let body = reqwest::Client::builder()
                .user_agent(format!(
                    "codescope/{} fetch_and_index",
                    env!("CARGO_PKG_VERSION")
                ))
                .build()?
                .get(source)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            let kind = if looks_like_html(&body) { "web" } else { "doc" };
            let title = if kind == "web" {
                extract_html_title(&body)
            } else {
                None
            };
            (body, kind.to_string(), title)
        } else {
            let body = std::fs::read_to_string(source)
                .with_context(|| format!("read local file {source}"))?;
            let title = std::path::Path::new(source)
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string);
            (body, "doc".to_string(), title)
        };

    let body = if kind == "web" {
        // Prefer the article / main / content subtree when present
        // — docs and blogs almost always wrap their real content in
        // one of these, and feeding html2text the whole document
        // pulls in nav / footer / cookie-banner prose that dilutes
        // BM25 scoring. When the selectors don't match (SPAs that
        // render the shell but no content, or pages that skip
        // semantic tags), fall back to the full body.
        let reduced = extract_article_body(&raw).unwrap_or(raw.clone());
        html2text::from_read(reduced.as_bytes(), 100).unwrap_or_else(|_| raw.clone())
    } else {
        raw
    };
    let title = title_override
        .map(str::to_string)
        .or(derived_title)
        .unwrap_or_else(|| source.to_string());

    let item = IndexedItem {
        id: None,
        title: title.clone(),
        body: body.clone(),
        source: source.to_string(),
        kind: Some(kind.clone()),
        tags: Some(tags.clone()),
        size_bytes: Some(body.len() as i64),
        indexed_at: Some(now_iso()),
    };
    store(db, item.clone()).await?;
    Ok(item)
}

/// BM25 search across title + body. Returns hits with a snippet
/// excerpt around the strongest match (we ask SurrealDB for it
/// via `search::highlight`).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub source: String,
    pub kind: Option<String>,
    pub snippet: String,
    pub score: f32,
}

pub async fn search(db: &DbHandle, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let lim = limit.clamp(1, 50);
    // SurrealDB 3.0.5 quirk: `search::score()` returns 0.0 even with a
    // valid FULLTEXT BM25 index + matching WHERE predicate (verified
    // via `INFO FOR TABLE` + direct probe). Until upstream ships real
    // scores, keep the FULLTEXT index for the WHERE speedup and
    // compute a simple "title match + body match" score manually:
    // title hits weight 2.0, body hits weight 1.0. Substring fallback
    // (`string::contains`) lets a query on an analyzer-rejected token
    // (URL fragments, punctuation) still rank, at the cost of a
    // linear scan on the filtered subset.
    let sql = format!(
        "SELECT title, source, kind,
            string::slice(body, 0, 280) AS snippet,
            (IF string::contains(string::lowercase(title), string::lowercase($q)) THEN 2.0 ELSE 0.0 END) +
            (IF string::contains(string::lowercase(body),  string::lowercase($q)) THEN 1.0 ELSE 0.0 END) AS score
         FROM indexed_content
         WHERE body @1@ $q
            OR title @2@ $q
            OR string::contains(string::lowercase(body),  string::lowercase($q))
            OR string::contains(string::lowercase(title), string::lowercase($q))
         ORDER BY score DESC, title
         LIMIT {lim}"
    );
    let mut resp = db.query(sql).bind(("q", query.to_string())).await?;
    let rows: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(SearchHit {
            title: row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source: row
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            kind: row.get("kind").and_then(|v| v.as_str()).map(str::to_string),
            snippet: row
                .get("snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            score: row.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
        });
    }
    Ok(out)
}

/// Drop everything from the indexed_content table for the
/// active DB. The `knowledge` table is untouched — that's
/// curated state; this one is recoverable by re-fetching.
pub async fn purge(db: &DbHandle) -> Result<u64> {
    let mut resp = db
        .query("DELETE indexed_content RETURN BEFORE")
        .await
        .context("purge indexed_content")?;
    let rows: Vec<serde_json::Value> = resp.take(0).unwrap_or_default();
    Ok(rows.len() as u64)
}

fn looks_like_html(text: &str) -> bool {
    let head = &text[..text.len().min(1024)].to_ascii_lowercase();
    head.contains("<html") || head.contains("<!doctype html") || head.contains("<body")
}

/// Try to pluck the main article body out of an HTML document.
/// Scans for `<article>`, `<main>`, and common content-container
/// class names in that order; returns the first matching subtree
/// as an HTML fragment. Returns `None` if nothing matches so the
/// caller can fall back to the full body.
///
/// Intentionally string-based — a proper DOM parse is overkill
/// for this heuristic and would pull in `html5ever` transitively
/// on the hot path. Not robust against pathological nesting
/// (an `<article>` containing another `<article>` will keep
/// the outer one intact, which is what we want here anyway).
fn extract_article_body(html: &str) -> Option<String> {
    for (open, close) in [
        ("<article", "</article>"),
        ("<main", "</main>"),
        ("<div class=\"content\"", "</div>"),
        ("<div id=\"content\"", "</div>"),
    ] {
        if let Some(snippet) = slice_tag(html, open, close) {
            return Some(snippet);
        }
    }
    None
}

fn slice_tag(html: &str, open_prefix: &str, close_tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open_lc = open_prefix.to_ascii_lowercase();
    let close_lc = close_tag.to_ascii_lowercase();
    let start = lower.find(&open_lc)?;
    // Skip to the closing `>` of the open tag so attributes (class,
    // id, data-*) don't trip the length calculation.
    let after_open = html[start..].find('>').map(|p| start + p + 1)?;
    let end = lower[after_open..]
        .find(&close_lc)
        .map(|p| after_open + p + close_lc.len())?;
    Some(html[start..end].to_string())
}

/// Pull `<title>...</title>` out of an HTML blob without a full
/// parser — good enough for the title field.
fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let after_open = html[start..].find('>').map(|p| start + p + 1)?;
    let end = lower[after_open..]
        .find("</title>")
        .map(|p| after_open + p)?;
    let raw = html[after_open..end].trim();
    if raw.is_empty() {
        None
    } else {
        Some(decode_minimal_entities(raw))
    }
}

/// Tiny entity decoder for the title field — we only care about
/// the four most common ones. Full entity decoding would pull a
/// dep we don't need.
fn decode_minimal_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_html_title() {
        let html = r#"<html><head><title>Hello &amp; World</title></head></html>"#;
        assert_eq!(extract_html_title(html).as_deref(), Some("Hello & World"));
    }

    #[test]
    fn looks_like_html_detects_doctype() {
        assert!(looks_like_html("<!DOCTYPE html><html></html>"));
        assert!(looks_like_html("<html><body></body></html>"));
        assert!(!looks_like_html("plain text content"));
    }

    #[test]
    fn article_extract_prefers_article_tag() {
        let html = r#"<html><body><nav>menu</nav><article>real content</article><footer>junk</footer></body></html>"#;
        let got = extract_article_body(html).unwrap();
        assert!(got.contains("real content"));
        assert!(!got.contains("menu"));
        assert!(!got.contains("junk"));
    }

    #[test]
    fn article_extract_falls_back_to_main() {
        let html = r#"<html><body><nav>menu</nav><main>main content</main></body></html>"#;
        let got = extract_article_body(html).unwrap();
        assert!(got.contains("main content"));
        assert!(!got.contains("menu"));
    }

    #[test]
    fn article_extract_respects_attributes_on_open_tag() {
        let html =
            r#"<html><body><article class="post" data-id="42">content</article></body></html>"#;
        let got = extract_article_body(html).unwrap();
        assert!(got.contains("content"));
    }

    #[test]
    fn article_extract_none_when_no_match() {
        let html = r#"<html><body><nav>menu</nav><p>paragraph</p></body></html>"#;
        assert!(extract_article_body(html).is_none());
    }
}

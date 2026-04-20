//! CMX-INDEX MCP tools — fetch_and_index, index_content,
//! search_indexed. Backed by `core::indexed`.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;

use crate::params::*;
use crate::server::GraphRagServer;

#[tool_router(router = indexed_router, vis = "pub(crate)")]
impl GraphRagServer {
    /// Fetch a URL or read a local file, extract text, index it
    /// in the `indexed_content` table for later BM25 search.
    /// Re-fetching the same source replaces (no duplicates).
    #[tool(
        description = "Fetch a URL or read a local file, extract text, store in indexed_content for BM25 retrieval."
    )]
    async fn fetch_and_index(&self, Parameters(params): Parameters<FetchAndIndexParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        match codescope_core::indexed::fetch_and_store(
            &ctx.db,
            &params.source,
            params.title.as_deref(),
            params.tags.unwrap_or_default(),
        )
        .await
        {
            Ok(item) => serde_json::to_string(&serde_json::json!({
                "ok": true,
                "title": item.title,
                "source": item.source,
                "kind": item.kind,
                "size_bytes": item.size_bytes,
            }))
            .unwrap_or_else(|_| "{}".into()),
            Err(e) => crate::error::tool_error(
                crate::error::code::INTERNAL,
                &format!("fetch_and_index failed: {e}"),
                Some("Check the URL/path or run with `codescope ingest <source>` for a verbose CLI error."),
            ),
        }
    }

    /// Store an arbitrary text blob in indexed_content. Useful
    /// for paste-buffer style ingestion where you already have
    /// the body in hand.
    #[tool(
        description = "Store a text blob in indexed_content. Pass title, body, and a stable source string for dedupe."
    )]
    async fn index_content(&self, Parameters(params): Parameters<IndexContentParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let item = codescope_core::indexed::IndexedItem {
            id: None,
            title: params.title,
            body: params.body,
            source: params.source,
            kind: params.kind,
            tags: params.tags,
            size_bytes: None,
            indexed_at: None,
        };
        match codescope_core::indexed::store(&ctx.db, item).await {
            Ok(()) => serde_json::json!({ "ok": true }).to_string(),
            Err(e) => crate::error::tool_error(
                crate::error::code::INTERNAL,
                &format!("index_content failed: {e}"),
                None,
            ),
        }
    }

    /// BM25 search over indexed_content. Returns title, source,
    /// 280-char snippet, and a score per hit.
    #[tool(
        description = "BM25 search over indexed_content (web fetches, log dumps, doc snapshots). Returns snippet + source per hit."
    )]
    async fn search_indexed(&self, Parameters(params): Parameters<SearchIndexedParams>) -> String {
        let ctx = match self.ctx().await {
            Ok(c) => c,
            Err(e) => return e,
        };
        let limit = params.limit.unwrap_or(10);
        match codescope_core::indexed::search(&ctx.db, &params.query, limit).await {
            Ok(hits) => serde_json::to_string(&serde_json::json!({ "hits": hits }))
                .unwrap_or_else(|_| "{\"hits\":[]}".into()),
            Err(e) => crate::error::tool_error(
                crate::error::code::INTERNAL,
                &format!("search_indexed failed: {e}"),
                None,
            ),
        }
    }
}

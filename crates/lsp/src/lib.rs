//! Codescope LSP — exposes the codescope knowledge graph via the Language
//! Server Protocol so any LSP-capable editor (VS Code, Zed, Neovim, Helix)
//! can use codescope as a graph-backed language server.
//!
//! Scope of this first draft (intentionally minimal):
//!   * initialize           — advertise capabilities
//!   * goto_definition      — graph-backed via find_function
//!   * hover                — markdown snippet for the function at cursor
//!   * workspace_symbol     — search_functions against the query
//!   * references / document_symbol — stubbed (return empty)
//!
//! The LSP connects to the per-repo SurrealKv database at
//! `~/.codescope/db/<repo_name>/` (same convention as the CLI and MCP server).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use codescope_core::graph::query::GraphQuery;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Backend {
    client: Client,
    state: Arc<RwLock<BackendState>>,
}

#[derive(Default)]
struct BackendState {
    /// Inferred repo name (directory name of the workspace root).
    repo: Option<String>,
    /// Workspace root folder, kept so we can resolve file paths.
    workspace_root: Option<PathBuf>,
    /// Graph query handle (connected on `initialized`).
    gq: Option<Arc<GraphQuery>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(BackendState::default())),
        }
    }

    /// Open a GraphQuery handle for `repo` via the shared surreal server.
    async fn connect_db(&self, repo: &str) -> anyhow::Result<()> {
        let db = codescope_core::connect_repo(repo).await?;
        let gq = Arc::new(GraphQuery::new(db));
        let mut st = self.state.write().await;
        st.gq = Some(gq);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// LanguageServer impl
// ---------------------------------------------------------------------------

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        // Prefer workspace_folders, then (deprecated) root_uri, then root_path.
        let root: Option<PathBuf> = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|f| uri_to_path(&f.uri))
            .or_else(|| {
                #[allow(deprecated)]
                params.root_uri.as_ref().and_then(uri_to_path)
            })
            .or_else(|| {
                #[allow(deprecated)]
                params.root_path.as_ref().map(PathBuf::from)
            });

        let repo = root
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".into());

        {
            let mut st = self.state.write().await;
            st.repo = Some(repo);
            st.workspace_root = root;
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // LSP spec defaults to UTF-16 when a client doesn't negotiate.
                // We advertise UTF-16 explicitly so clients know what we
                // expect for `Position::character` offsets.
                position_encoding: Some(PositionEncodingKind::UTF16),
                // We advertise FULL sync (rather than INCREMENTAL) because we
                // don't maintain an in-memory text buffer — `did_change` only
                // logs for now, and `word_at_position_from_uri` reads from
                // disk. FULL is the honest match for that behavior.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "codescope-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let repo = {
            let st = self.state.read().await;
            st.repo.clone().unwrap_or_else(|| "default".into())
        };

        match self.connect_db(&repo).await {
            Ok(()) => {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("codescope-lsp: connected to graph for repo '{}'", repo),
                    )
                    .await;
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!(
                            "codescope-lsp: failed to open graph DB for '{}': {}",
                            repo, e
                        ),
                    )
                    .await;
            }
        }
    }

    async fn shutdown(&self) -> LspResult<()> {
        let mut st = self.state.write().await;
        st.gq = None;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // goto_definition — graph-backed via find_function
    // -----------------------------------------------------------------------
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let Some(word) = word_at_position_from_uri(&uri, pos) else {
            return Ok(None);
        };

        let gq = match self.graph().await {
            Some(g) => g,
            None => return Ok(None),
        };

        let results = gq.find_function(&word).await.unwrap_or_default();
        let ws_root = {
            let st = self.state.read().await;
            st.workspace_root.clone()
        };

        let locations: Vec<Location> = results
            .into_iter()
            .filter_map(|r| {
                let file = r.file_path?;
                let line = r.start_line.unwrap_or(0);
                let end = r.end_line.unwrap_or(line);
                location_for_entity(ws_root.as_deref(), &file, line, end)
            })
            .collect();

        if locations.is_empty() {
            Ok(None)
        } else if locations.len() == 1 {
            Ok(Some(GotoDefinitionResponse::Scalar(
                locations.into_iter().next().unwrap(),
            )))
        } else {
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
    }

    // -----------------------------------------------------------------------
    // references — graph-backed via find_callers
    // -----------------------------------------------------------------------
    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let Some(word) = word_at_position_from_uri(&uri, pos) else {
            return Ok(None);
        };

        let gq = match self.graph().await {
            Some(g) => g,
            None => return Ok(None),
        };

        let results = gq.find_callers(&word).await.unwrap_or_default();
        let ws_root = {
            let st = self.state.read().await;
            st.workspace_root.clone()
        };

        let mut locations: Vec<Location> = Vec::new();

        // Per the LSP spec, when `context.include_declaration` is true the
        // response should include the declaration site(s) alongside the
        // callers.
        if include_declaration {
            let defs = gq.find_function(&word).await.unwrap_or_default();
            for r in defs {
                if let Some(file) = r.file_path {
                    let line = r.start_line.unwrap_or(0);
                    let end = r.end_line.unwrap_or(line);
                    if let Some(loc) = location_for_entity(ws_root.as_deref(), &file, line, end) {
                        locations.push(loc);
                    }
                }
            }
        }

        for r in results {
            if let Some(file) = r.file_path {
                let line = r.start_line.unwrap_or(0);
                let end = r.end_line.unwrap_or(line);
                if let Some(loc) = location_for_entity(ws_root.as_deref(), &file, line, end) {
                    locations.push(loc);
                }
            }
        }

        Ok(Some(locations))
    }

    // -----------------------------------------------------------------------
    // hover — markdown for the function at cursor
    // -----------------------------------------------------------------------
    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let Some(word) = word_at_position_from_uri(&uri, pos) else {
            return Ok(None);
        };

        let gq = match self.graph().await {
            Some(g) => g,
            None => return Ok(None),
        };

        let results = gq.find_function(&word).await.unwrap_or_default();
        if results.is_empty() {
            return Ok(None);
        }

        // Disambiguate when multiple entities share a name: prefer the one
        // whose [start_line, end_line] range contains the cursor line. LSP
        // positions are 0-based; codescope stores 1-based lines, so add 1
        // before comparing. Fall back to the first match if nothing overlaps
        // (e.g. cursor is on a declaration line outside any stored range).
        let cursor_line_1based = pos.line.saturating_add(1);
        let best_idx = results
            .iter()
            .position(|e| match (e.start_line, e.end_line) {
                (Some(s), Some(end)) => cursor_line_1based >= s && cursor_line_1based <= end,
                (Some(s), None) => cursor_line_1based == s,
                _ => false,
            })
            .unwrap_or(0);
        let r = results.into_iter().nth(best_idx).unwrap();

        let mut md = String::new();
        md.push_str(&format!("**{}**", r.name.as_deref().unwrap_or(&word)));
        if let Some(q) = &r.qualified_name {
            md.push_str(&format!("  \n`{}`", q));
        }
        if let Some(sig) = &r.signature {
            let lang = r.language.as_deref().unwrap_or("");
            md.push_str(&format!("\n\n```{}\n{}\n```", lang, sig));
        }
        if let (Some(file), Some(line)) = (&r.file_path, r.start_line) {
            md.push_str(&format!("\n\n_{}:{}_", file, line));
        }

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: None,
        }))
    }

    // -----------------------------------------------------------------------
    // workspace_symbol — graph-backed via search_functions
    // -----------------------------------------------------------------------
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<SymbolInformation>>> {
        let gq = match self.graph().await {
            Some(g) => g,
            None => return Ok(None),
        };

        let query = params.query;
        let results = if query.is_empty() {
            Vec::new()
        } else {
            gq.search_functions(&query).await.unwrap_or_default()
        };

        let ws_root = {
            let st = self.state.read().await;
            st.workspace_root.clone()
        };

        let symbols: Vec<SymbolInformation> = results
            .into_iter()
            .filter_map(|r| {
                let file = r.file_path.clone()?;
                let line = r.start_line.unwrap_or(0);
                let end = r.end_line.unwrap_or(line);
                let loc = location_for_entity(ws_root.as_deref(), &file, line, end)?;
                // `search_functions` only queries the `function` table, so
                // every hit here is a function/method.
                #[allow(deprecated)]
                Some(SymbolInformation {
                    name: r.name.unwrap_or_else(|| "?".into()),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    location: loc,
                    container_name: r.qualified_name,
                })
            })
            .collect();

        Ok(Some(symbols))
    }

    // -----------------------------------------------------------------------
    // document_symbol — graph-backed via file_entities
    // -----------------------------------------------------------------------
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let gq = match self.graph().await {
            Some(g) => g,
            None => return Ok(None),
        };

        let Some(path) = uri_to_path(&params.text_document.uri) else {
            return Ok(None);
        };

        // The indexer may have stored either absolute or workspace-relative
        // paths depending on how the repo was indexed. Try both so we don't
        // return empty just because of a path mismatch.
        let ws_root = {
            let st = self.state.read().await;
            st.workspace_root.clone()
        };

        let abs = path.to_string_lossy().to_string();
        let mut results = gq.file_entities(&abs).await.unwrap_or_default();
        if results.is_empty() {
            if let Some(root) = ws_root.as_ref() {
                if let Ok(rel) = path.strip_prefix(root) {
                    let rel_s = rel.to_string_lossy().replace('\\', "/");
                    results = gq.file_entities(&rel_s).await.unwrap_or_default();
                }
            }
        }

        let symbols: Vec<SymbolInformation> = results
            .into_iter()
            .filter_map(|r| {
                let file = r.file_path.clone()?;
                let line = r.start_line.unwrap_or(0);
                let end = r.end_line.unwrap_or(line);
                let loc = location_for_entity(ws_root.as_deref(), &file, line, end)?;
                #[allow(deprecated)]
                Some(SymbolInformation {
                    name: r.name.unwrap_or_else(|| "?".into()),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    location: loc,
                    container_name: r.qualified_name,
                })
            })
            .collect();

        Ok(Some(DocumentSymbolResponse::Flat(symbols)))
    }
}

impl Backend {
    async fn graph(&self) -> Option<Arc<GraphQuery>> {
        self.state.read().await.gq.clone()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `~/.codescope/db/<repo>/` — same convention as the CLI and MCP server.
/// Kept on the shelf for the legacy SurrealKV-path flow; callers have
/// migrated to `connect_repo` which goes through the bundled server,
/// so this exists only for the remote-filesystem-override diagnostic.
#[allow(dead_code)]
fn default_db_path(repo: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescope")
        .join("db")
        .join(repo)
}

/// Best-effort conversion from a file:// URI to a local path.
fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    if uri.scheme() == "file" {
        uri.to_file_path().ok()
    } else {
        None
    }
}

/// Build an LSP `Location` for an indexed entity. `file` may be absolute or
/// workspace-relative; if relative, `ws_root` is used to resolve it.
fn location_for_entity(
    ws_root: Option<&Path>,
    file: &str,
    start_line: u32,
    end_line: u32,
) -> Option<Location> {
    let path = PathBuf::from(file);
    let abs = if path.is_absolute() {
        path
    } else if let Some(root) = ws_root {
        root.join(&path)
    } else {
        path
    };

    let uri = Url::from_file_path(&abs).ok()?;

    // LSP positions are zero-based; codescope stores 1-based line numbers.
    let start_l = start_line.saturating_sub(1);
    let end_l = end_line.saturating_sub(1).max(start_l);

    Some(Location {
        uri,
        range: Range {
            start: Position {
                line: start_l,
                character: 0,
            },
            end: Position {
                line: end_l,
                character: 0,
            },
        },
    })
}

/// Extract the identifier at `pos` within `text`.
///
/// Accepts any Unicode alphanumeric character plus `_` — this matches what
/// Rust, Python, and JavaScript all allow in identifiers (they allow more,
/// but `is_alphanumeric || _` is a safe superset for "looks like a word").
///
/// `pos.character` is interpreted as a **UTF-16 code unit offset** per the
/// LSP spec default (and what we advertise via `positionEncoding`). We
/// translate that to a char index by accumulating `char::len_utf16()` per
/// character until we hit the target offset. Characters outside the BMP
/// (e.g. many emoji, some CJK Extension B) count as 2 UTF-16 units.
///
/// Returns None if the cursor isn't on an identifier character (and isn't
/// immediately after one).
pub fn word_at_position(text: &str, pos: Position) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    let target_utf16 = pos.character as usize;

    let is_ident = |c: char| c.is_alphanumeric() || c == '_';

    // Collect the line's chars so we can walk in both directions and slice
    // by char index. Lines are typically short, so this is cheap.
    let chars: Vec<char> = line.chars().collect();

    // Convert UTF-16 code unit offset → char index.
    // If target_utf16 lands in the middle of a surrogate pair we snap to the
    // preceding char (conservative; cursor can't really be mid-surrogate).
    let line_utf16_len: usize = chars.iter().map(|c| c.len_utf16()).sum();
    if target_utf16 > line_utf16_len {
        // Past end of line → invalid.
        return None;
    }
    let mut char_idx = chars.len(); // default: end-of-line
    let mut acc = 0usize;
    for (i, c) in chars.iter().enumerate() {
        if acc >= target_utf16 {
            char_idx = i;
            break;
        }
        acc += c.len_utf16();
    }

    // Walk left over identifier characters.
    let mut start = char_idx;
    while start > 0 && is_ident(chars[start - 1]) {
        start -= 1;
    }
    // Walk right over identifier characters.
    let mut end = char_idx;
    while end < chars.len() && is_ident(chars[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }

    let ident: String = chars[start..end].iter().collect();
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

/// Read the file pointed to by `uri` and return the identifier at `pos`.
/// The LSP is stateless here — we read from disk instead of keeping a text
/// cache. Good enough for a first draft; editors always save before "go to
/// definition" in practice anyway.
fn word_at_position_from_uri(uri: &Url, pos: Position) -> Option<String> {
    let path = uri_to_path(uri)?;
    let text = std::fs::read_to_string(path).ok()?;
    word_at_position(&text, pos)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_at_position_extracts_identifier() {
        let text = "fn hello_world() {}\n";
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 5,
            },
        );
        assert_eq!(got.as_deref(), Some("hello_world"));
    }

    #[test]
    fn word_at_position_returns_none_on_whitespace() {
        let text = "  \n";
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 1,
            },
        );
        assert_eq!(got, None);
    }

    #[test]
    fn word_at_position_at_boundary() {
        // Cursor right after the identifier still captures it.
        let text = "foo bar";
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 3,
            },
        );
        assert_eq!(got.as_deref(), Some("foo"));
    }

    // ----- non-ASCII identifiers -----

    #[test]
    fn word_at_position_accented_python_identifier() {
        // Python allows Unicode identifiers. "données" = 7 chars, all BMP,
        // so UTF-16 offset == char offset.
        let text = "def données():";
        // Cursor in the middle of "données" (after 'd').
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 5,
            },
        );
        assert_eq!(got.as_deref(), Some("données"));
    }

    #[test]
    fn word_at_position_cjk_identifier() {
        // JS/TS allow CJK identifiers. "変数" is 2 BMP chars, 2 UTF-16 units.
        let text = "let 変数 = 1;";
        // Cursor on the first CJK char.
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 4,
            },
        );
        assert_eq!(got.as_deref(), Some("変数"));

        // Cursor immediately AFTER the identifier (2 UTF-16 units in).
        let got2 = word_at_position(
            text,
            Position {
                line: 0,
                character: 6,
            },
        );
        assert_eq!(got2.as_deref(), Some("変数"));
    }

    #[test]
    fn word_at_position_at_start_of_accented_identifier() {
        let text = "foo été bar";
        // Cursor right at the 'é' (UTF-16 offset 4, after "foo ").
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 4,
            },
        );
        assert_eq!(got.as_deref(), Some("été"));
    }

    #[test]
    fn word_at_position_at_end_of_accented_identifier() {
        let text = "foo été bar";
        // "foo " (4) + "été" (3 chars, 3 UTF-16 units) = 7.
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 7,
            },
        );
        assert_eq!(got.as_deref(), Some("été"));
    }

    #[test]
    fn word_at_position_middle_of_accented_identifier() {
        let text = "let naïve = 1;";
        // "let " (4) + "na" (2) = 6 → cursor between 'a' and 'ï'.
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 6,
            },
        );
        assert_eq!(got.as_deref(), Some("naïve"));
    }

    #[test]
    fn word_at_position_non_bmp_char_uses_two_utf16_units() {
        // 🦀 (U+1F980) is a non-BMP char — 2 UTF-16 code units, 1 char.
        // Emoji aren't typically valid identifier chars in most languages,
        // but we need to verify the UTF-16 offset math handles surrogate
        // pairs in preceding text correctly.
        //
        // Layout: "🦀 foo"
        //   UTF-16 units: 🦀=2, space=1, f=1, o=1, o=1 → total 6
        //   Chars:        🦀=1, space=1, f=1, o=1, o=1 → total 5
        //
        // A cursor at UTF-16 offset 3 is right before 'f' (after "🦀 "),
        // which would have been offset 2 under a char-based count. The
        // returned identifier should be "foo" regardless.
        let text = "🦀 foo";
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 3,
            },
        );
        assert_eq!(got.as_deref(), Some("foo"));

        // Sanity check: a naive char-based implementation would have placed
        // the cursor at UTF-16 offset 3 inside "foo" (char idx 3 = 'o'),
        // which would also return "foo" — so also test offset 4 which a
        // byte-based impl would place mid-"foo" but UTF-16-correct impl
        // places between 'f' and 'o'.
        let got2 = word_at_position(
            text,
            Position {
                line: 0,
                character: 4,
            },
        );
        assert_eq!(got2.as_deref(), Some("foo"));
    }

    #[test]
    fn word_at_position_past_end_of_line_returns_none() {
        let text = "foo";
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 100,
            },
        );
        assert_eq!(got, None);
    }

    #[test]
    fn word_at_position_at_exact_end_of_line() {
        // Cursor at the newline position still captures the trailing word.
        let text = "hello";
        let got = word_at_position(
            text,
            Position {
                line: 0,
                character: 5,
            },
        );
        assert_eq!(got.as_deref(), Some("hello"));
    }
}

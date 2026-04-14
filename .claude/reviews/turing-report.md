# Turing's LSP Audit — 2026-04-14

Scope: `crates/lsp/src/lib.rs` (544 LOC, single file). Review only — no code changes.

## Method coverage

| Method              | Status          | Notes |
|---------------------|-----------------|-------|
| `initialize`        | Implemented     | Advertises definition/references/hover/workspace_symbol/document_symbol. Text sync = INCREMENTAL but we hold no doc cache, so the sync kind is advertised cosmetically only. |
| `initialized`       | Implemented     | Connects to `~/.codescope/db/<repo>/`. Logs success/failure via `client.log_message` — does not crash. |
| `goto_definition`   | Implemented     | Backed by `GraphQuery::find_function(word)`. Returns `Scalar` for single hit, `Array` for multi. `None` when empty. |
| `references`        | Implemented     | Backed by `GraphQuery::find_callers(word)`. Returns `Some(vec![])` on empty (see red flag #1). Note: LSP contract says references should include the declaration when `context.include_declaration` is true — not honored. |
| `hover`             | Implemented     | Markdown card: name, qualified_name, signature fenced by language, file:line footer. Only uses `.into_iter().next()` — arbitrary pick if multiple matches exist. |
| `workspace_symbol`  | Implemented (`symbol`) | Empty query short-circuits to empty vec. Backed by `search_functions`. |
| `document_symbol`   | Implemented     | Tries absolute path first, then strips `ws_root` prefix and retries with forward-slash normalized path. Good defense against abs/rel mismatch. |
| `shutdown`          | Implemented     | Drops the `GraphQuery` handle. Does not flush/close DB explicitly — relies on `Surreal<Db>` drop. |
| `rename`            | Not implemented | Called out as future work in the agent brief. Capability not advertised, so editors won't offer it. |
| `did_open/did_change/did_save` | Not implemented | INCREMENTAL sync is advertised but no handler exists → tower-lsp will no-op. Not harmful given the stateless design, but misleading. |

## Correctness checks

- Position mapping: ⚠️ partial
  - ASCII identifiers: ✅ correct (`is_ascii_alphanumeric() || b'_'`).
  - Unicode identifiers: ❌ broken. The walker operates on `line.as_bytes()`. A multi-byte UTF-8 identifier (e.g. `λ`, `résumé`, Turkish `ığü`) will: (a) fail `is_ascii_alphanumeric`, truncating the word; (b) if the cursor lands mid-codepoint, `std::str::from_utf8` can return `Err` and the whole lookup silently returns `None`. Rust/Python/JS all allow non-ASCII identifiers — this is a real miss.
  - Column is treated as a byte offset, but LSP `Position::character` is defined as **UTF-16 code units** (per spec, absent a negotiated `positionEncoding`). For pure-ASCII files this happens to work because byte = char = UTF-16 unit. For any file with non-ASCII chars on the same line before the cursor, the offset is wrong.
  - `bytes[col]` / `bytes[start-1]`: bounds are guarded (`col > bytes.len()` returns None, walks check `start > 0` and `end < bytes.len()`), so no panic risk.

- Line number conversion: ✅
  - `location_for_entity` converts codescope's 1-based to LSP's 0-based via `saturating_sub(1)`. `end_l = end_l.max(start_l)` prevents inverted ranges when `end_line < start_line` (can happen if `end_line` is missing and defaults to 0 while `start_line` ≥ 1 — saturating_sub then sets end=0, start≥0, and max restores invariant). Correct.
  - Character column is always set to 0 for both start and end. That's a range of "whole line" — acceptable for GoTo but loses precision for definition-peek UIs. Not a bug, just coarse.

- Path handling: ✅ mostly
  - Uses `Url::from_file_path` / `Url::to_file_path` — correct per the agent brief ("don't hand-roll Windows escaping").
  - `document_symbol` normalizes `\` → `/` when falling back to a workspace-relative lookup. Good.
  - `location_for_entity` joins `ws_root.join(&path)` for relative paths. Standard library handles the separator. ✅
  - One subtle issue: `path.to_string_lossy().to_string()` on Windows yields `C:\Users\...\foo.rs`. If the indexer stored normalized `C:/Users/.../foo.rs`, the absolute-path lookup in `document_symbol` misses on the first attempt and falls back to the relative form. Works, but fragile — depends on indexer normalization.

- Empty DB graceful: ✅
  - `initialized` logs on failure instead of panicking; `self.state.gq` stays `None`.
  - Every handler pattern-matches on `self.graph().await`, returning `Ok(None)` or `Ok(Some(vec![]))` when no graph is attached. No unwrap/expect on the DB handle.
  - `gq.find_function(..)` / `find_callers(..)` / `search_functions(..)` / `file_entities(..)` all use `.unwrap_or_default()` — query errors silently degrade to empty, which is LSP-spec-correct.
  - `connect_db` uses `std::fs::create_dir_all(parent).ok()` — deliberately swallows the error, letting SurrealKv surface a meaningful failure. Fine.

## Red flags

1. **References returns `Some(vec![])` on empty** (line 235). LSP spec allows `None` or `[]`; both are valid, but be consistent with `goto_definition` which returns `None`. Minor.
2. **Unicode identifier handling is byte-oriented** (see Position mapping above). This is the single most impactful correctness bug. Will silently corrupt GoTo / Hover / References for any file with non-ASCII identifiers or non-ASCII characters earlier on the same line.
3. **`positionEncoding` capability not negotiated.** Server does not send `position_encoding`, so clients assume UTF-16. Code uses byte offsets. Latent bug; only manifests on non-ASCII lines.
4. **No `did_change`/`did_save` handler despite `TextDocumentSyncKind::INCREMENTAL`.** Advertising incremental sync without consuming the diffs is wasted bandwidth between the editor and LSP. Consider advertising `NONE` (the LSP re-reads from disk anyway, per line 489 comment) or implementing a cache.
5. **Re-reads file from disk on every request** (`word_at_position_from_uri`, lines 490-494). On save-before-jump this is fine; on dirty buffers (unsaved edits) GoTo uses stale content. Agent brief acknowledges this; worth a doc note for users.
6. **`hover` picks `.next()` arbitrarily when multiple functions share a name** (line 255). In a monorepo with overloads / duplicate names across modules, this shows a random one. Should prefer the entity whose `file_path == current uri` and whose `start_line..=end_line` contains `pos.line + 1`. Agent brief calls this out as step 2 of position-to-entity mapping — not implemented.
7. **Repo inference from directory name only** (line 94). If two different projects share a directory basename (e.g. two checkouts of `api/`), they share a DB. Agent brief already documents this as a known gotcha.
8. **`references.context.include_declaration` not honored** — LSP contract violation when the client expects the declaration site included.
9. **`workspace_symbol` ignores `SymbolKind` diversity** — hardcoded `SymbolKind::FUNCTION` (line 315, 371). Classes/structs/traits appearing in results would all be mislabeled as functions. Consider mapping from the entity's `kind` field.
10. **Surreal `ns/db`**: `use_ns("codescope").use_db(repo)` — `repo` is used verbatim, which is fine for SurrealKv but if `repo` contains unusual characters (spaces, colons), SurrealDB string parsing could choke. The agent brief doesn't flag this; low probability in practice.
11. **Tests cover only `word_at_position`** (3 tests, lines 504-542). No tests for: line conversion (`location_for_entity`), URI→path round-trip, empty-DB behavior, or the document_symbol abs/rel fallback. All are pure functions — trivially testable.

## Action items

1. Fix Unicode identifier handling — switch from `line.as_bytes()` byte-walk to a `char_indices()`-based walk using `is_xid_continue`/`is_xid_start` (via the `unicode-ident` crate) or at minimum `char::is_alphanumeric() || '_'`. Translate LSP UTF-16 columns to byte offsets correctly.
2. Negotiate `position_encoding` in `InitializeResult::capabilities` (offer UTF-8) so the byte-offset assumption becomes explicit.
3. In `hover` / `goto_definition` when `find_function` returns multiple hits, filter by `(file_path matches current uri) && (start_line..=end_line contains pos.line+1)` before picking, falling back to `.next()` only if nothing matches.
4. Map entity `kind` to `SymbolKind` in `workspace_symbol` and `document_symbol` (Function / Class / Method / Struct / etc.) so editors render proper icons.
5. Honor `ReferenceParams.context.include_declaration` — if true, also include the function's own definition location.
6. Decide: either implement `did_change`/`did_save` (cache buffers → use in `word_at_position_from_uri`) **or** downgrade the advertised sync to `TextDocumentSyncKind::NONE` so the protocol matches reality.
7. Add unit tests for `location_for_entity` (1-based→0-based conversion, end<start clamp, abs vs rel join) and an integration-ish test that boots a `Backend` against a temp empty DB and verifies no-crash on each RPC.
8. Normalize path separators once at the boundary: on Windows, `path.to_string_lossy().replace('\\', "/")` before the first `file_entities` lookup would avoid the two-shot pattern in `document_symbol`.
9. Document the subdirectory-vs-repo-root DB inference gotcha in the LSP user-facing README (agent brief already flagged this; make sure it's actually in the docs).
10. Consider persisting `gq` beyond `shutdown` or explicitly draining/flushing the Surreal handle — right now shutdown just drops the `Arc`; if the DB has pending writes (the LSP is read-only today, so currently moot, but worth locking in before any write-path lands).

## File reference

- Source: `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\lsp\src\lib.rs` (544 lines, reviewed in full).
- Agent brief: `C:\Users\onurg\OneDrive\Documents\graph-rag\.claude\agents\lsp-bridge-lead.md`.

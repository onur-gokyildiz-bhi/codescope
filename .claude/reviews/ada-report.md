# Ada's Grill Report — 2026-04-14

> "Bir tool-count budget'ını ve bir token budget'ını ihlal etmeden geçemezsin."
> (Note: Bash was denied this session, so clippy/fmt were NOT executed. Static-only audit. Rerun with bash for the final gate.)

## Invariants

| Invariant | Status | Detail |
|---|---|---|
| Tool count ≤ 40 (target ≤ 32) | PASS | 32 `#[tool(...)]` attrs across 17 files — exactly at target. Zero headroom. |
| Description ≤ 100 chars | FAIL | 11 of 32 tool descriptions exceed 100 chars. Worst offender: `code_search` at ~280 chars. |
| Schema ↔ migrations sync | PASS (with caveat) | `init_schema` uses `DEFINE ... IF NOT EXISTS` throughout, so additive changes don't require a migration entry per the `migrations.rs` docstring. `SCHEMA_VERSION = 1`, single v0→v1 no-op migration registered. Latest commit (tags search) is additive on existing field — fine. |
| Clippy clean (`-D warnings`) | UNVERIFIED | Bash denied this session. Last clippy-touching commit (`745ee9e`) fixed `ptr_arg`. Must be re-run before signoff. |
| Fmt clean (`--check`) | UNVERIFIED | Bash denied this session. CI auto-formats on push now (commit `7b58344`), so drift is possible on local WIP. Must be re-run before signoff. |

## Red flags

### 1. Tool description length violations (INVARIANT BREACH)

Canonical rule: ≤ 100 chars. Current state:

| File:line | Tool (approx) | Length | Delta over budget |
|---|---|---|---|
| `crates/mcp-server/src/tools/search.rs:18` | `search` (unified) | ~280 | +180 |
| `crates/mcp-server/src/tools/temporal.rs:47` | `code_health` | ~199 | +99 |
| `crates/mcp-server/src/tools/skills.rs:15` | `skills` | ~171 | +71 |
| `crates/mcp-server/src/tools/contributors.rs:14` | `contributors` | ~160 | +60 |
| `crates/mcp-server/src/tools/http.rs:15` | `http_analysis` | ~142 | +42 |
| `crates/mcp-server/src/tools/knowledge.rs:100` | `knowledge` | ~140 | +40 |
| `crates/mcp-server/src/tools/conversations.rs:15` | `conversations` | ~137 | +37 |
| `crates/mcp-server/src/tools/quality.rs:15` | `lint` | ~128 | +28 |
| `crates/mcp-server/src/tools/memory.rs:14` | `memory` | ~112 | +12 |
| `crates/mcp-server/src/tools/refactor.rs:15` | `refactor` | ~106 | +6 |
| `crates/mcp-server/src/tools/search.rs:122` | `retrieve_archived` | ~104 | +4 |

Pattern: every unified/multi-mode tool crams the full mode enumeration into the description. That's exactly what degrades Claude's selection. Move mode docs into per-mode parameter descriptions or an `action` enum doc.

### 2. admin.rs tool description sits at exactly 100

`admin.rs:15` (`project_admin`): "Project management: action=init|list. init: open a project (daemon mode). list: show open projects." — 100 chars flat. Adding a single character to that string breaches the invariant. Zero buffer.

### 3. Tool count has zero headroom vs. target

32 tools against a target of 32. The next feature that adds a tool will break the soft target (target ≤ 32), though still under the hard budget (≤ 40). Protect the consolidation work: any new MCP tool proposal should merge into an existing unified tool unless there is a structural reason not to.

### 4. Schema defect unrelated to migrations (but worth flagging)

`crates/core/src/graph/schema.rs` lines 272–274:
```
DEFINE FIELD IF NOT EXISTS agent ON problem TYPE option<string>;
DEFINE FIELD IF NOT EXISTS agent ON solution TYPE option<string>;
DEFINE FIELD IF NOT EXISTS agent ON conv_topic TYPE option<string>;
```
These appear BEFORE `DEFINE TABLE ... problem` (line 276) and `DEFINE TABLE ... solution` (line 292). On a fresh DB this may error or be silently dropped depending on SurrealDB's execution order for semicolon-separated statements. Move them below their respective table definitions, or co-locate with the other per-table fields. Not a migration invariant breach (all fields are additive and idempotent) but a latent correctness bug.

### 5. Clippy + fmt unverified this run

Invariants 4 and 5 are pre-commit gates but I could not execute them. Do not sign off based on this report alone — run `cargo clippy --workspace -- -D warnings` and `cargo fmt --all -- --check` before the next push.

## Green flags

- Tool count discipline holds: 32 = target. The 57→32 consolidation has not leaked.
- Schema version scaffolding is in place: `SCHEMA_VERSION` constant, `meta:schema` row, `migrate_to_current` walker, and a documented convention that additive `IF NOT EXISTS` changes do not need migration entries. This is the right architecture.
- Recent commits show hygiene: auto-fmt CI (`7b58344`), clippy fix (`745ee9e`), stale-lock recovery (`4d2c674`).
- Recent tag-search feature (`b1770a2`) landed without touching schema — the `tags` field already existed at `schema.rs:379`. No migration hole.
- Migration registry is correctly structured with `from_version`/`to_version`/`description`/idempotent `run`. Easy to extend.

## Recommended actions

1. **Shorten the 11 over-budget tool descriptions**. Highest priority: `search` (280), `code_health` (199), `skills` (171). Target pattern: "One-line purpose. Modes: a|b|c." and move per-mode detail into the `mode` parameter's `#[schemars(description = ...)]`.
2. **Fix field-before-table ordering in `schema.rs:272-274`**. Move the stray `agent` field definitions under their owning table DEFINE blocks.
3. **Re-run `cargo clippy --workspace -- -D warnings` and `cargo fmt --all -- --check` before the next commit lands**. This report could not verify them.
4. **Enforce a CI gate on tool description length**. A 10-line Python script in CI that greps `description = "..."` in `crates/mcp-server/src/tools/*.rs` and fails on any string >100 chars would make this invariant auto-enforced — same treatment fmt already got.
5. **Protect the 32-tool target**. Add a CI check that counts `#[tool(` and fails above 40 (hard) or warns above 32 (soft). Prevents silent sprawl.
6. **Before the next release tag**: verify every shipped commit since the last tag has a `status:done + shipped:YYYY-MM-DD + vX.Y.Z` knowledge entry (I did not cross-check this — knowledge graph not queried this session).

---
Signoff: **BLOCKED** until description-length violations are resolved and clippy/fmt are re-verified.

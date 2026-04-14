---
name: _SHARED (reference, not an agent)
description: Shared codescope-first rule + guardrails copied into every agent prompt.
---

# Codescope-first rule — MANDATORY for all agents

ALWAYS prefer codescope MCP tools over Read/Grep/Glob. Tokens are not free.

| Instead of... | Use... | Saves |
|---|---|---|
| Read whole file | `context_bundle(file_path)` | ~80% |
| Grep for callers | `find_callers(name)` | ~90% |
| Search for function | `search(mode="fuzzy", query=...)` / `search(mode="exact", query=...)` | ~70% |
| Trace call graph | `impact_analysis(name, depth=3)` | ~95% |
| Explore neighborhood | `search(mode="neighborhood", query=name)` | ~75% |
| Check git churn | `code_health(mode="churn")` / `code_health(mode="hotspots")` | ~85% |
| Query past work | `knowledge(action="search", query=topic)` before implementing | prevents redo |

Read is ONLY for reading function bodies after codescope gave you the exact file:line.

# Shared guardrails

1. **`cargo fmt --all` before every commit.** CI auto-fixes if forgotten but creates an extra commit.
2. **`cargo clippy --workspace -- -D warnings`** must pass. No warnings allowed.
3. **No breaking MCP tool API without bumping minor version.** Tool consolidation is a breaking change — ship in a .minor bump with migration table.
4. **Knowledge save on every meaningful finding** — use `knowledge(action="save", ...)` with `status:done|in-progress|planned|blocked` + `shipped:YYYY-MM-DD` + `vX.Y.Z` tags.
5. **Task tracking** — non-trivial multi-step work uses TaskCreate/TaskUpdate. Mark `completed` as soon as done, never batch.
6. **Release cadence** — every shipped feature goes out in a tagged release. Not `cargo build --release && scp`. Tag, push, `gh release create`, let CI build binaries.
7. **Dean / user feedback goes to knowledge graph** — treat external user reports as first-class knowledge entries, `kind: "entity"` or `kind: "decision"` depending on whether it drove a code change.
8. **Web UI + MCP + LSP share one DB** — never open SurrealKv from two processes. Daemon mode exists for this. If a tool hits "LOCK held", suggest `pkill -f codescope` and retry, or point at the daemon.
9. **CUDA `__global__` / `__device__` qualifiers are first-class graph citizens** — if a parser change drops them, PPL gate fails immediately.
10. **Observability is opt-in** — `CODESCOPE_OTLP_ENDPOINT` unset = zero network calls, zero overhead. Never change that default.

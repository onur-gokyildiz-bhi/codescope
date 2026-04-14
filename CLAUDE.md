# Codescope

Rust-native code intelligence engine with SurrealDB knowledge graphs.

## CRITICAL: Use Codescope MCP Tools, Not Read/Grep

This rule survives context compaction. Even if you forget everything else, remember this:
- `context_bundle(file)` before Read
- `find_callers(name)` before Grep
- `search_functions(query)` before Glob
- `impact_analysis(name)` before manual tracing
- `knowledge_search(topic)` before implementing anything

Read is ONLY for reading function bodies after codescope gave you the exact file:line.

## Quick Commands

```bash
cargo run -p codescope -- index <path> --repo <name>
cargo run -p codescope -- search <pattern>
cargo run -p codescope -- query "SELECT * FROM \`function\` LIMIT 10"
cargo run -p codescope -- mcp <path> --auto-index
cargo run -p codescope -- web <path> --port 9876 --auto-index
cargo run -p codescope-bench -- <path> --json
```

## SurrealQL Note

`function` is a reserved word — always use backticks: `` `function` ``

## For Projects Using Codescope

When codescope MCP is available, ALWAYS prefer these tools over Read/Grep:

| Instead of...              | Use...                          | Token savings |
|----------------------------|----------------------------------|---------------|
| Read whole file            | `context_bundle(file_path)`      | ~80%          |
| Grep + Read for callers    | `find_callers(name)`             | ~90%          |
| Multiple Read for function | `find_function(name)`            | ~70%          |
| Manual call graph tracing  | `impact_analysis(name, depth=3)` | ~95%          |
| Grep across codebase       | `search_functions` / `related`   | ~85%          |
| Read file to understand it | `explore(entity_name)`           | ~75%          |

**Rule**: Only use `Read` AFTER codescope pinpoints the exact function/line you need.

## Work Tracking via Knowledge Graph (MANDATORY)

Every completed task, feature, or fix MUST be saved to the knowledge graph with status tags. This prevents future sessions from redoing work that's already shipped.

### Status Tags

| Tag | When to Use |
|-----|-------------|
| `status:done` | Task completed and shipped |
| `status:in-progress` | Currently being worked on |
| `status:planned` | On the roadmap, not started |
| `status:blocked` | Waiting on something |
| `shipped:YYYY-MM-DD` | Ship date (absolute, not relative) |
| `vX.Y.Z` | Which release it shipped in |

### When to Save

- **After completing a feature/fix**: `knowledge_save` with `kind: "decision"`, tags include `status:done`, version, ship date
- **After planning work**: `knowledge_save` with `kind: "concept"`, tags include `status:planned`
- **Before starting work**: `knowledge_search` for the topic — check if `status:done` already exists

### Example

```
knowledge_save(
  title: "Delta-Mode context_bundle",
  content: "Session cache returns diff on repeat calls...",
  kind: "decision",
  tags: ["status:done", "v0.7.1", "mcp-server", "shipped:2026-04-14"]
)
```

### Rule: Search Before You Build

Before implementing anything, run `knowledge_search` for the topic. If a `status:done` entry exists, do NOT reimplement — ask the user if they want changes to the existing implementation.

## Memory (lightweight — don't overthink it)

Use `capture_insight` only for **significant** moments, not every micro-decision:
- Architecture decisions that affect multiple files → `kind: "decision"`
- Bugs that took >5 minutes to find → `kind: "problem"` (so next time it's instant)
- User corrections ("no, do it this way") → `kind: "correction"`

Skip: variable renames, formatting choices, obvious fixes. Less is more.

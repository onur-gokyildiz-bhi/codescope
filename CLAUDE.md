# Codescope

Rust-native code intelligence engine with SurrealDB knowledge graphs.

## Quick Commands

```bash
cargo run -p codescope -- index <path> --repo <name>
cargo run -p codescope -- search <pattern>
cargo run -p codescope -- query "SELECT * FROM \`function\` LIMIT 10"
cargo run -p codescope -- mcp <path> --auto-index
cargo run -p codescope -- web <path> --port 9091 --auto-index
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

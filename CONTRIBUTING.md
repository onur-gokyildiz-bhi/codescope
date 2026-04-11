# Contributing to Codescope

## Prerequisites

- Rust 1.82+ (`rustup update stable`)
- C/C++ compiler (MSVC on Windows, gcc/clang on Linux/macOS)
- Git

No external database needed — SurrealDB runs embedded.

## Development

```bash
# Build
cargo build --workspace

# Run tests
cargo test --workspace

# Lint
cargo clippy --workspace -- -D warnings

# Format
cargo fmt --all

# Run MCP server (stdio mode)
cargo run -p codescope-mcp -- . --auto-index

# Run CLI
cargo run -p codescope -- index /path/to/project --repo myproject
cargo run -p codescope -- search "function_name" --repo myproject
```

## Architecture

```
crates/
├── core/          # Parser, graph builder, embeddings, temporal analysis
├── mcp-server/    # MCP server (52 tools), daemon mode
├── cli/           # CLI binary (index, search, query, init, web)
├── web/           # 3D web UI (Three.js visualization)
└── bench/         # Benchmark suite
```

## Testing Conventions

- **Unit tests**: `#[test]` in `crates/core/tests/`
- **Async DB tests**: `#[tokio::test]` with in-memory SurrealDB (`kv-mem`)
- **Ignored tests**: Require `CODESCOPE_TEST_JSONL_DIR` env var
- **All tests must pass before PR**: `cargo test --workspace`

## SurrealQL Notes

- `function` is a reserved word — always use backticks: `` `function` ``
- Use parameterized bindings (`$name`) — never string-interpolate user input
- All DB queries should use timeout wrappers (30s default)

## Code Patterns

### MCP Tool
```rust
#[tool(description = "What this tool does")]
async fn my_tool(&self, params: Parameters<MyParams>) -> String {
    let ctx = match self.ctx().await {
        Ok(c) => c,
        Err(e) => return e,
    };
    // ... query graph, format output ...
}
```

### DB Query (with timeout)
```rust
let result = self.timed_query("SELECT ...")
    .await?
    .take(0)?;
```

## PR Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean
- [ ] New MCP tools have integration tests
- [ ] No `unwrap()` in production code
- [ ] DB queries use parameterized bindings

## Filing Issues

Please include:

1. **Codescope version** — `codescope --version`
2. **Platform** — OS, architecture (x86_64, aarch64)
3. **Reproduction** — the exact command or MCP tool call, and the codebase it ran against (name + approximate size is enough)
4. **What happened vs what you expected**
5. **Relevant log output** — run with `RUST_LOG=codescope=debug` for verbose traces

For feature requests, explain the use case first (what you're trying to do)
before the proposed solution — that usually leads to a better design discussion.

## Support Expectations

Codescope is maintained by a single developer. Realistic expectations:

- **Bugs with reproductions** — highest priority, typically addressed within a few days
- **Feature requests** — evaluated against the project roadmap; large additions may be deferred or declined if they don't fit the graph-first focus
- **Usage questions** — please check the [quickstart](docs/quickstart.md) and [troubleshooting](docs/troubleshooting.md) docs first; unresolved questions are welcome as GitHub issues
- **PRs** — small, focused PRs with tests are the fastest path to merge; please open an issue to discuss larger changes before writing code

Silence from the maintainer is not rejection — it's queue depth. Friendly
pings after 10 days on stale issues are fine.

## Scope Boundaries

Codescope is focused and opinionated. The following are explicitly out of scope:

- LLM-hosted semantic search (we run embeddings locally; remote inference breaks the local-first guarantee)
- Language parsers beyond tree-sitter (we accept new languages as thin parser modules, not as custom frontends)
- Web UI beyond the current 3D graph view (the primary interface is the MCP server for AI agents)
- Replacing your IDE's LSP (codescope complements language servers, it does not replace them)

In-scope additions are welcome:

- New tree-sitter language extractors (target: 47+ languages)
- New graph-query MCP tools (target: structural questions agents cannot answer with grep)
- Performance improvements to existing tools
- Cross-repo analysis features

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

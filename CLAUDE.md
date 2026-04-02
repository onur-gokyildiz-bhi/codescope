# Codescope

Rust-native code intelligence engine with SurrealDB knowledge graphs.

## Quick Commands

```bash
cargo run -p codescope -- index <path> --repo <name>
cargo run -p codescope -- search <pattern>
cargo run -p codescope -- query "SELECT * FROM \`function\` LIMIT 10"
cargo run -p codescope-mcp -- <path> --auto-index
cargo run -p codescope-bench -- <path> --json
```

## SurrealQL Note

`function` is a reserved word — always use backticks: `` `function` ``

# Codescope

Rust-native code intelligence engine that builds knowledge graphs from source code. Query, analyze, and understand codebases using SurrealDB graph database — with 99%+ token savings compared to reading raw files.

## Why Codescope?

AI coding assistants burn thousands of tokens reading entire files to answer simple questions. Codescope indexes your codebase into a knowledge graph, then answers questions with precise graph queries instead of full file reads.

| Question | Traditional (read files) | Codescope (graph query) | Saving |
|----------|------------------------|------------------------|--------|
| "Find function X and its callers" | 148K tokens | 542 tokens | **99.6%** |
| "List all structs" | 1.4M tokens | 1.2K tokens | **99.9%** |
| "What's the largest function?" | 454K tokens | 289 tokens | **99.9%** |
| "Impact if I change Y?" | 142K tokens | 278 tokens | **99.8%** |

*Benchmarked on ripgrep, axum, and tokio. See [BENCHMARKS.md](BENCHMARKS.md) for full results.*

## Features

- **12 programming languages**: TypeScript, JavaScript, Python, Rust, Go, Java, C, C++, C#, Ruby, PHP, TSX
- **9 content formats**: JSON, YAML, TOML, Markdown, Dockerfile, SQL, Terraform (HCL), OpenAPI, package manifests (package.json, Cargo.toml)
- **SurrealDB graph engine**: Native graph relations (RELATE), vector search (HNSW), embedded mode (zero config)
- **18 MCP tools**: Drop-in integration with Claude Code, Cursor, and any MCP-compatible AI agent
- **Temporal analysis**: Git history tracking, hotspot detection, change coupling, contributor expertise maps
- **Code review**: PR diff analysis with graph context, automated reviewer suggestions
- **Natural language queries**: Ask questions in plain English, get graph query results

## Quick Start

```bash
# Install from source
git clone https://github.com/onur-gokyildiz-bhi/codescope
cd codescope
cargo build --release

# Index a codebase
cargo run -p codescope -- index /path/to/project --repo myproject

# Search functions
cargo run -p codescope -- search "parse"

# Ask in natural language
cargo run -p codescope -- query "SELECT name, file_path, (end_line - start_line) AS size FROM \`function\` ORDER BY size DESC LIMIT 10"
```

## MCP Server (Claude Code Integration)

```bash
# Run as MCP server
cargo run -p codescope-mcp -- /path/to/project --auto-index
```

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "codescope": {
      "type": "stdio",
      "command": "/path/to/codescope-mcp",
      "args": ["/path/to/project", "--auto-index"]
    }
  }
}
```

### Available MCP Tools

| Category | Tools |
|----------|-------|
| **Search** | `search_functions`, `find_function`, `find_callers`, `find_callees`, `file_entities` |
| **Analysis** | `impact_analysis`, `hotspot_detection`, `graph_stats` |
| **Temporal** | `sync_git_history`, `file_churn`, `change_coupling`, `contributor_map` |
| **Review** | `review_diff`, `suggest_reviewers` |
| **Query** | `ask` (NL to SurrealQL), `raw_query` |
| **Admin** | `index_codebase`, `supported_languages` |

## CLI Commands

```
codescope index <path>          Index a codebase
codescope search <pattern>      Search functions by name
codescope query <surql>         Execute raw SurrealQL
codescope stats                 Show graph statistics
codescope history <path> churn  Most changed files
codescope history <path> coupling  Files changed together
codescope embed                 Generate embeddings (Ollama/OpenAI)
codescope semantic-search <q>   Vector similarity search
codescope sync-history <path>   Sync git history to graph
codescope hotspots              Detect high-risk code areas
codescope languages             List supported languages
```

## Architecture

```
codescope/
├── crates/
│   ├── core/          # Graph engine, parser, embeddings, temporal
│   ├── cli/           # Command-line interface (codescope)
│   ├── mcp-server/    # MCP server (codescope-mcp)
│   └── bench/         # Benchmark suite
```

**Tech stack**: Rust, SurrealDB 2.x (embedded RocksDB), tree-sitter, rmcp (official MCP SDK)

## Benchmarks

Run on real open-source projects:

| Repository | Files | Entities | Relations | Index Time | Avg Token Saving |
|-----------|-------|----------|-----------|------------|-----------------|
| ripgrep | 101 | 3,594 | 15,551 | 9.6s | **99.8%** |
| axum | 296 | 4,231 | 14,143 | 10.7s | **98.9%** |
| tokio | 769 | 12,628 | 43,755 | 33.3s | **99.8%** |

See [BENCHMARKS.md](BENCHMARKS.md) for detailed methodology and per-scenario results.

## Run Your Own Benchmarks

```bash
cargo run -p codescope-bench -- /path/to/repo --json --output results.json
```

## License

MIT

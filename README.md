# Codescope

Rust-native code intelligence engine that builds knowledge graphs from any codebase. Query, analyze, and understand code + config + docs + infrastructure using SurrealDB graph database — with **99%+ token savings** compared to reading raw files.

## Why Codescope?

AI coding assistants burn thousands of tokens reading entire files to answer simple questions. Codescope indexes your codebase into a knowledge graph, then answers questions with precise graph queries instead of full file reads.

| Question | Traditional (read files) | Codescope (graph query) | Saving |
|----------|------------------------|------------------------|--------|
| "Find function X and its callers" | 148K tokens | 542 tokens | **99.6%** |
| "List all structs" | 1.4M tokens | 1.2K tokens | **99.9%** |
| "What's the largest function?" | 454K tokens | 289 tokens | **99.9%** |
| "Impact if I change Y?" | 142K tokens | 278 tokens | **99.8%** |

*Benchmarked on ripgrep, axum, and tokio. See [BENCHMARKS.md](BENCHMARKS.md) for full results.*

---

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [CLI Reference](#cli-reference)
- [MCP Server (AI Agent Integration)](#mcp-server-ai-agent-integration)
- [Supported Languages & Formats](#supported-languages--formats)
- [Query Guide (SurrealQL)](#query-guide-surql)
- [Git History Analysis](#git-history-analysis)
- [Embedding & Semantic Search](#embedding--semantic-search)
- [Benchmarking](#benchmarking)
- [Architecture](#architecture)
- [Configuration](#configuration)
- [Contributing](#contributing)
- [License](#license)

---

## Features

- **21 supported formats**: 12 programming languages + 9 content formats (JSON, YAML, Markdown, Dockerfile, SQL, Terraform, OpenAPI, package manifests)
- **SurrealDB graph engine**: Native graph relations (RELATE), vector search (HNSW), embedded mode — zero external dependencies
- **MCP server**: 11 tools for Claude Code, Cursor, and any MCP-compatible AI agent
- **Temporal analysis**: Git history tracking, hotspot detection, change coupling, contributor expertise maps
- **Natural language queries**: Ask questions in plain English, get graph query results
- **Semantic search**: Embed code with Ollama or OpenAI, search by meaning
- **Benchmark suite**: Built-in benchmarking against traditional approaches

---

## Installation

### Option 1: Download Pre-built Binary (Recommended)

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.ps1 | iex
```

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh | bash
```

This downloads the latest release, installs `codescope` and `codescope-mcp` to your PATH, and you're ready to go. No Rust toolchain required.

You can also download binaries manually from the [Releases page](https://github.com/onur-gokyildiz-bhi/codescope/releases).

### Option 2: Build from Source

Requires [Rust](https://rustup.rs/) 1.75+ and a C/C++ compiler (for SurrealDB RocksDB and tree-sitter).

```bash
git clone https://github.com/onur-gokyildiz-bhi/codescope
cd codescope
cargo build --release
```

Binaries will be at:
- `target/release/codescope` — CLI tool
- `target/release/codescope-mcp` — MCP server
- `target/release/codescope-bench` — Benchmark runner

Optionally add them to PATH:

```bash
# Linux/macOS
cp target/release/codescope ~/.local/bin/
cp target/release/codescope-mcp ~/.local/bin/

# Windows (PowerShell)
Copy-Item target\release\codescope.exe $env:LOCALAPPDATA\codescope\bin\
Copy-Item target\release\codescope-mcp.exe $env:LOCALAPPDATA\codescope\bin\
```

### Option 3: Cargo Install (Rust users)

```bash
cargo install --git https://github.com/onur-gokyildiz-bhi/codescope codescope
cargo install --git https://github.com/onur-gokyildiz-bhi/codescope codescope-mcp
```

---

## Quick Start

### 1. Index a Codebase

```bash
codescope index /path/to/project --repo myproject
```

This parses all supported files, extracts entities (functions, classes, configs, docs, etc.) and relationships (calls, imports, contains, depends_on, etc.), and stores them in an embedded SurrealDB database at `.graph-rag/db`.

**Options:**
- `--repo <name>` — Repository name (default: directory name)
- `--clean` — Clear existing data before re-indexing
- `--db-path <path>` — Custom database location

### 2. Search

```bash
# Search functions by name
codescope search "parse"

# Search is case-insensitive and supports partial matches
codescope search "auth"
```

### 3. Query the Graph

```bash
# Count all indexed files
codescope query "SELECT count() FROM file GROUP ALL"

# List the 10 largest functions
codescope query "SELECT name, file_path, (end_line - start_line) AS size FROM \`function\` ORDER BY size DESC LIMIT 10"

# Find all structs
codescope query "SELECT name, file_path FROM class WHERE kind = 'Struct' ORDER BY name"
```

> **Note:** `function` is a reserved word in SurrealQL — always wrap it in backticks: `` `function` ``

### 4. List Supported Languages

```bash
codescope languages
```

---

## CLI Reference

### `codescope index <path>`

Index a codebase into the knowledge graph.

```bash
codescope index ./my-project --repo my-project
codescope index ./my-project --repo my-project --clean    # fresh re-index
codescope index ./my-project --db-path /tmp/mydb           # custom DB location
```

**What gets indexed:**
- Functions, methods, classes, structs, enums, interfaces, traits
- Import statements and call sites (who calls whom)
- JSON/YAML/TOML config keys and sections
- Markdown headings, links, and code blocks
- Dockerfile stages and instructions
- SQL tables, views, and indexes
- Terraform resources, variables, and providers
- OpenAPI endpoints and schemas
- Package dependencies and scripts (package.json, Cargo.toml)

### `codescope search <pattern>`

Search functions by name pattern (case-insensitive partial match).

```bash
codescope search "handler"         # find all handler functions
codescope search "test"            # find all test functions
codescope search "parse" --limit 5 # limit results
```

### `codescope query <surql>`

Execute a raw SurrealQL query against the knowledge graph.

```bash
# List all tables and counts
codescope query "SELECT count() FROM file GROUP ALL"
codescope query "SELECT count() FROM \`function\` GROUP ALL"
codescope query "SELECT count() FROM class GROUP ALL"
codescope query "SELECT count() FROM config GROUP ALL"
codescope query "SELECT count() FROM doc GROUP ALL"
codescope query "SELECT count() FROM infra GROUP ALL"

# Find functions by file
codescope query "SELECT name, start_line FROM \`function\` WHERE string::contains(file_path, 'main.rs')"

# Graph traversal: who calls this function?
codescope query "SELECT <-calls<-\`function\`.name AS callers FROM \`function\` WHERE name = 'handle_request'"

# Graph traversal: what does this function call?
codescope query "SELECT ->calls->\`function\`.name AS callees FROM \`function\` WHERE name = 'main'"

# Find all Docker stages
codescope query "SELECT name, file_path FROM infra WHERE kind = 'DockerStage'"

# Find all config keys containing 'database'
codescope query "SELECT name, body, file_path FROM config WHERE string::contains(string::lowercase(name), 'database')"

# Find all Markdown sections
codescope query "SELECT name, file_path FROM doc WHERE kind = 'DocSection' LIMIT 20"

# Language distribution
codescope query "SELECT language, count() AS cnt FROM file GROUP BY language ORDER BY cnt DESC"
```

### `codescope stats`

Show graph statistics (file/function/class/relation counts).

```bash
codescope stats
```

### `codescope history <path> <action>`

Analyze git history without requiring the graph database.

```bash
# Most frequently changed files
codescope history /path/to/repo churn --limit 20

# Files that change together (change coupling)
codescope history /path/to/repo coupling --limit 20

# Recent commits
codescope history /path/to/repo commits --limit 10

# Who knows what (contributor expertise map)
codescope history /path/to/repo contributors
```

### `codescope embed`

Generate vector embeddings for all indexed functions (requires Ollama or OpenAI).

```bash
# Using Ollama (default, local)
codescope embed --provider ollama --model nomic-embed-text

# Using OpenAI
OPENAI_API_KEY=sk-... codescope embed --provider openai --model text-embedding-3-small

# Custom batch size
codescope embed --provider ollama --batch-size 50
```

### `codescope semantic-search <query>`

Search code by meaning using vector similarity (requires embeddings).

```bash
codescope semantic-search "function that handles HTTP requests"
codescope semantic-search "error handling logic" --limit 5
```

### `codescope languages`

List all supported programming languages and content formats.

---

## MCP Server (AI Agent Integration)

The MCP server exposes Codescope's capabilities as tools that AI agents (Claude Code, Cursor, etc.) can call directly.

### Start the Server

```bash
codescope-mcp /path/to/project --auto-index
```

**Options:**
- `--auto-index` — Parse and index the codebase on startup
- `--repo <name>` — Repository name
- `--db-path <path>` — Custom database location
- `--embeddings ollama` — Enable semantic search with Ollama

### Quick Setup (Recommended)

One command installs MCP config, slash commands, and skills:

```bash
# Linux/macOS
curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/setup-claude.sh | bash

# Windows (PowerShell)
irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/setup-claude.ps1 | iex
```

This configures:
- **MCP server** in `~/.claude.json` (auto-index on startup)
- **6 slash commands**: `/codescope`, `/cs-search`, `/cs-index`, `/cs-stats`, `/cs-ask`, `/cs-impact`
- **CLAUDE.md template** for project-level instructions

### Manual Setup

Add to `~/.claude.json` (global, all projects):

```json
{
  "mcpServers": {
    "codescope": {
      "command": "codescope-mcp",
      "args": [".", "--auto-index"]
    }
  }
}
```

Or create `.mcp.json` in your project root (project-level):

```json
{
  "mcpServers": {
    "codescope": {
      "command": "codescope-mcp",
      "args": [".", "--auto-index"]
    }
  }
}
```

### Slash Commands

After setup, these commands are available in Claude Code:

| Command | Description |
|---------|-------------|
| `/codescope` | Main menu — routes to sub-commands |
| `/cs-search <pattern>` | Search functions by name |
| `/cs-index` | Re-index current project |
| `/cs-stats` | Show codebase statistics |
| `/cs-ask <question>` | Ask in Turkish or English |
| `/cs-impact <function>` | Analyze change impact |
| `/cs-callers <function>` | Who calls this function? |
| `/cs-file <path>` | List all entities in a file |
| `/cs-query <surql>` | Execute raw SurrealQL query |

Claude will also **automatically use Codescope tools** when you ask code structure questions in natural language (Turkish or English):

```
> Bu fonksiyonu kim cagiriyor?        → find_callers
> auth ile ilgili fonksiyonlari goster → search_functions
> Bunu degistirsem ne etkilenir?       → impact_analysis
> What's the largest function?         → raw_query
```

### Available MCP Tools

| Tool | Description | Example |
|------|-------------|---------|
| `search_functions` | Search functions by name pattern | `{"query": "parse", "limit": 10}` |
| `find_function` | Find function by exact name | `{"query": "main"}` |
| `file_entities` | List all entities in a file | `{"file_path": "src/main.rs"}` |
| `find_callers` | Find all callers of a function | `{"function_name": "handle_request"}` |
| `find_callees` | Find all functions called by a function | `{"function_name": "main"}` |
| `impact_analysis` | Analyze blast radius of changing a function | `{"function_name": "parse", "depth": 3}` |
| `graph_stats` | Get graph statistics | (no params) |
| `raw_query` | Execute raw SurrealQL | `{"query": "SELECT * FROM class LIMIT 5"}` |
| `index_codebase` | Re-index the codebase | `{"clean": true}` |
| `ask` | Natural language question | `{"question": "what are the largest functions?"}` |
| `supported_languages` | List supported languages | (no params) |

### Natural Language Query Examples

The `ask` tool translates plain English questions to SurrealQL:

| Question | Generated Query |
|----------|----------------|
| "how many files are indexed?" | `SELECT count() FROM file GROUP ALL` |
| "what are the largest functions?" | `SELECT name, file_path, (end_line - start_line) AS size FROM \`function\` ORDER BY size DESC LIMIT 10` |
| "list all structs" | `SELECT name, kind, file_path FROM class ORDER BY name LIMIT 50` |
| "show call graph for main" | `SELECT ->calls->\`function\`.name AS calls FROM \`function\` WHERE name = 'main'` |
| "find all imports" | `SELECT name, file_path FROM import_decl ORDER BY file_path LIMIT 50` |

---

## Supported Languages & Formats

### Programming Languages (tree-sitter)

| Language | Extensions |
|----------|-----------|
| TypeScript | `.ts` |
| TSX | `.tsx` |
| JavaScript | `.js`, `.jsx`, `.mjs`, `.cjs` |
| Python | `.py`, `.pyi` |
| Rust | `.rs` |
| Go | `.go` |
| Java | `.java` |
| C | `.c`, `.h` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` |
| C# | `.cs` |
| Ruby | `.rb` |
| PHP | `.php` |

### Content Formats (custom parsers)

| Format | Extensions | Extracted Entities |
|--------|-----------|-------------------|
| JSON | `.json` | Keys, sections, nested structures |
| YAML | `.yaml`, `.yml` | Keys, sections, nested maps |
| TOML | `.toml` | Sections, key-value pairs |
| Markdown | `.md`, `.mdx` | Headings, links, code blocks |
| Dockerfile | `Dockerfile*` | Stages (FROM), instructions (RUN, COPY, ENV) |
| SQL | `.sql` | Tables, views, indexes |
| Terraform | `.tf`, `.tfvars` | Resources, variables, providers, modules |
| OpenAPI | (auto-detected) | Endpoints, schemas, fields |
| Package manifests | `package.json`, `Cargo.toml` | Package info, dependencies, scripts |

---

## Query Guide (SurrealQL)

### Entity Tables

| Table | Contains | Key Fields |
|-------|----------|------------|
| `file` | Source files | `path`, `language`, `repo` |
| `` `function` `` | Functions & methods | `name`, `signature`, `file_path`, `start_line`, `end_line` |
| `class` | Classes, structs, enums, traits | `name`, `kind`, `file_path` |
| `import_decl` | Import statements | `name`, `body`, `file_path` |
| `config` | JSON/YAML/TOML entries | `name`, `kind`, `body` |
| `doc` | Markdown elements | `name`, `kind` (DocSection/DocLink/DocCodeBlock) |
| `infra` | Docker/Terraform entities | `name`, `kind` (DockerStage/InfraResource) |
| `package` | Package manifests | `name`, `kind` (Package/Dependency/Script) |
| `db_entity` | SQL schema objects | `name`, `kind` (DbTable/DbView/DbIndex) |
| `api` | API definitions | `name`, `kind` (ApiEndpoint/ApiSchema) |

### Relation Tables (Edges)

| Edge | Meaning | Example |
|------|---------|---------|
| `contains` | Parent contains child | file → function |
| `calls` | Function calls function | main → parse |
| `imports` | File imports module | file → import_decl |
| `inherits` | Class extends class | Dog → Animal |
| `implements` | Class implements interface | UserService → Service |
| `depends_on` | Module depends on module | — |
| `depends_on_package` | Package depends on package | myapp → express |
| `has_field` | Schema has field | UserSchema → email |
| `references` | Doc links to target | heading → url |

### Common Query Patterns

```sql
-- Find functions in a specific file
SELECT name, start_line FROM `function`
WHERE string::contains(file_path, 'main.rs');

-- Call graph: who calls function X?
SELECT <-calls<-`function`.name AS callers
FROM `function` WHERE name = 'handle_request';

-- Call graph: what does function X call?
SELECT ->calls->`function`.name AS callees
FROM `function` WHERE name = 'main';

-- Find all config values containing "port"
SELECT name, body FROM config
WHERE string::contains(string::lowercase(name), 'port');

-- Most complex functions (by line count)
SELECT name, file_path, (end_line - start_line) AS lines
FROM `function` ORDER BY lines DESC LIMIT 10;

-- Language distribution
SELECT language, count() AS cnt FROM file
GROUP BY language ORDER BY cnt DESC;

-- All Dockerfile stages across the project
SELECT name, file_path, body FROM infra
WHERE kind = 'DockerStage';
```

---

## Git History Analysis

Codescope analyzes git history independently of the graph database.

### File Churn

Find the most frequently changed files — high churn often indicates complexity or instability.

```bash
codescope history /path/to/repo churn --limit 20
```

Output:
```
  22  src/lib.rs
  19  src/models/turbo_generic.rs
  14  README.md
  13  src/main.rs
```

### Change Coupling

Find files that always change together — strong coupling suggests hidden dependencies.

```bash
codescope history /path/to/repo coupling --limit 10
```

Output:
```
  12  turbo_generic.rs <-> lib.rs
   6  BENCHMARK.md <-> README.md
```

### Contributor Expertise

See who knows which parts of the codebase best.

```bash
codescope history /path/to/repo contributors
```

---

## Embedding & Semantic Search

Codescope supports vector embeddings for semantic code search.

### Setup with Ollama (recommended, local)

```bash
# Install Ollama: https://ollama.ai
ollama pull nomic-embed-text

# Generate embeddings
codescope embed --provider ollama

# Search by meaning
codescope semantic-search "function that validates user input"
```

### Setup with OpenAI

```bash
export OPENAI_API_KEY=sk-...
codescope embed --provider openai --model text-embedding-3-small
codescope semantic-search "error handling and retry logic"
```

---

## Benchmarking

### Run Benchmarks

```bash
# Benchmark a repository
codescope-bench /path/to/repo --json --output results.json

# With custom repo name
codescope-bench /path/to/repo --repo myrepo --json
```

### Benchmark Output

The benchmark measures:
1. **Index speed** — files/sec, entities/sec
2. **Query speed** — ms per query type
3. **Token savings** — vs traditional file reading approach

### Results on Open-Source Projects

| Repository | Files | Entities | Relations | Index Time | Avg Token Saving |
|-----------|-------|----------|-----------|------------|-----------------|
| ripgrep | 101 | 3,594 | 15,551 | 9.6s | **99.8%** |
| axum | 296 | 4,231 | 14,143 | 10.7s | **98.9%** |
| tokio | 769 | 12,628 | 43,755 | 33.3s | **99.8%** |

See [BENCHMARKS.md](BENCHMARKS.md) for detailed per-scenario results.

---

## Architecture

```
codescope/
├── crates/
│   ├── core/              # Graph engine, parsers, embeddings, temporal analysis
│   │   ├── parser/        # tree-sitter + custom content parsers
│   │   ├── graph/         # SurrealDB schema, builder, query engine
│   │   ├── embeddings/    # Ollama/OpenAI vector embedding pipeline
│   │   ├── temporal/      # Git history analysis
│   │   └── crossrepo/     # Multi-repository linking
│   ├── cli/               # Command-line interface (codescope)
│   ├── mcp-server/        # MCP server for AI agents (codescope-mcp)
│   └── bench/             # Benchmark suite (codescope-bench)
├── BENCHMARKS.md          # Detailed benchmark results
└── CLAUDE.md              # Quick reference for AI assistants
```

**Tech stack:**
- **Rust** — Core language, zero-cost abstractions
- **SurrealDB 2.x** — Embedded graph + document + vector database (RocksDB backend)
- **tree-sitter** — Incremental parsing for 12 programming languages
- **rmcp** — Official Rust MCP SDK for AI agent integration
- **git2** — Native git history analysis

### Data Flow

```
Source Files → tree-sitter / Custom Parsers → Entities + Relations → SurrealDB Graph
                                                                          ↓
                                              CLI / MCP Server ← SurrealQL Queries
```

---

## Configuration

### Database Location

By default, Codescope stores its database in `.graph-rag/db` relative to the current directory. Override with:

```bash
codescope index /path/to/project --db-path /custom/db/path
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OPENAI_API_KEY` | OpenAI API key for embeddings | — |
| `RUST_LOG` | Log level (error/warn/info/debug) | error |
| `RUST_MIN_STACK` | Thread stack size (for large projects) | 8MB |

### Large Projects

For very large monorepos, increase the stack size:

```bash
RUST_MIN_STACK=16777216 codescope index /path/to/large-monorepo
```

---

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes and ensure `cargo check` passes
4. Run benchmarks to verify no regressions: `cargo run -p codescope-bench -- /path/to/repo`
5. Submit a pull request

### Adding a New Language

Add a tree-sitter grammar to `crates/core/src/parser/languages.rs`:

```rust
languages.push(LanguageConfig {
    name: "kotlin".into(),
    language: tree_sitter_kotlin::LANGUAGE.into(),
    extensions: vec!["kt".into(), "kts".into()],
});
```

### Adding a New Content Parser

Create a new file in `crates/core/src/parser/content/` implementing `ContentParser`:

```rust
pub struct MyParser;

impl ContentParser for MyParser {
    fn name(&self) -> &str { "myformat" }
    fn extensions(&self) -> &[&str] { &["myext"] }
    fn parse(&self, file_path: &str, source: &str, repo: &str)
        -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        // Extract entities and relations from source
    }
}
```

Register it in `crates/core/src/parser/content/mod.rs`.

---

## License

MIT — see [LICENSE](LICENSE) for details.

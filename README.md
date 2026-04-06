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
- [Comparison with Alternatives](#comparison-with-alternatives)
- [Contributing](#contributing)
- [License](#license)

---

## Features

- **44 supported formats**: 35 programming languages + 9 content formats (JSON, YAML, Markdown, Dockerfile, SQL, Terraform, OpenAPI, package manifests)
- **SurrealDB graph engine**: Native graph relations (RELATE), vector search (HNSW), embedded mode — zero external dependencies
- **45 MCP tools**: Code search, call graphs, Obsidian-like exploration, semantic search, conversation history, git analysis, and more
- **Obsidian-like knowledge navigation**: `explore` (local graph), `context_bundle` (file overview), `backlinks` (incoming references), `related` (cross-type search)
- **Conversation memory**: Auto-indexes Claude Code sessions — tracks decisions, problems, solutions, and links them to code entities
- **Auto CONTEXT.md**: Generates a dynamic context file with recent decisions/problems so Claude sees your project history automatically
- **Skill/Knowledge graphs**: Index markdown skill files with `[[wikilinks]]` + YAML frontmatter, progressive disclosure traversal — arscontexta compatible
- **HTTP cross-service linking**: Detect HTTP client calls (reqwest, fetch, axios, requests) and link to API endpoints
- **Symbol-level refactoring**: `rename_symbol`, `find_unused`, `safe_delete` for code cleanup
- **Temporal analysis**: Git history tracking, hotspot detection, change coupling, contributor expertise maps
- **Semantic search**: Local embeddings with FastEmbed (zero deps), or Ollama/OpenAI — search code by meaning
- **Incremental indexing**: Hash-based change detection, only re-parses modified files
- **Daemon mode**: Multi-project HTTP/SSE server for concurrent AI agent connections
- **Web UI**: Interactive D3.js force-directed graph visualization
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

This downloads the latest release and installs the `codescope` binary to your PATH. The single binary includes CLI, MCP server, and Web UI as subcommands. No Rust toolchain required.

You can also download binaries manually from the [Releases page](https://github.com/onur-gokyildiz-bhi/codescope/releases).

### Option 2: Build from Source

Requires [Rust](https://rustup.rs/) 1.75+ and a C/C++ compiler (for SurrealDB and tree-sitter).

```bash
git clone https://github.com/onur-gokyildiz-bhi/codescope
cd codescope
cargo build --release
```

The unified binary will be at `target/release/codescope` (CLI, MCP server, and Web UI all in one). The benchmark runner is a separate binary at `target/release/codescope-bench`.

Optionally add to PATH:

```bash
# Linux/macOS
cp target/release/codescope ~/.local/bin/

# Windows (PowerShell)
Copy-Item target\release\codescope.exe $env:LOCALAPPDATA\codescope\bin\
```

### Option 3: Cargo Install (Rust users)

```bash
cargo install --git https://github.com/onur-gokyildiz-bhi/codescope codescope
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
codescope mcp /path/to/project --auto-index
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
      "command": "codescope",
      "args": ["mcp", ".", "--auto-index"]
    }
  }
}
```

Or create `.mcp.json` in your project root (project-level):

```json
{
  "mcpServers": {
    "codescope": {
      "command": "codescope",
      "args": ["mcp", ".", "--auto-index"]
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

### Daemon Mode (Multi-Project)

```bash
# Start daemon (background, multi-project)
codescope mcp serve --port 3333

# Start as background process
codescope mcp start --port 3333

# Check status
codescope mcp status --port 3333

# Stop daemon
codescope mcp stop --port 3333
```

### Available MCP Tools (43 tools)

**Code Search & Navigation:**

| Tool | Description |
|------|-------------|
| `search_functions` | Search functions by name pattern |
| `find_function` | Find function by exact name |
| `file_entities` | List all entities in a file |
| `find_callers` | Find all callers of a function |
| `find_callees` | Find all functions called by a function |
| `impact_analysis` | Analyze blast radius of changing a function |
| `find_dead_code` | Find functions with zero callers |

**Symbol-Level Refactoring:**

| Tool | Description |
|------|-------------|
| `rename_symbol` | Find all references to plan a rename |
| `find_unused` | Find unused symbols (zero-reference functions) |
| `safe_delete` | Check if a symbol can be safely removed |

**Obsidian-like Exploration:**

| Tool | Description |
|------|-------------|
| `explore` | Full neighborhood of any entity — like Obsidian's local graph view |
| `context_bundle` | Complete file context with cross-file links |
| `backlinks` | Find everything that references a given entity |
| `related` | Universal search across all entity types |

**HTTP Cross-Service Linking:**

| Tool | Description |
|------|-------------|
| `find_http_calls` | Find HTTP client calls (reqwest, fetch, axios) |
| `find_endpoint_callers` | Find code that calls a specific HTTP endpoint |

**Skill/Knowledge Graph:**

| Tool | Description |
|------|-------------|
| `index_skill_graph` | Index markdown skill files with [[wikilinks]] + YAML frontmatter |
| `traverse_skill_graph` | Navigate skill graph with progressive disclosure (4 detail levels) |

**Semantic Search:**

| Tool | Description |
|------|-------------|
| `embed_functions` | Generate vector embeddings (FastEmbed local, Ollama, or OpenAI) |
| `semantic_search` | Search code by meaning, not just name |

**Git & Temporal Analysis:**

| Tool | Description |
|------|-------------|
| `sync_git_history` | Import git commits into the graph |
| `hotspot_detection` | Find high-risk code (complexity x churn) |
| `file_churn` | Most frequently changed files |
| `change_coupling` | Files that always change together |
| `review_diff` | Analyze a diff with graph context |
| `suggest_reviewers` | Find best reviewers based on expertise |
| `contributor_map` | Who knows which parts of the codebase |

**Conversation Memory:**

| Tool | Description |
|------|-------------|
| `index_conversations` | Index Claude Code JSONL sessions into the graph |
| `conversation_search` | Search past decisions, problems, solutions |
| `conversation_timeline` | Track changes to a specific entity over time |
| `memory_save` | Save persistent memory notes across sessions |
| `memory_search` | Search saved memories and past decisions |

**Code Quality & Patterns:**

| Tool | Description |
|------|-------------|
| `team_patterns` | Detect naming, import, and structure conventions |
| `edit_preflight` | Check if a planned edit aligns with team patterns |
| `community_detection` | Find code clusters, bridge modules, central nodes |
| `manage_adr` | Create and manage Architecture Decision Records |

**Infrastructure:**

| Tool | Description |
|------|-------------|
| `graph_stats` | Get graph statistics |
| `raw_query` | Execute raw SurrealQL |
| `ask` | Natural language question → SurrealQL |
| `index_codebase` | Re-index (incremental by default) |
| `init_project` | Initialize a project (daemon mode) |
| `list_projects` | List open projects (daemon mode) |
| `supported_languages` | List supported formats |

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

### Programming Languages (35, tree-sitter)

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
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.hxx` |
| C# | `.cs` |
| Ruby | `.rb` |
| PHP | `.php` |
| Swift | `.swift` |
| Dart | `.dart` |
| Zig | `.zig` |
| Scala | `.scala`, `.sc` |
| Haskell | `.hs` |
| Elixir | `.ex`, `.exs` |
| Lua | `.lua` |
| OCaml | `.ml`, `.mli` |
| HTML | `.html`, `.htm` |
| Julia | `.jl` |
| Bash | `.sh`, `.bash`, `.zsh` |
| R | `.r`, `.R` |
| CSS | `.css` |
| Erlang | `.erl`, `.hrl` |
| Objective-C | `.m`, `.mm` |
| HCL / Terraform | `.hcl`, `.tf`, `.tfvars` |
| Nix | `.nix` |
| CMake | `.cmake` |
| Makefile | `.mk` |
| Verilog | `.v`, `.sv`, `.svh` |
| Fortran | `.f`, `.f90`, `.f95`, `.f03`, `.f08` |
| GLSL | `.glsl`, `.vert`, `.frag`, `.comp` |
| GraphQL | `.graphql`, `.gql` |

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
| `http_call` | HTTP client calls | `name`, `method`, `url_pattern`, `file_path` |
| `skill` | Skill/knowledge graph nodes | `name`, `description`, `node_type`, `created` |
| `conversation` | Claude Code sessions | `name`, `hash`, `timestamp` |
| `decision` | Decisions from conversations | `name`, `body`, `timestamp` |
| `problem` | Problems from conversations | `name`, `body`, `timestamp` |
| `solution` | Solutions from conversations | `name`, `body`, `timestamp` |
| `conv_topic` | Discussion topics | `name`, `body`, `timestamp` |

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
| `modified_in` | Entity modified in commit | file → commit |
| `discussed_in` | Entity discussed in conversation | decision → function |
| `decided_about` | Decision about a code entity | decision → struct |
| `solves_for` | Solution solves a problem | solution → problem |
| `co_discusses` | Sessions discussing same entity | session → session |
| `calls_endpoint` | Function calls HTTP endpoint | function → http_call |
| `links_to` | Skill wikilink connection | skill → skill |

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

Codescope supports vector embeddings for semantic code search with three providers.

### FastEmbed (recommended, zero dependencies)

```bash
# No setup needed — model downloads automatically on first use
codescope embed --provider fastembed

# Search by meaning
codescope semantic-search "function that validates user input"
```

FastEmbed runs entirely in-process using ONNX Runtime. No external services, no API keys, no Docker.

### Ollama (local, larger models)

```bash
ollama pull nomic-embed-text
codescope embed --provider ollama
codescope semantic-search "function that validates user input"
```

### OpenAI (cloud)

```bash
export OPENAI_API_KEY=sk-...
codescope embed --provider openai --model text-embedding-3-small
codescope semantic-search "error handling and retry logic"
```

### Via MCP Tools (AI agents)

AI agents can use `embed_functions` and `semantic_search` tools directly — no CLI needed.

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
│   │   ├── parser/        # tree-sitter (35 langs) + custom content parsers (9 formats)
│   │   ├── graph/         # SurrealDB schema, builder, query, incremental indexer
│   │   ├── embeddings/    # FastEmbed/Ollama/OpenAI vector embedding pipeline
│   │   ├── temporal/      # Git history analysis + graph sync
│   │   ├── conversation/  # Claude session parser, classifier, entity linker
│   │   └── crossrepo/     # Multi-repository linking
│   ├── cli/               # Unified binary — CLI, MCP server, Web UI (codescope)
│   ├── mcp-server/        # MCP server library (used by unified binary)
│   ├── web/               # Web UI library (used by unified binary)
│   └── bench/             # Benchmark suite (codescope-bench, separate binary)
├── BENCHMARKS.md          # Detailed benchmark results
└── CLAUDE.md              # Quick reference for AI assistants
```

**Tech stack:**
- **Rust** — Core language, zero-cost abstractions
- **SurrealDB 2.x** — Embedded graph + document + vector database (SurrealKV backend)
- **tree-sitter** — Incremental parsing for 35 programming languages
- **FastEmbed** — In-process ONNX embeddings (zero external deps)
- **rmcp** — Official Rust MCP SDK for AI agent integration
- **git2** — Native git history analysis
- **rayon** — Parallel file parsing for fast indexing
- **axum** — Web server for D3.js visualization

### Data Flow

```
Source Files ──→ tree-sitter / Custom Parsers ──→ Entities + Relations ──→ SurrealDB Graph
                                                                                ↓
Claude Sessions ──→ JSONL Parser ──→ Decisions/Problems/Solutions ──────→ Linked to Code
                                                                                ↓
Memory Files ──→ Markdown Parser ──→ Knowledge Entities ───────────────→ Linked to Code
                                                                                ↓
                                              CLI / MCP / Web UI ← SurrealQL Queries
                                                                                ↓
                                              CONTEXT.md ← Auto-generated summary
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

## Comparison with Alternatives

How does Codescope compare to other code intelligence tools?

### Feature Matrix

| Feature | Codescope | [codebase-memory-mcp](https://github.com/nicobailon/codebase-memory-mcp) | [CodeGraphContext](https://github.com/nicobailon/CodeGraphContext) | [Serena](https://github.com/oraios/serena) | [Claude Context](https://github.com/zilliztech/claude-context) | [code-graph-mcp](https://github.com/entrepeneur4lyf/code-graph-mcp) |
|---------|-----------|---------------------|------------------|--------|----------------|----------------|
| **Language** | Rust | C | Python | Python | Node.js | Python |
| **Analysis** | tree-sitter + SurrealDB graph | tree-sitter + SQLite | tree-sitter + KuzuDB/Neo4j | LSP servers | Embeddings only | ast-grep |
| **Languages** | 35 + 9 formats | 66 | 14 | 40+ | All (embedding) | 25+ |
| **MCP Server** | Yes (43 tools) | Yes (14 tools) | Yes | Yes | Yes | Yes |
| **Knowledge Graph** | SurrealDB (graph+doc+vector) | SQLite | KuzuDB / Neo4j / FalkorDB | None (live LSP) | None (vector only) | In-memory |
| **Persistent Storage** | Yes | Yes | Yes | No | Yes (Milvus) | No |
| **Local Embeddings** | Yes (FastEmbed, zero deps) | No | No | No | No (needs API key) | No |
| **Conversation Memory** | Yes (auto-index Claude sessions) | No | No | No | No | No |
| **Incremental Indexing** | Yes (hash-based) | No | No | No | No | No |
| **Daemon Mode** | Yes (multi-project SSE) | No | No | No | No | No |
| **Single Binary** | Yes | Yes | No | No | No | No |
| **Config/Doc Parsing** | Yes (JSON, YAML, MD, Docker, SQL, Terraform, OpenAPI) | No | No | No | No | No |
| **Semantic Search** | Yes (in-process) | No | No | No | Yes (external) | No |
| **Git History** | Yes (churn, coupling, contributors) | No | No | No | No | No |
| **Claude Code Skills** | Yes (9 slash commands) | No | No | No | No | No |
| **License** | MIT | MIT | MIT | MIT | MIT | MIT |

### Approach Comparison

| Approach | Tools | Pros | Cons |
|----------|-------|------|------|
| **Knowledge Graph + Embeddings** (Codescope) | SurrealDB, tree-sitter, FastEmbed | Persistent, queryable, semantic search, single binary, offline | Fewer languages than LSP-based |
| **Knowledge Graph Only** (codebase-memory-mcp, CGC) | SQLite/KuzuDB, tree-sitter | Fast structural queries, broad language support | No semantic search, no config/doc parsing |
| **LSP-based** (Serena) | Language Server Protocol | 40+ languages, IDE-grade precision | Ephemeral (resets each session), heavy (40+ processes), no embeddings |
| **Embedding Only** (Claude Context) | Milvus, OpenAI/Voyage | Semantic similarity search | No structural graph, requires API keys, no call graph |
| **SaaS** (Sourcegraph, Greptile) | Proprietary | Enterprise scale, managed | Not open source, requires cloud, paid |

### What Makes Codescope Different

1. **Graph + Embeddings + Conversations + Skills** — Structural code graph, semantic search, conversation memory, AND knowledge/skill graphs in a single binary. No other tool does all four.
2. **Zero external dependencies** — SurrealDB embedded, FastEmbed in-process (ONNX Runtime). No Docker, no API keys, no external databases.
3. **Beyond code** — 44 formats: 35 programming languages + JSON, YAML, TOML, Markdown, Dockerfile, SQL, Terraform, OpenAPI, package manifests.
4. **Obsidian-like navigation** — `explore`, `backlinks`, `context_bundle`, `related` — browse your codebase like an Obsidian vault.
5. **Conversation memory** — Auto-indexes Claude Code sessions, extracts decisions/problems/solutions, links them to code entities. Auto-generates CONTEXT.md so Claude knows your project history.
6. **Claude Code native** — 9 slash commands with Turkish + English natural language support.
7. **Temporal analysis** — Git history integration for churn analysis, change coupling, hotspot detection, contributor mapping.

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

<div align="center">

# Codescope

**The brain your AI coding assistant is missing.**

A graph-first code intelligence engine. Your agents stop reading files and start traversing a knowledge graph — 98-99% fewer tokens, deterministic, single-digit-millisecond traversal.

Rust-native · Fully local · MCP + LSP + Web · 57 languages · 9 agents

[Install](#install) · [Quick Start](#quick-start) · [Why Graph-First?](#why-graph-first) · [Benchmarks](BENCHMARKS.md) · [Docs](docs/) · [Releases](https://github.com/onur-gokyildiz-bhi/codescope/releases)

<img src="assets/demo-twitter.gif" alt="Codescope Demo" width="720">

</div>

---

## Why This Exists

Most AI coding assistants still embed every file as a vector, nearest-neighbor a chunk, and pray it's relevant. When you ask *"if I change `User::email`, what breaks?"* they read 40 files and burn 150,000 tokens guessing.

That's not a code intelligence problem. It's an **architecture** problem. Vectors can't do graph traversal. Fuzzy search can't tell you who calls whom.

Codescope solves it the right way: parse the code into a **knowledge graph** — functions, calls, imports, type hierarchies, decisions, all of it — and let agents **walk the graph** instead of flipping through files.

```
Question: "Who calls parse_config transitively within 3 hops?"

Traditional RAG:        Codescope:
─────────────────       ─────────────────
~150K tokens            ~1-2K tokens
~12 seconds             ~3 ms (end-to-end)
Fuzzy text match        Deterministic edge walk
Guess confidence        Actual answer
```

---

## Install

| Platform | Command |
|----------|---------|
| **Linux / macOS** | `curl -fsSL https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.sh \| bash` |
| **Windows** | `irm https://raw.githubusercontent.com/onur-gokyildiz-bhi/codescope/main/install.ps1 \| iex` |
| **Homebrew** | `brew install onur-gokyildiz-bhi/codescope/codescope` |
| **Claude Code plugin** | `/plugin marketplace add onur-gokyildiz-bhi/codescope` then `/plugin install codescope@codescope` |
| **Build from source** | `cargo install --git https://github.com/onur-gokyildiz-bhi/codescope` |

Already installed? `codescope --version` to check. Update in-place with `codescope upgrade`.

Pre-built binaries: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`.

---

## Quick Start

```bash
# 1. Bring the bundled SurrealDB server up (idempotent)
codescope start

# 2. In your project — writes .mcp.json and indexes your code
cd your-project
codescope init

# That's it. Claude Code, Cursor, Codex — any MCP-compatible
# agent in this project now has codescope wired in.
```

**Target a different agent:**
```bash
codescope init --agent cursor          # .cursor/mcp.json
codescope init --agent gemini-cli      # ~/.gemini/settings.json
codescope init --agent vscode-copilot  # .vscode/mcp.json
codescope init --agent codex           # ~/.codex/config.toml
codescope init --agent windsurf        # ~/.codeium/windsurf/mcp_config.json
codescope init --agent kiro            # .kiro/settings/mcp.json
codescope init --agent cline           # .vscode/cline_mcp_settings.json
codescope init --agent antigravity     # global + GEMINI.md nudge
```

**Daemon mode (MCP + Web UI in one process):**
```bash
codescope init --daemon   # port 9877 — per-repo routing at /mcp/<repo>
# Web UI: http://localhost:9877/
```

**LSP mode (editor-agnostic — VS Code, Zed, Neovim, Helix, IntelliJ):**
```bash
codescope lsp
# Go-to-def, Find References, Hover, Workspace Symbols — all graph-backed.
```

**Daily operation:**
```bash
codescope status            # surreal server state
codescope gain              # cumulative token savings
codescope insight           # per-repo + hourly activity
codescope session           # last 5 MCP sessions with tails
codescope upgrade           # in-place self-update
codescope repair --repo <n> # drop + re-index a corrupted repo
codescope hook install      # PreToolUse bash-suggest for Claude Code
codescope doctor            # diagnose setup (+ --fix)
```

---

## What Your Agent Gets

A structured MCP surface your agent programs against, instead of scrolling output.

### Code navigation & impact
- `search(mode)` — fuzzy / exact / file / cross_type / neighborhood / backlinks
- `find_callers` / `find_callees` — 1-hop call graph
- `impact_analysis` — transitive BFS blast radius
- `type_hierarchy` — inheritance chains
- `context_bundle` — file overview with delta-mode caching (97% savings on repeat visits)

### Knowledge — memory that survives sessions
- `knowledge(action)` — save / search / link / lint; scopes `project` / `global` / `both`
- `memory(action)` — save / search / pin
- `capture_insight` — record decisions in real time
- `manage_adr` — Architecture Decision Records

### Git & temporal
- `code_health(mode)` — hotspots / churn / coupling / review_diff
- `sync_git_history` — pipe git log into the graph
- `contributors(mode)` — map / reviewers / patterns
- `conversations(action)` — index / search / timeline of assistant chat history

### Quality & refactor
- `lint(mode)` — dead_code / smells / custom SurrealQL rules
- `refactor(action)` — rename / find_unused / safe_delete
- `edit_preflight` — check edit against team patterns

### Semantic search & HTTP
- `semantic_search` — embedding-based fallback for natural language
- `ask` — decomposes questions into structured queries
- `http_analysis(mode)` — calls / endpoint_callers

### Tool output & shell output (CMX + RTK absorbed)
- `fetch_and_index(source)` — URL or local file → per-repo BM25 full-text
- `search_indexed(query)` — BM25 over indexed content
- `sandbox_run(language, code)` — python / node / bash subprocess, timeout + output cap
- `codescope exec <cmd>` — wraps cargo, pytest, npm, tsc, docker, git, grep, … and compresses output 80-95% (`--full` opts out)

Plus `raw_query` (SurrealQL escape hatch), `graph_stats`, `project(action)`, `skills(action)`, `export_obsidian`, `retrieve_archived`.

---

## Why Graph-First

Embeddings are fine for *"find something that means X"*. They're catastrophic for:

- *"What functions transitively depend on `parse_config`?"*
- *"If I change `User::email`, what tests break?"*
- *"Show me the full call graph 3 hops out from `main`."*
- *"Who implements this trait?"*

These are **graph traversal questions**. Vector search gives fuzzy matches; codescope gives an exact answer by walking indexed edges.

```
  EMBEDDINGS-FIRST                 GRAPH-FIRST (codescope)
  ─────────────────                ─────────────────────────
  parse → embed → vector DB        parse → entities + edges → SurrealDB
                                                              + embeddings (fallback)
  query: nearest neighbor          query: traverse edges + (optional) NN
  best at: semantic similarity     best at: structural reasoning
  blind to: call relationships     sees: who calls whom, blast radius,
           type hierarchies                type hierarchies, dependencies
```

Embeddings stay as a **secondary index** for natural-language queries where structure doesn't help. The **primary index is the graph** — the same way developers actually walk through code.

### Think in code, not in data

Treat your LLM as a code generator, not a data processor:

```
Without codescope:  Read main.rs + user.rs + … (40 files, 150K tokens)  → "I count 247 functions"
With codescope:     impact_analysis(User::email, depth=3) → {"callers": 12, "tests_affected": 3}
                    ↑ one query, 800 tokens, deterministic
```

Every codescope tool is a structured query — `find_callers`, `impact_analysis`, `knowledge_search`, `code_health` — that the LLM programs and the graph executes.

---

## Context Diet — 3 of 4 layers in one binary

Context waste comes in four flavours. Codescope covers three; pair it with **GSD** for the fourth:

| Layer | Covered by | How |
|-------|------------|-----|
| Workflow / planning | [GSD](https://github.com/gsd-build/get-shit-done) | Spec-driven pipeline: roadmap → phase → plan → execute → verify → ship |
| **Code semantics** | **codescope** MCP tools | Functions, callers, impact, decisions, conversations — graph traversal over code |
| **Generic tool output** | **codescope** (`fetch_and_index`, `search_indexed`, `sandbox_run`) | Ingest web / doc / log captures into per-repo BM25; run short snippets in a sandbox |
| **Shell output** | **codescope exec** | Wrap cargo / pytest / git / grep / docker / … — compressor per command (`--full` opts out) |

GSD's planning subagents automatically use codescope MCP tools when both are installed — see [`docs/integrations/gsd.md`](docs/integrations/gsd.md) for the pairing guide.

The `codescope hook --agent claude-code` command installs a PreToolUse nudge that routes matching Bash calls through `codescope exec` automatically.

---

## Supported Languages

**47 programming languages via tree-sitter:**
TypeScript · JavaScript · Python · **Rust** · Go · Java · C · C++ · C# · CUDA (`__global__` / `__device__` / kernel launches) · Ruby · PHP · Swift · Dart · Kotlin · Scala · Lua · Zig · Elixir · Haskell · OCaml · HTML · Julia · Bash · R · CSS · Erlang · Objective-C · HCL/Terraform · Nix · CMake · Makefile · Verilog · Fortran · GLSL · GraphQL · D · Solidity · GDScript · Elm · Groovy · Pascal · Ada · Common Lisp · Scheme · Racket · XML/SVG · Protobuf

**10 content formats via custom parsers:**
JSON · YAML · TOML · Markdown · Dockerfile · SQL · Terraform · OpenAPI · Gradle · .env

---

## Benchmarks

Re-benchmarked 2026-04-24 on the same 4 corpora against v0.8.11 (Windows 11, Rust 1.91.1, bundled SurrealDB 3.0.5 server, release build, bench tool with batched INSERTs):

| Project | Language | Files | Entities | Relations | Index |
|---------|----------|------:|---------:|----------:|------:|
| ripgrep | Rust     | 142   | 4,623    | 16,535    | 3.3s  |
| axum    | Rust     | 410   | 5,319    | 15,353    | 4.6s  |
| tokio   | Rust     | 819   | 13,776   | 45,548    | 11.2s |
| Gin     | Go       | 109   | 2,400    | 11,324    | 2.2s  |

**8-13× faster than the 2026-04-10 run** (ripgrep 36.9s → 3.3s, tokio 141.8s → 11.2s) — combined effect of the R1-v2 server migration and the batched-INSERT builder.

**Multi-hop traversal (end-to-end via `impact_analysis`):** 0.48–1.11 ms at depth 3 across all four repos (up to 45.5k edges). Graph traversal scales with edge fan-out, not corpus size.

**Token savings — sample rows:**

| Question | Repo | Traditional | Codescope | Saved |
|----------|------|:-----------:|:---------:|:-----:|
| Find function + context            | tokio   | 125,620 tokens   | 1,894 tokens | **98.5%** |
| List all structs                   | tokio   | 1,463,493 tokens | 1,183 tokens | **99.9%** |
| Impact analysis (callers+callees)  | ripgrep | 197,615 tokens   | 2,252 tokens | **98.9%** |
| Find largest functions             | axum    | 415,438 tokens   |   292 tokens | **99.9%** |

Full per-repo tables, competitive comparison, and methodology: [BENCHMARKS.md](BENCHMARKS.md)

---

## How It Plugs In

```
Your Code
    ↓
tree-sitter parsers (47 langs + 10 formats)
    ↓
SurrealDB knowledge graph
    │
    ├── Entities: function, class, file, import, package, config, doc, infra, knowledge
    ├── Relations: calls, contains, imports, implements, inherits, supports,
    │              contradicts, related_to, launches (CUDA kernels)
    └── Secondary: fastembed-rs vector embeddings for semantic_search
    ↓
3 interfaces, same graph:
    ├── MCP (stdio or HTTP daemon) — Claude Code, Cursor, Codex, Zed, …
    ├── LSP (stdio)                — VS Code, Neovim, Helix, IntelliJ
    └── Web UI (HTTP 9876 / 9877)  — 3D graph, knowledge panel, session timeline
```

### Multi-agent memory

Every connected agent sees the same graph. Decisions captured by one persist for the next.

```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│ Claude Code │  │   Cursor    │  │  Codex CLI  │
└──────┬──────┘  └──────┬──────┘  └──────┬──────┘
       │                │                │
       └────────────────┼────────────────┘
                        │
                ┌───────▼────────┐
                │ Codescope MCP  │
                └───────┬────────┘
                        │
                ┌───────▼────────┐
                │   SurrealDB    │
                │ Entities /     │
                │ Call graphs /  │
                │ Decisions /    │
                │ Embeddings     │
                └────────────────┘
```

---

## Codescope Is (and Isn't)

**Codescope is not an editor. Not an agent. Not a SaaS.**

It's a **context layer** — the brain behind whatever AI coding tool you already use. Plug it in via MCP or LSP and every connected client gets the same graph-backed memory.

```
┌──────────────────────────────────────────────────────┐
│   Editor / Agent (Claude Code, Cursor, Zed, ...)     │  ← you pick this
├──────────────────────────────────────────────────────┤
│   Context layer                                      │
│   ┌──────────────────────┐   ┌──────────────────────┐│
│   │ Built-in (embeddings)│   │ Codescope (graph)    ││  ← you can swap this
│   └──────────────────────┘   └──────────────────────┘│
├──────────────────────────────────────────────────────┤
│   Your code                                          │
└──────────────────────────────────────────────────────┘
```

So "codescope vs Cursor" is the wrong framing. **Codescope vs the built-in embeddings RAG** is the right one.

### vs built-in context engines

| Capability | **Codescope** | Cursor built-in | Windsurf built-in | Continue.dev | Claude Code skills |
|---|:---:|:---:|:---:|:---:|:---:|
| Architecture              | **Graph-first**             | Embeddings | Embeddings | Embeddings | File-reading |
| Call graph traversal      | **Native, single-digit ms** | ❌ | ❌ | ❌ | Read-based |
| Impact analysis (N-hop)   | **Native**                  | ❌ | ❌ | ❌ | ❌ |
| Type hierarchy queries    | **Native**                  | ❌ | ❌ | ❌ | ❌ |
| Cross-session memory      | **Shared across agents**    | Per-editor | ❌ | ❌ | Per-project files |
| Editor/agent lock-in      | **None — MCP + LSP**        | Cursor only | Windsurf only | Continue only | Claude only |
| Fully local               | **Yes**                     | ❌ (cloud indexing) | ❌ (cloud) | Yes | Yes |
| CUDA/GPU code-aware       | **Yes**                     | ❌ | ❌ | ❌ | ❌ |

**Honest positioning:** if you already love Cursor or Claude Code, don't switch. Add codescope as a second brain. If you're building your own agent, codescope handles context so you don't have to.

---

## Configuration

| Setting | Default | Override |
|---------|---------|----------|
| DB path       | `~/.codescope/db/<repo>/` | `--db-path` or `CODESCOPE_DB_PATH` |
| Web UI port   | `9876` | `--port` |
| Daemon port   | `9877` | `--port` |
| Embeddings    | FastEmbed (local) | `--provider ollama\|openai` |
| Log level     | `info` | `RUST_LOG=debug` |
| OTLP endpoint | off | `CODESCOPE_OTLP_ENDPOINT=http://localhost:4317` |

Set `CODESCOPE_OTLP_ENDPOINT` to export MCP tool invocations, graph queries, and cache-hit counters over OTLP (tested with Jaeger, Tempo, Honeycomb). Unset by default — zero overhead and zero network.

---

## Documentation

- [Quickstart](docs/quickstart.md) — step-by-step walkthrough
- [LLM Usage Guide](docs/llm-usage-guide.md) — tool selection patterns for AI agents
- [Troubleshooting](docs/troubleshooting.md) — common issues + fixes
- [Benchmarks](BENCHMARKS.md) — methodology and numbers
- [Contributing](CONTRIBUTING.md) — dev setup, test conventions
- [Architecture deep-dive](ARCHITECTURE.md) — graph schema and internals
- [Security](SECURITY.md) — threat model and disclosure policy

---

## Contributing

```bash
cargo test --workspace          # All tests
cargo clippy -- -D warnings     # Lint (strict)
cargo run -p codescope-bench    # Benchmarks
cargo fmt --all                 # Format (required before commit)
```

CI auto-formats on push to main; run `cargo fmt --all` locally to avoid the extra commit. See [CONTRIBUTING.md](CONTRIBUTING.md) for dev setup.

---

## Credits

- **Graph traversal:** SurrealDB
- **Parsing:** tree-sitter + its 47 language grammars
- **Embeddings:** FastEmbed-rs
- **MCP protocol:** rmcp
- **LSP server:** tower-lsp
- **3D visualization:** Three.js + 3d-force-graph + SolidJS

Inspired by:
- [Karpathy's LLM Wiki pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) — the wiki IS the product
- [Graph of Skills (ICLR 2026)](https://github.com/davidliuk/graph-of-skills) — PPR over typed edges
- [Relational Transformer (ICLR 2026)](https://github.com/snap-stanford/relational-transformer) — structure as attention mask

---

## License

MIT — [Onur Gokyildiz](https://github.com/onur-gokyildiz-bhi)

<div align="center">

**If codescope saves you an afternoon of context-switching, [star the repo](https://github.com/onur-gokyildiz-bhi/codescope).**

</div>

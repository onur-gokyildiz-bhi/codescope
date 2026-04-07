# Changelog

All notable changes to Codescope will be documented in this file.

## [Unreleased]

### Added
- CI: `cargo audit` security scanning via `rustsec/audit-check`
- CI: Strict clippy and test enforcement (no more `continue-on-error`)
- `clippy.toml` and `deny.toml` for lint and supply chain security config
- `Dockerfile` with multi-stage build for containerized deployment
- Pre-commit hook config for local quality gates
- `CHANGELOG.md` for tracking releases
- SHA256 checksums for release binaries

### Fixed
- `.mcp.json` hardcoded user paths — now portable across machines
- All MCP config templates standardized to use `codescope-mcp` binary
- 3 `unwrap()` calls in production code replaced with safe alternatives
- All clippy warnings resolved (was 39, now 0)
- Hardcoded test paths replaced with `CODESCOPE_TEST_JSONL_DIR` env var

## [0.4.0] - 2025-03-15

### Added
- 3D interactive web UI with force-directed graph visualization
- Type hierarchy analysis (`type_hierarchy` tool)
- Skill/knowledge graph support with wikilink navigation
- Conversation history panel with date filter and search
- Auto project insights after indexing
- File tree, hotspots, skills, timeline, minimap in web UI
- 45 MCP tools (up from 36)

### Changed
- Unified 3 binaries into single `codescope` executable (kept separate binaries for backward compat)
- Faster indexing with parallel file collection

### Fixed
- Repo name derived from target path instead of CWD
- False-positive CLAUDE.md check in insights
- File tree nested entity array flattening

## [0.3.0] - 2025-02-20

### Added
- 35 language support (up from 10)
- HTTP endpoint linking and caller tracing
- Conversation memory: indexes Claude Code session transcripts
- Binary quantization for 32x memory-efficient semantic search
- Symbol rename and safe delete operations
- Dead code detection
- File watcher for live re-indexing
- Progressive disclosure in search results
- One-line install scripts (`install.ps1`, `install.sh`)
- `codescope init` command for zero-config setup
- 5 agent configs (Claude Code, Cursor, Zed, Codex CLI, Gemini CLI)

### Changed
- Switched to SurrealKV backend (from RocksDB)
- Optimized binary size with release profile tuning

### Fixed
- Call graph resolution for same-file callees
- DB lock limitation with SurrealKV migration
- 19 performance and memory issues across query engine

## [0.2.0] - 2025-01-30

### Added
- Team patterns and contributor mapping
- Edit preflight checks
- ADR (Architecture Decision Records) management
- Community detection in code graphs
- Memory and visualization tools
- Daemon mode (SSE server for multi-project)

## [0.1.0] - 2025-01-15

### Added
- Initial release
- Code parsing with tree-sitter (10 languages)
- SurrealDB knowledge graph storage
- MCP server for Claude Code integration
- Semantic search with FastEmbed
- `find_callers`, `find_callees`, `impact_analysis`
- `context_bundle`, `explore`, `search_functions`
- Git history sync and file churn analysis
- Benchmark suite

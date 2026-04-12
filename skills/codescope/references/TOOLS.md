# Codescope MCP Tools Reference

Complete list of 52 MCP tools grouped by category. Each tool is available when the codescope MCP server is running.

## Tool Selection Guide

Before calling a tool, use this decision tree:

1. **Know the exact function name?** → `find_function(name)`
2. **Fuzzy/partial name?** → `search_functions(query)`
3. **Who calls X?** → `find_callers(function_name)` (1 hop)
4. **What breaks if I change X?** → `impact_analysis(function_name, depth=3)` (N hops)
5. **Full context of an entity?** → `explore(entity_name)`
6. **File overview before editing?** → `context_bundle(file_path)`
7. **Type inheritance?** → `type_hierarchy(name)`
8. **Free-text question?** → `semantic_search(query)` or `ask(question)`

## Search & Navigation

| Tool | Params | Description |
|------|--------|-------------|
| `search_functions` | `query`, `limit?`, `scope?` | Fuzzy/substring search for functions. Case-insensitive. |
| `find_function` | `name` | Exact name lookup. Returns full details. |
| `file_entities` | `file_path` | All functions and classes in a file. |
| `find_callers` | `function_name` | Direct (1-hop) callers of a function. |
| `find_callees` | `function_name` | Direct (1-hop) callees of a function. |
| `graph_stats` | — | Entity and relationship counts. |
| `raw_query` | `query` | Raw SurrealQL. See [SURREALQL.md](../cs-query/references/SURREALQL.md). |
| `supported_languages` | — | List of 59 supported languages/formats. |

## Call Graph & Impact

| Tool | Params | Description |
|------|--------|-------------|
| `impact_analysis` | `function_name`, `depth?` | Transitive BFS through the call graph (default depth=3, max 5). Sub-10ms. |
| `type_hierarchy` | `name`, `depth?` | Parents, subtypes, implementors, interfaces for a type. |
| `find_dead_code` | `scope?` | Functions with zero callers. |
| `find_unused` | `scope?` | Unused imports and symbols. |

## Exploration

| Tool | Params | Description |
|------|--------|-------------|
| `explore` | `entity_name` | Full neighborhood: callers, callees, siblings, file, configs. |
| `context_bundle` | `file_path` | Complete file map with cross-file callers. Use BEFORE reading a file. |
| `related` | `entity_name` | Cross-graph search: code, configs, docs, packages. |
| `backlinks` | `entity_name` | Everything that points TO this entity (reverse references). |

## Semantic Search

| Tool | Params | Description |
|------|--------|-------------|
| `semantic_search` | `query`, `limit?` | Cosine similarity search over function embeddings (BQ + rerank). |
| `embed_functions` | `scope?` | Trigger embedding pipeline for functions. |

## HTTP & Endpoints

| Tool | Params | Description |
|------|--------|-------------|
| `find_http_calls` | `scope?` | All HTTP client calls in the codebase. |
| `find_endpoint_callers` | `query` | Who calls a specific API endpoint. |

## Quality & Analysis

| Tool | Params | Description |
|------|--------|-------------|
| `detect_code_smells` | `scope?` | Long functions, deep nesting, excessive params. |
| `custom_lint` | `rules` | Custom lint rules in JSON format. |
| `hotspot_detection` | — | Files with highest churn + complexity. |
| `change_coupling` | — | Files that change together (temporal coupling). |
| `file_churn` | `path?` | Git change frequency for files. |
| `community_detection` | — | Module clusters based on call graph. |
| `review_diff` | `diff?` | Structural review of a git diff. |
| `edit_preflight` | `file_path`, `function_name?` | Check before editing: callers, tests, impact. |
| `safe_delete` | `entity_name` | Verify no references before deleting. |
| `api_changelog` | — | Track API surface changes. |

## Refactoring

| Tool | Params | Description |
|------|--------|-------------|
| `rename_symbol` | `old_name`, `new_name` | Find all references for a rename. |
| `suggest_structure` | `scope?` | Suggest file/module reorganization. |
| `suggest_reviewers` | `file_path?` | Who knows this code best (git blame analysis). |

## Memory & Decisions

| Tool | Params | Description |
|------|--------|-------------|
| `memory_save` | `content`, `kind?` | Save an insight to the knowledge graph. |
| `memory_search` | `query` | Search saved insights and decisions. |
| `memory_pin` | `content`, `scope?` | Pin important context for the session. |
| `capture_insight` | `content`, `kind` | Record a decision, problem, or correction. |
| `manage_adr` | `action`, `title?`, `content?` | Architecture Decision Records (list/create/update). |

## Conversations

| Tool | Params | Description |
|------|--------|-------------|
| `conversation_search` | `query` | Search past Claude Code session transcripts. |
| `conversation_timeline` | `days?` | Timeline of recent sessions. |
| `index_conversations` | — | Re-index conversation history. |

## Skills & Knowledge

| Tool | Params | Description |
|------|--------|-------------|
| `generate_skill_notes` | — | Auto-generate a navigable knowledge base. |
| `index_skill_graph` | — | Build the skill/knowledge graph. |
| `traverse_skill_graph` | `start`, `depth?` | Walk the skill graph from a starting point. |
| `export_obsidian` | `output_path?` | Export knowledge graph to an Obsidian vault with wikilinks. |

## Git & Temporal

| Tool | Params | Description |
|------|--------|-------------|
| `sync_git_history` | — | Import git history into the knowledge graph. |
| `contributor_map` | `path?` | Who contributed to which files/modules. |
| `team_patterns` | — | Team coding patterns and conventions. |

## Admin

| Tool | Params | Description |
|------|--------|-------------|
| `init_project` | `path`, `repo`, `auto_index?` | Initialize a project in daemon mode. |
| `list_projects` | — | List open projects. |
| `index_codebase` | `path?` | Re-index the codebase. |
| `ask` | `question` | Natural language question (Turkish/English) → graph query. |
